using System.Text;
using System.Windows.Input;
using Avalonia.Threading;
using FluentAvalonia.UI.Controls;
using ThinkVisionControl.Avalonia.Platform;
using ThinkVisionControl.Core.Control;
using ThinkVisionControl.Core.Ddc;
using ThinkVisionControl.Core.Logging;
using ThinkVisionControl.Core.Profile;
using ThinkVisionControl.Core.Settings;

namespace ThinkVisionControl.Avalonia.ViewModels;

/// <summary>
/// State shared by the main window and the tray flyout; it outlives both. Every card is built from
/// <see cref="ControlService.Controls"/> (never empty), monitor cards are usable only while <see cref="IsConnected"/>,
/// and one InfoBar explains any other state. Port of the WPF <c>PanelViewModel</c>.
/// </summary>
public sealed class MainViewModel : Observable
{
    internal const byte Brightness = 0x10, Contrast = 0x12, ColourMode = 0x14, Input = 0x60, Power = 0xD6;
    const byte CustomColourMode = 0x12, PowerStandby = 0x04, PowerOff = 0x05, FactoryReset = 0x04;
    const string MonitorName = "ThinkVision P32p-20", OtherSection = "More settings";
    static readonly byte[] RgbGains = [0x16, 0x18, 0x1A];
    static readonly byte[] DisplayCodes = [Brightness, Contrast, ColourMode, 0x16, 0x18, 0x1A, Input];
    static readonly byte[] ColourModeOrder = [0x0B, 0x01, 0x0F, 0x06, 0x0E, 0x05, 0x08, 0x12];
    static readonly TimeSpan StateHold = TimeSpan.FromSeconds(1), RetryProgress = TimeSpan.FromSeconds(16);

    /// <summary>Monitor settings page: where each known code goes. Other visible codes (named in the finder) go to "More settings".</summary>
    static readonly (byte Code, string Section, string Icon, string? Description)[] KnownSettings =
    [
        (0xE0, "Picture", Glyphs.OverDrive, "Reduces motion blur. Extreme can leave trails behind moving objects"),
        (0xEA, "Picture", Glyphs.DynamicContrast, "Adjusts contrast to match what's on screen"),
        (0x86, "Picture", Glyphs.AspectRatio, "The monitor greys this out at its native resolution and may ignore changes"),
        (0xEC, "Ports", Glyphs.Usb, "Charge devices from the monitor's USB ports while it's in standby"),
        (0xCC, "On-screen menu", Glyphs.Language, "The language of the monitor's own menus"),
    ];
    static readonly string[] SectionOrder = ["Picture", "Ports", "On-screen menu", OtherSection];

    readonly ControlService _control;
    readonly SettingsStore _settingsStore;
    readonly PlatformServices _services;
    readonly Action<AppSettings> _settingsChanged;
    readonly Dictionary<byte, ControlItem> _items = [];
    readonly DispatcherTimer _holdTimer, _retryTimer;
    IReadOnlyDictionary<byte, ControlDefinition>? _builtFrom;
    IReadOnlyCollection<HotkeyAction> _failedHotkeys = [];
    IReadOnlyList<SetupHint> _setupHints = [];
    AppSettings _settings;
    MonitorStatus _status;
    LinkFailure _reason;
    bool _barOpen, _barProgress, _retrying;
    FAInfoBarSeverity _barSeverity;
    string? _barTitle, _barMessage, _barAction, _settingsError;
    readonly RestoreRow _restoreRow;

    public MainViewModel(ControlService control, SettingsStore settingsStore, ProfileStore profiles,
        PlatformServices services, Action<AppSettings> settingsChanged)
    {
        _control = control;
        _settingsStore = settingsStore;
        Profiles = profiles;
        _services = services;
        _settingsChanged = settingsChanged;
        _settings = settingsStore.Load();
        _status = control.Status;
        _reason = control.StatusReason;

        _holdTimer = new DispatcherTimer { Interval = StateHold };
        _holdTimer.Tick += (_, _) => ShowStateBar();
        _retryTimer = new DispatcherTimer { Interval = RetryProgress };
        _retryTimer.Tick += (_, _) => EndRetry();
        InfoBarCommand = new RelayCommand(_ => CheckAgain());
        StandbyCommand = new RelayCommand(_ => { if (PowerItem is { } p) _ = WriteAsync(p, PowerStandby); });
        TurnOffCommand = new RelayCommand(_ => { if (PowerItem is { } p) _ = WriteAsync(p, PowerOff); });
        RestoreBackupCommand = new RelayCommand(_ => _ = RestoreBackupAsync());
        _restoreRow = new RestoreRow(RestoreBackupCommand);
        Day = new PresetItem("Day", Glyphs.Day, this, night: false);
        Night = new PresetItem("Night", Glyphs.Night, this, night: true);
        Hotkeys = Enum.GetValues<HotkeyAction>().Select(a => new HotkeyItem(a, HotkeyLabel(a))).ToList();

        control.ValueChanged += (code, value) => Dispatcher.UIThread.Post(() => OnValueChanged(code, value));
        control.StatusChanged += status => Dispatcher.UIThread.Post(() => OnStatusChanged(status));

        Rebuild();
        RebuildHotkeys();
        DetectSetup();
        // The Connecting bar only appears if connecting takes longer than the hold time.
        if (_status == MonitorStatus.Connecting) _holdTimer.Start();
        else ShowStateBar();
    }

    internal ControlService Control => _control;
    internal ProfileStore Profiles { get; }

    /// <summary>
    /// Asks the user to confirm a guarded write (the main window sets this to its ContentDialog). With no confirmer
    /// the write is cancelled. A view can pass its own confirmer to <see cref="SelectInputAsync"/> instead.
    /// </summary>
    public Func<DialogSpec, Task<bool>>? ConfirmAsync { get; set; }

    /// <summary>Raised by <see cref="RequestOpenSettings"/> (the flyout's "All settings").</summary>
    public event Action? OpenSettingsRequested;

    public void RequestOpenSettings() => OpenSettingsRequested?.Invoke();

    // ---------------------------------------------------------------- connection state

    public MonitorStatus Status => _status;
    public bool IsConnected => _status == MonitorStatus.Connected;
    public string MonitorTitle => MonitorName;

    public string StatusGlyph => _status switch
    {
        MonitorStatus.Connected => Glyphs.Connected,
        MonitorStatus.Connecting => Glyphs.Connecting,
        MonitorStatus.NotFound => Glyphs.NotFound,
        _ => Glyphs.NotResponding,
    };

    public string StatusText => _status switch
    {
        MonitorStatus.Connected => $"Connected over {InputLabel(ThisMachineInput)}" + (Firmware is { } f ? $" · Firmware {f}" : ""),
        _ => StatusShortText,
    };

    public string StatusShortText => _status switch
    {
        MonitorStatus.Connected => $"Connected · {InputLabel(ThisMachineInput)}",
        MonitorStatus.Connecting => "Looking for your monitor…",
        MonitorStatus.NotFound => "Not connected",
        _ => "Not responding",
    };

    public bool IsInfoBarOpen { get => _barOpen; private set => Set(ref _barOpen, value); }
    public FAInfoBarSeverity InfoBarSeverity { get => _barSeverity; private set => Set(ref _barSeverity, value); }
    public string? InfoBarTitle { get => _barTitle; private set => Set(ref _barTitle, value); }
    public string? InfoBarMessage { get => _barMessage; private set => Set(ref _barMessage, value); }

    public string? InfoBarAction
    {
        get => _barAction;
        private set { if (Set(ref _barAction, value)) OnChanged(nameof(HasInfoBarAction)); }
    }

    public bool HasInfoBarAction => _barAction is not null;
    public bool InfoBarProgress { get => _barProgress; private set => Set(ref _barProgress, value); }
    public ICommand InfoBarCommand { get; }

    /// <summary>The firmware level the monitor reports in its capability string (0x04's value list), if any.</summary>
    string? Firmware =>
        _control.Capabilities.Vcp.TryGetValue(FactoryReset, out var bytes) && bytes is { Count: >= 2 } && bytes.All(b => b is >= 0x20 and < 0x7F)
            ? Encoding.ASCII.GetString([.. bytes])
            : null;

    void OnStatusChanged(MonitorStatus status)
    {
        // The reason can change without the status (e.g. permission fixed, module still missing).
        var reason = _control.StatusReason;
        var reasonChanged = reason != _reason;
        _reason = reason;
        DetectSetup();
        if (status == _status)
        {
            if (reasonChanged && _barOpen) ShowStateBar();
            return;
        }

        _status = status;
        EndRetry();
        foreach (var name in new[] { nameof(Status), nameof(IsConnected), nameof(StatusGlyph), nameof(StatusText), nameof(StatusShortText), nameof(AboutMonitorText) })
            OnChanged(name);
        foreach (var range in _items.Values.OfType<RangeItem>()) range.OnConnectionChanged();

        if (status == MonitorStatus.Connected)
        {
            // Capabilities read on connect (or a finder edit) can change the controls: rebuild, then fill gaps.
            if (!ReferenceEquals(_builtFrom, _control.Controls)) Rebuild();
            else SyncAll();
            _ = RefreshAsync(_items.Values.Where(i => i is not ActionItem && !_control.TryGetCached(i.Code, out _)).Select(i => i.Code));
            ShowStateBar();
        }
        else
        {
            // Hold a new state for a moment before the bar changes, so it never flashes.
            _holdTimer.Stop();
            _holdTimer.Start();
        }
    }

    void ShowStateBar()
    {
        _holdTimer.Stop();
        const string SeeSetup = " See App settings → System setup.";
        switch (_status)
        {
            case MonitorStatus.Connected:
                IsInfoBarOpen = false;
                return;
            case MonitorStatus.Connecting:
                SetBar(FAInfoBarSeverity.Informational, "Looking for your monitor…", null, null);
                break;
            // A known OS-level cause beats the generic text: it says what to fix and where.
            case not MonitorStatus.Connecting when _reason == LinkFailure.PermissionDenied:
                SetBar(FAInfoBarSeverity.Warning, "This app isn't allowed to use the monitor controls",
                    "Your system blocks access to the monitor's DDC/CI bus." + SeeSetup, "Try again");
                break;
            case not MonitorStatus.Connecting when _reason == LinkFailure.ModuleMissing:
                SetBar(FAInfoBarSeverity.Warning, "Monitor controls aren't turned on in your system",
                    "The i2c-dev kernel module isn't loaded, so the monitor can't be reached." + SeeSetup, "Try again");
                break;
            case not MonitorStatus.Connecting when _reason == LinkFailure.UnsupportedPort:
                SetBar(FAInfoBarSeverity.Warning, "This port can't control the monitor",
                    "Connect the monitor to a USB-C or Thunderbolt port." + SeeSetup, "Try again");
                break;
            case MonitorStatus.NotFound:
                SetBar(FAInfoBarSeverity.Informational, $"{MonitorName} isn't connected", "Connect it and we'll find it automatically.", "Check again");
                break;
            default:
                SetBar(FAInfoBarSeverity.Error, "Your monitor isn't responding",
                    "DDC/CI might be off. Hold the monitor's Exit key for 10 seconds, then try again.", "Try again");
                break;
        }
        InfoBarProgress = _status == MonitorStatus.Connecting || _retrying;
        IsInfoBarOpen = true;
    }

    void SetBar(FAInfoBarSeverity severity, string title, string? message, string? action)
    {
        InfoBarSeverity = severity;
        InfoBarTitle = title;
        InfoBarMessage = message;
        InfoBarAction = action;
    }

    void CheckAgain()
    {
        _control.RequestReconnect();
        _retrying = true;
        InfoBarProgress = true;
        _retryTimer.Stop();
        _retryTimer.Start();
    }

    void EndRetry()
    {
        _retryTimer.Stop();
        _retrying = false;
        InfoBarProgress = _status == MonitorStatus.Connecting;
    }

    // ---------------------------------------------------------------- controls

    public RangeItem? BrightnessItem { get; private set; }
    public RangeItem? ContrastItem { get; private set; }
    public EnumItem? ColourModeItem { get; private set; }
    public IReadOnlyList<RangeItem> RgbItems { get; private set; } = [];
    public EnumItem? InputItem { get; private set; }
    public bool HasInput => InputItem is not null;
    public bool HasColourMode => ColourModeItem is not null;
    public EnumItem? PowerItem { get; private set; }
    public IReadOnlyList<SettingsGroup> MonitorGroups { get; private set; } = [];
    public IReadOnlyList<ActionItem> ResetItems { get; private set; } = [];

    public bool IsCustomColour => _control.TryGetCached(ColourMode, out var r) && r.Current == CustomColourMode;
    public string CustomColourDescription => IsCustomColour ? "Red, green and blue gain" : "Available in User / Custom colour mode";
    public string InputDescription => $"This PC uses {InputLabel(ThisMachineInput)}";
    public bool HasPower => PowerItem is not null;
    public bool CanStandby => PowerItem?.Options.Any(o => o.Value == PowerStandby) == true;
    public bool CanTurnOff => PowerItem?.Options.Any(o => o.Value == PowerOff) == true;
    public ICommand StandbyCommand { get; }
    public ICommand TurnOffCommand { get; }
    public ICommand RestoreBackupCommand { get; }
    public string? RestoreBackupDescription => _restoreRow.Description;

    /// <summary>"Reset and restore" rows: the monitor's reset actions, then Restore backup.</summary>
    public IReadOnlyList<IActionRow> ResetRows { get; private set; } = [];
    public PresetItem Day { get; }
    public PresetItem Night { get; }

    /// <summary>Rebuilds every card from <see cref="ControlService.Controls"/> and the value cache.</summary>
    void Rebuild()
    {
        var controls = _control.Controls;
        _builtFrom = controls;
        _items.Clear();

        BrightnessItem = Fixed(Brightness, Glyphs.Brightness) as RangeItem;
        ContrastItem = Fixed(Contrast, Glyphs.Contrast) as RangeItem;
        ColourModeItem = Fixed(ColourMode, Glyphs.ColourMode, "Changes apply straight away") as EnumItem;
        RgbItems = RgbGains.Select(code => Fixed(code, null)).OfType<RangeItem>().ToList();
        InputItem = Fixed(Input, Glyphs.Input) as EnumItem;
        PowerItem = Fixed(Power, Glyphs.Power, "Put the monitor in standby or turn it off") as EnumItem;

        var placed = controls.Values
            .Where(d => Visible(d) && d.Kind != ControlKind.Action && !DisplayCodes.Contains(d.Code) && d.Code != Power)
            .Select(d => (Def: d, Index: Array.FindIndex(KnownSettings, k => k.Code == d.Code)))
            .OrderBy(x => x.Index < 0 ? int.MaxValue : x.Index)
            .ThenBy(x => x.Def.Code)
            .ToList();
        MonitorGroups = SectionOrder
            .Select(section => new SettingsGroup(section, placed
                .Where(x => (x.Index < 0 ? OtherSection : KnownSettings[x.Index].Section) == section)
                .Select(x => x.Index < 0 ? Create(x.Def, Glyphs.Options, null) : Create(x.Def, KnownSettings[x.Index].Icon, KnownSettings[x.Index].Description))
                .ToList()))
            .Where(g => g.Items.Count > 0)
            .ToList();
        ResetItems = controls.Values
            .Where(d => Visible(d) && d.Kind == ControlKind.Action)
            .OrderBy(d => d.Code)
            .Select(d => (ActionItem)Create(d, null, d.Code == FactoryReset ? "Restores every on-screen-menu setting. We save a backup first" : null))
            .ToList();
        ResetRows = [.. ResetItems, _restoreRow];

        SyncAll();
        RefreshPresets();
        OnChanged(string.Empty);
    }

    ControlItem? Fixed(byte code, string? icon, string? description = null) =>
        _control.Controls.TryGetValue(code, out var def) && Visible(def) ? Create(def, icon, description) : null;

    static bool Visible(ControlDefinition d) =>
        d.Section != PanelSection.Hidden && d.Safety is not (SafetyClass.NeverWrite or SafetyClass.ReadOnlyUntilNamed);

    ControlItem Create(ControlDefinition def, string? icon, string? description)
    {
        ControlItem item = def.Kind switch
        {
            ControlKind.Range => new RangeItem(this, def, icon, description),
            ControlKind.Action => new ActionItem(this, def, icon, description),
            _ => AllowedValues(def) is var values && IsOnOff(def, values)
                ? new ToggleItem(this, def, values, icon, description)
                : new EnumItem(this, def, values, icon, description),
        };
        _items[def.Code] = item;
        return item;
    }

    IReadOnlyList<byte> AllowedValues(ControlDefinition def)
    {
        var caps = _control.Capabilities;
        if (def.Code == ColourMode)
            return ColourModeOrder.Where(value => caps.AllowsValue(ColourMode, value)).ToList();

        var values = caps.Vcp.TryGetValue(def.Code, out var list) && list is not null ? list : def.ValueLabels.Keys.Order().ToList();
        // Inputs always read HDMI, DisplayPort, USB-C, whatever order the capabilities list them in.
        return def.Code == Input
            ? values.OrderBy(v => v switch { 0x11 => 0, 0x0F => 1, 0x31 => 2, _ => 3 }).ToList()
            : values;
    }

    static bool IsOnOff(ControlDefinition def, IReadOnlyList<byte> values) =>
        def.ValueLabels.Count == 2 && def.ValueLabels.GetValueOrDefault((byte)0) == "Off" && def.ValueLabels.GetValueOrDefault((byte)1) == "On"
        && values.All(v => v <= 1);

    /// <summary>
    /// The window is about to show: re-reads the Display page codes (the user may have used the monitor's own buttons)
    /// and re-checks the system setup. Never rebuilds.
    /// </summary>
    public void OnShowing()
    {
        DetectSetup();
        _ = RefreshAsync(DisplayCodes.Where(_items.ContainsKey));
    }

    /// <summary>Rebuilds the cards if the profile changed (a code was named in the finder).</summary>
    internal void RebuildIfControlsChanged()
    {
        if (ReferenceEquals(_builtFrom, _control.Controls)) return;
        Rebuild();
        _ = RefreshAsync(_items.Values.Where(i => i is not ActionItem && !_control.TryGetCached(i.Code, out _)).Select(i => i.Code));
    }

    async Task RefreshAsync(IEnumerable<byte> codes)
    {
        var list = codes.ToList();
        if (!IsConnected || list.Count == 0) return;
        try { await Task.Run(() => _control.RefreshAsync(list)); }
        catch (Exception ex) { FileLog.Write($"Refresh failed: {ex.Message}"); }
    }

    void OnValueChanged(byte code, uint value)
    {
        if (_items.TryGetValue(code, out var item))
            item.Sync(value, _control.TryGetCached(code, out var r) ? r.Max : 0);
        if (code == ColourMode) OnColourModeChanged();
    }

    void OnColourModeChanged()
    {
        OnChanged(nameof(IsCustomColour));
        OnChanged(nameof(CustomColourDescription));
    }

    void SyncAll()
    {
        foreach (var item in _items.Values) SyncFromCache(item);
        OnColourModeChanged();
    }

    internal void SyncFromCache(ControlItem item)
    {
        if (_control.TryGetCached(item.Code, out var r)) item.Sync(r.Current, r.Max);
        else item.Sync(uint.MaxValue, 0); // unknown: clear any selection the user just made
    }

    uint? Cached(byte code) => _control.TryGetCached(code, out var r) ? r.Current : null;

    /// <summary>Selects an input from a view; switching away from this PC's input asks <paramref name="confirm"/> first.</summary>
    public Task SelectInputAsync(EnumOption option, Func<DialogSpec, Task<bool>>? confirm = null)
    {
        if (InputItem is not { } input || !IsConnected || option == input.Selected)
        {
            // Snap a clicked radio button / tile back to the real state.
            InputItem?.Mark(InputItem.Selected);
            return Task.CompletedTask;
        }
        input.Mark(option);
        return WriteAsync(input, option.Value, confirm);
    }

    internal async Task WriteAsync(ControlItem item, uint value, Func<DialogSpec, Task<bool>>? confirm = null)
    {
        var confirmed = false;
        if (ConfirmationFor(item.Definition, value) is { } spec)
        {
            confirm ??= ConfirmAsync;
            if (confirm is null || !await confirm(spec))
            {
                SyncFromCache(item);
                return;
            }
            confirmed = true;
        }

        var result = await Guard(() => _control.SetAsync(item.Code, value, confirmed));
        item.Note = result is { Outcome: WriteOutcome.Blocked or WriteOutcome.NeedsConfirmation } ? result.Message : null;
        SyncFromCache(item);
        if (item.Code == ColourMode) OnColourModeChanged();
        // A reset changes many values (and can turn DDC/CI off): re-read everything.
        if (item is ActionItem && result?.Outcome == WriteOutcome.Ok)
            await RefreshAsync(_items.Values.Where(i => i is not ActionItem).Select(i => i.Code));
    }

    DialogSpec? ConfirmationFor(ControlDefinition def, uint value)
    {
        const string Cancel = "Cancel";
        return def.Code switch
        {
            Input when value != ThisMachineInput => new($"Switch to {InputLabel((byte)value)}?",
                "You won't be able to control the monitor from this PC until you switch back.", "Switch", null, Cancel),
            FactoryReset when def.Kind == ControlKind.Action => new("Reset monitor to factory settings?",
                "This restores every on-screen-menu setting, and it can turn off DDC/CI on some Lenovo monitors. We save a backup first.",
                "Reset", null, Cancel),
            0x05 when def.Kind == ControlKind.Action => new("Reset brightness and contrast?", null, "Reset", null, Cancel),
            0x08 when def.Kind == ControlKind.Action => new("Reset colour settings?", null, "Reset", null, Cancel),
            Power when value == PowerOff => new($"Turn off {MonitorName}?",
                "You might need the monitor's power button to turn it back on.", "Turn off", null, Cancel),
            Power when value == PowerStandby => new($"Put {MonitorName} in standby?",
                "You might need the monitor's power button to wake it.", "Standby", null, Cancel),
            _ when def.Safety == SafetyClass.ConfirmRequired => new(def.ConfirmMessage ?? $"Change {def.Name}?", null, "Continue", null, Cancel),
            _ => null,
        };
    }

    internal async Task ApplyPresetAsync(PresetItem item)
    {
        var result = await Guard(() => _control.ApplyPresetAsync(item.IsNight ? _settings.Night : _settings.Day));
        item.Note = result is null or { Outcome: WriteOutcome.Ok or WriteOutcome.NotResponding } ? null : result.Message;
        SyncAll();
    }

    internal void SavePreset(PresetItem item)
    {
        var old = item.IsNight ? _settings.Night : _settings.Day;
        var preset = old with
        {
            Brightness = Cached(Brightness) ?? old.Brightness,
            Contrast = Cached(Contrast) ?? old.Contrast,
            ColourMode = Cached(ColourMode) is { } mode ? (byte)mode : old.ColourMode,
        };
        if (SaveSettings(item.IsNight ? _settings with { Night = preset } : _settings with { Day = preset }))
        {
            item.Note = null;
            RefreshPresets();
        }
    }

    void RefreshPresets()
    {
        Day.Values = Describe(_settings.Day);
        Night.Values = Describe(_settings.Night);
    }

    string Describe(Preset preset)
    {
        var colour = preset.ColourMode is { } mode && ColourModeItem?.Options.FirstOrDefault(o => o.Value == mode) is { } option
            ? $", {option.Label}"
            : "";
        return $"Brightness {preset.Brightness}, contrast {preset.Contrast}{colour}";
    }

    async Task RestoreBackupAsync()
    {
        var result = await Guard(() => _control.RestoreLatestBackupAsync());
        _restoreRow.Note = result switch
        {
            null or { Outcome: WriteOutcome.NotResponding } => null,
            { Outcome: WriteOutcome.Ok } => "Backup restored",
            _ => result.Message,
        };
        OnChanged(nameof(RestoreBackupDescription));
        SyncAll();
    }

    static async Task<WriteResult?> Guard(Func<Task<WriteResult>> write)
    {
        try { return await write(); }
        catch (Exception ex)
        {
            FileLog.Write($"Write failed: {ex.Message}");
            return null;
        }
    }

    // ---------------------------------------------------------------- system setup

    /// <summary>One-time OS setup steps (App settings → System setup); empty hides the section.</summary>
    public IReadOnlyList<SetupHint> SetupHints
    {
        get => _setupHints;
        private set
        {
            _setupHints = value;
            OnChanged();
            OnChanged(nameof(HasSetupHints));
        }
    }

    public bool HasSetupHints => _setupHints.Count > 0;

    void DetectSetup()
    {
        IReadOnlyList<SetupHint> hints;
        try { hints = _services.Setup.Detect(_control.StatusReason, _services.Hotkeys.IsAvailable, _services.Tray.IsHostAvailable); }
        catch (Exception ex)
        {
            FileLog.Write($"Setup detection failed: {ex.Message}");
            hints = [];
        }
        if (hints.Count == _setupHints.Count && hints.Zip(_setupHints).All(p => p.First == p.Second)) return;
        SetupHints = hints;
    }

    // ---------------------------------------------------------------- app settings

    public IReadOnlyList<InputChoice> InputChoices { get; } =
        [new(0x11, "HDMI"), new(0x0F, "DisplayPort"), new(0x31, "USB-C")];

    string InputLabel(byte value) =>
        InputChoices.FirstOrDefault(c => c.Value == value)?.Label
        ?? (_control.Controls.TryGetValue(Input, out var def) && def.ValueLabels.TryGetValue(value, out var label) ? label : $"0x{value:X2}");

    public byte ThisMachineInput
    {
        get => _settings.ThisMachineInput;
        set
        {
            if (value == _settings.ThisMachineInput || !SaveSettings(_settings with { ThisMachineInput = value })) return;
            foreach (var name in new[] { nameof(ThisMachineInput), nameof(ThisMachineChoice), nameof(InputDescription), nameof(StatusText), nameof(StatusShortText) })
                OnChanged(name);
            InputItem?.RefreshThisPc(value);
        }
    }

    /// <summary><see cref="ThisMachineInput"/> as a ComboBox item.</summary>
    public InputChoice? ThisMachineChoice
    {
        get => InputChoices.FirstOrDefault(c => c.Value == _settings.ThisMachineInput);
        set { if (value is not null) ThisMachineInput = value.Value; }
    }

    public bool BackgroundMode
    {
        get => _settings.BackgroundMode;
        set
        {
            if (value == _settings.BackgroundMode || !SaveSettings(_settings with { BackgroundMode = value })) return;
            OnChanged();
            OnChanged(nameof(StartWithWindowsDescription));
            OnChanged(nameof(ShortcutsDescription));
            RefreshHotkeyWarnings();
        }
    }

    public string BackgroundDescription => OperatingSystem.IsMacOS()
        ? "Keep the menu bar icon and keyboard shortcuts running after you close the window"
        : "Keep the tray icon and keyboard shortcuts running after you close the window";

    public bool StartWithWindows
    {
        get => _settings.StartWithWindows;
        set
        {
            if (value == _settings.StartWithWindows || !SaveSettings(_settings with { StartWithWindows = value })) return;
            OnChanged();
        }
    }

    public string StartWithWindowsLabel => OperatingSystem.IsWindows() ? "Start with Windows" : "Start when you log in";

    public string StartWithWindowsDescription => (BackgroundMode ? "Starts in the background" : "Opens the window")
        + (OperatingSystem.IsWindows() ? " when you sign in" : " when you log in");

    public bool HotkeysAvailable => _services.Hotkeys.IsAvailable;

    public string ShortcutsDescription =>
        !HotkeysAvailable ? "This desktop doesn't let apps register global shortcuts. See System setup above."
        : BackgroundMode ? "Shortcuts work whenever ThinkVision Control is running."
        : "Shortcuts work while Run in background is on. Turn it on to use them.";

    public string AppVersion => typeof(MainViewModel).Assembly.GetName().Version is { } v ? $"Version {v.Major}.{v.Minor}.{v.Build}" : "";

    public string AboutMonitorText => Firmware is { } f ? $"{MonitorName} · Firmware {f}" : MonitorName;

    /// <summary>Settings could not be saved (shown on the App settings page).</summary>
    public string? SettingsError
    {
        get => _settingsError;
        private set { _settingsError = value; OnChanged(); OnChanged(nameof(HasSettingsError)); }
    }

    public bool HasSettingsError => _settingsError is not null;

    public IReadOnlyList<HotkeyItem> Hotkeys { get; }

    void RebuildHotkeys()
    {
        foreach (var item in Hotkeys)
            item.Text = _settings.Hotkeys.TryGetValue(item.Action, out var text) ? text : "";
        RefreshHotkeyWarnings();
    }

    /// <summary>
    /// The shell passes back the actions <see cref="IHotkeys.Register"/> couldn't register (after every
    /// <c>settingsChanged</c>); their cards say another app is using the shortcut.
    /// </summary>
    public void SetFailedHotkeys(IReadOnlyCollection<HotkeyAction> failed)
    {
        _failedHotkeys = failed;
        RefreshHotkeyWarnings();
    }

    void RefreshHotkeyWarnings()
    {
        // Unavailable hotkeys fail wholesale; that's the setup card's job, not a per-shortcut warning.
        var failed = BackgroundMode && HotkeysAvailable ? _failedHotkeys : [];
        foreach (var item in Hotkeys) item.IsInUse = failed.Contains(item.Action);
    }

    /// <summary>The label of another action that already uses <paramref name="text"/>, or null.</summary>
    public string? HotkeyConflict(HotkeyAction action, string text) =>
        text.Length == 0 ? null
            : Hotkeys.FirstOrDefault(h => h.Action != action && string.Equals(h.Text, text, StringComparison.OrdinalIgnoreCase))?.Label;

    public static string DefaultHotkey(HotkeyAction action) => SettingsStore.Defaults.Hotkeys.GetValueOrDefault(action, "");

    /// <summary>Releases the global shortcuts while one is being edited, so its keys reach the editor.</summary>
    public void SuspendHotkeys()
    {
        try { _services.Hotkeys.Suspend(); }
        catch (Exception ex) { FileLog.Write($"Suspending hotkeys failed: {ex.Message}"); }
    }

    /// <summary>Ends <see cref="SuspendHotkeys"/>: the shell re-registers on <c>settingsChanged</c>.</summary>
    public void ResumeHotkeys() => _settingsChanged(_settings);

    /// <summary>Saves one shortcut; an empty <paramref name="text"/> clears it.</summary>
    public bool SaveHotkey(HotkeyAction action, string text)
    {
        var map = new Dictionary<HotkeyAction, string>(_settings.Hotkeys) { [action] = text };
        if (!SaveSettings(_settings with { Hotkeys = map })) return false;
        Hotkeys.First(h => h.Action == action).Text = text;
        return true;
    }

    public static string HotkeyLabel(HotkeyAction action) => action switch
    {
        HotkeyAction.BrightnessUp => "Brightness up",
        HotkeyAction.BrightnessDown => "Brightness down",
        HotkeyAction.ContrastUp => "Contrast up",
        HotkeyAction.ContrastDown => "Contrast down",
        HotkeyAction.DayPreset => "Day preset",
        HotkeyAction.NightPreset => "Night preset",
        HotkeyAction.InputHdmi => "Input HDMI",
        HotkeyAction.InputDp => "Input DisplayPort",
        HotkeyAction.InputUsbC => "Input USB-C",
        HotkeyAction.SwitchToMe => "Switch to this PC",
        HotkeyAction.OpenPanel => "Open ThinkVision Control",
        _ => action.ToString(),
    };

    bool SaveSettings(AppSettings settings)
    {
        try
        {
            _settingsStore.Save(settings);
            _settings = settings;
            _settingsChanged(settings);
            SettingsError = null;
            return true;
        }
        catch (Exception ex)
        {
            SettingsError = "Couldn't save settings: " + ex.Message;
            return false;
        }
    }
}
