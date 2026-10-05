using System.ComponentModel;
using System.Runtime.CompilerServices;
using System.Text;
using System.Windows.Input;
using System.Windows.Threading;
using ThinkVisionControl.Core.Control;
using ThinkVisionControl.Core.Logging;
using ThinkVisionControl.Core.Profile;
using ThinkVisionControl.Core.Settings;
using ThinkVisionControl.Ui.Controls;

namespace ThinkVisionControl.Ui;

public abstract class Observable : INotifyPropertyChanged
{
    public event PropertyChangedEventHandler? PropertyChanged;

    protected void OnChanged([CallerMemberName] string? name = null) => PropertyChanged?.Invoke(this, new PropertyChangedEventArgs(name));

    protected bool Set<T>(ref T field, T value, [CallerMemberName] string? name = null)
    {
        if (EqualityComparer<T>.Default.Equals(field, value)) return false;
        field = value;
        OnChanged(name);
        return true;
    }
}

public sealed class RelayCommand(Action<object?> run) : ICommand
{
    public event EventHandler? CanExecuteChanged { add { } remove { } }
    public bool CanExecute(object? parameter) => true;
    public void Execute(object? parameter) => run(parameter);
}

public sealed record InputChoice(byte Value, string Label)
{
    public override string ToString() => Label;
}

/// <summary>A titled group of settings cards on the Monitor settings page.</summary>
public sealed record SettingsGroup(string Header, IReadOnlyList<ControlItem> Items);

/// <summary>
/// State shared by the main window and the tray flyout; it outlives both. Every card is built from
/// <see cref="ControlService.Controls"/> (never empty), monitor cards are usable only while <see cref="IsConnected"/>,
/// and one InfoBar explains any other state.
/// </summary>
public sealed class PanelViewModel : Observable
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
        (0xE0, "Picture", "\uE916", "Reduces motion blur. Extreme can leave trails behind moving objects"),
        (0xEA, "Picture", "\uE793", "Adjusts contrast to match what's on screen"),
        (0x86, "Picture", "\uE740", "The monitor greys this out at its native resolution and may ignore changes"),
        (0xEC, "Ports", "\uE88E", "Charge devices from the monitor's USB ports while it's in standby"),
        (0xCC, "On-screen menu", "\uE774", "The language of the monitor's own menus"),
    ];
    static readonly string[] SectionOrder = ["Picture", "Ports", "On-screen menu", OtherSection];

    readonly ControlService _control;
    readonly SettingsStore _settingsStore;
    readonly Func<IReadOnlyCollection<HotkeyAction>> _failedHotkeys;
    readonly Action<AppSettings> _settingsChanged;
    readonly Dispatcher _dispatcher = Dispatcher.CurrentDispatcher;
    readonly Dictionary<byte, ControlItem> _items = [];
    readonly DispatcherTimer _holdTimer, _retryTimer;
    IReadOnlyDictionary<byte, ControlDefinition>? _builtFrom;
    AppSettings _settings;
    MonitorStatus _status;
    bool _barOpen, _barProgress, _retrying;
    InfoBarSeverity _barSeverity;
    string? _barTitle, _barMessage, _barAction, _restoreNote, _settingsError;

    public PanelViewModel(ControlService control, SettingsStore settingsStore, ProfileStore profiles,
        Func<IReadOnlyCollection<HotkeyAction>> failedHotkeys, Action<AppSettings> settingsChanged)
    {
        _control = control;
        _settingsStore = settingsStore;
        Profiles = profiles;
        _failedHotkeys = failedHotkeys;
        _settingsChanged = settingsChanged;
        _settings = settingsStore.Load();
        _status = control.Status;

        _holdTimer = new DispatcherTimer(StateHold, DispatcherPriority.Normal, (_, _) => ShowStateBar(), _dispatcher) { IsEnabled = false };
        _retryTimer = new DispatcherTimer(RetryProgress, DispatcherPriority.Normal, (_, _) => EndRetry(), _dispatcher) { IsEnabled = false };
        InfoBarCommand = new RelayCommand(_ => CheckAgain());
        StandbyCommand = new RelayCommand(_ => { if (PowerItem is { } p) _ = WriteAsync(p, PowerStandby); });
        TurnOffCommand = new RelayCommand(_ => { if (PowerItem is { } p) _ = WriteAsync(p, PowerOff); });
        RestoreBackupCommand = new RelayCommand(_ => _ = RestoreBackupAsync());
        Day = new PresetItem("Day", "\uE706", this, night: false);
        Night = new PresetItem("Night", "\uE708", this, night: true);
        Hotkeys = Enum.GetValues<HotkeyAction>().Select(a => new HotkeyItem(a, HotkeyLabel(a))).ToList();

        control.ValueChanged += (code, value) => _dispatcher.InvokeAsync(() => OnValueChanged(code, value));
        control.StatusChanged += status => _dispatcher.InvokeAsync(() => OnStatusChanged(status));

        Rebuild();
        RebuildHotkeys();
        // The Connecting bar only appears if connecting takes longer than the hold time.
        if (_status == MonitorStatus.Connecting) _holdTimer.Start();
        else ShowStateBar();
    }

    internal ControlService Control => _control;
    internal ProfileStore Profiles { get; }

    /// <summary>
    /// Asks the user to confirm a guarded write (the window sets this to its ContentDialogHost). With no confirmer
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
        MonitorStatus.Connected => "\uE73E",
        MonitorStatus.Connecting => "\uE895",
        MonitorStatus.NotFound => "\uF140",
        _ => "\uE783",
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
    public InfoBarSeverity InfoBarSeverity { get => _barSeverity; private set => Set(ref _barSeverity, value); }
    public string? InfoBarTitle { get => _barTitle; private set => Set(ref _barTitle, value); }
    public string? InfoBarMessage { get => _barMessage; private set => Set(ref _barMessage, value); }
    public string? InfoBarAction { get => _barAction; private set => Set(ref _barAction, value); }
    public bool InfoBarProgress { get => _barProgress; private set => Set(ref _barProgress, value); }
    public ICommand InfoBarCommand { get; }

    /// <summary>The firmware level the monitor reports in its capability string (0x04's value list), if any.</summary>
    string? Firmware =>
        _control.Capabilities.Vcp.TryGetValue(FactoryReset, out var bytes) && bytes is { Count: >= 2 } && bytes.All(b => b is >= 0x20 and < 0x7F)
            ? Encoding.ASCII.GetString([.. bytes])
            : null;

    void OnStatusChanged(MonitorStatus status)
    {
        if (status == _status) return;
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
        switch (_status)
        {
            case MonitorStatus.Connected:
                IsInfoBarOpen = false;
                return;
            case MonitorStatus.Connecting:
                SetBar(InfoBarSeverity.Informational, "Looking for your monitor…", null, null);
                break;
            case MonitorStatus.NotFound:
                SetBar(InfoBarSeverity.Informational, $"{MonitorName} isn't connected", "Connect it and we'll find it automatically.", "Check again");
                break;
            default:
                SetBar(InfoBarSeverity.Error, "Your monitor isn't responding",
                    "DDC/CI might be off. Hold the monitor's Exit key for 10 seconds, then try again.", "Try again");
                break;
        }
        InfoBarProgress = _status == MonitorStatus.Connecting || _retrying;
        IsInfoBarOpen = true;
    }

    void SetBar(InfoBarSeverity severity, string title, string? message, string? action)
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
    public string RestoreBackupDescription => _restoreNote ?? "Put back the settings saved before the last reset";
    public PresetItem Day { get; }
    public PresetItem Night { get; }

    /// <summary>Rebuilds every card from <see cref="ControlService.Controls"/> and the value cache.</summary>
    void Rebuild()
    {
        var controls = _control.Controls;
        _builtFrom = controls;
        _items.Clear();

        BrightnessItem = Fixed(Brightness, "\uE706") as RangeItem;
        ContrastItem = Fixed(Contrast, "\uE7A1") as RangeItem;
        ColourModeItem = Fixed(ColourMode, "\uE790", "Changes apply straight away") as EnumItem;
        RgbItems = RgbGains.Select(code => Fixed(code, null)).OfType<RangeItem>().ToList();
        InputItem = Fixed(Input, "\uE961") as EnumItem;
        PowerItem = Fixed(Power, "\uE7E8", "Put the monitor in standby or turn it off") as EnumItem;

        var placed = controls.Values
            .Where(d => Visible(d) && d.Kind != ControlKind.Action && !DisplayCodes.Contains(d.Code) && d.Code != Power)
            .Select(d => (Def: d, Index: Array.FindIndex(KnownSettings, k => k.Code == d.Code)))
            .OrderBy(x => x.Index < 0 ? int.MaxValue : x.Index)
            .ThenBy(x => x.Def.Code)
            .ToList();
        MonitorGroups = SectionOrder
            .Select(section => new SettingsGroup(section, placed
                .Where(x => (x.Index < 0 ? OtherSection : KnownSettings[x.Index].Section) == section)
                .Select(x => x.Index < 0 ? Create(x.Def, "\uE9E9", null) : Create(x.Def, KnownSettings[x.Index].Icon, KnownSettings[x.Index].Description))
                .ToList()))
            .Where(g => g.Items.Count > 0)
            .ToList();
        ResetItems = controls.Values
            .Where(d => Visible(d) && d.Kind == ControlKind.Action)
            .OrderBy(d => d.Code)
            .Select(d => (ActionItem)Create(d, null, d.Code == FactoryReset ? "Restores every on-screen-menu setting. We save a backup first" : null))
            .ToList();

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

    /// <summary>Re-reads the Display page codes (the user may have used the monitor's own buttons). Never rebuilds.</summary>
    public void OnShowing() => _ = RefreshAsync(DisplayCodes.Where(_items.ContainsKey));

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
        if (InputItem is not { } input || !IsConnected || option == input.Selected) return Task.CompletedTask;
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
        _restoreNote = result switch
        {
            null or { Outcome: WriteOutcome.NotResponding } => null,
            { Outcome: WriteOutcome.Ok } => "Backup restored",
            _ => result.Message,
        };
        OnChanged(nameof(RestoreBackupDescription));
        SyncAll();
    }

    async Task<WriteResult?> Guard(Func<Task<WriteResult>> write)
    {
        try { return await write(); }
        catch (Exception ex)
        {
            FileLog.Write($"Write failed: {ex.Message}");
            return null;
        }
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
            foreach (var name in new[] { nameof(ThisMachineInput), nameof(InputDescription), nameof(StatusText), nameof(StatusShortText) })
                OnChanged(name);
            InputItem?.RefreshThisPc(value);
        }
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

    public bool StartWithWindows
    {
        get => _settings.StartWithWindows;
        set
        {
            if (value == _settings.StartWithWindows || !SaveSettings(_settings with { StartWithWindows = value })) return;
            OnChanged();
        }
    }

    public string StartWithWindowsDescription => BackgroundMode
        ? "Starts in the background when you sign in"
        : "Opens the window when you sign in";

    public string ShortcutsDescription => BackgroundMode
        ? "Shortcuts work whenever ThinkVision Control is running."
        : "Shortcuts work while Run in background is on. Turn it on to use them.";

    public string AppVersion => typeof(PanelViewModel).Assembly.GetName().Version is { } v ? $"Version {v.Major}.{v.Minor}.{v.Build}" : "";

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

    /// <summary>Re-reads which shortcuts failed to register (call after hotkeys were re-registered).</summary>
    internal void RefreshHotkeyWarnings()
    {
        var failed = FailedHotkeys();
        foreach (var item in Hotkeys) item.IsInUse = failed.Contains(item.Action);
    }

    /// <summary>The label of another action that already uses <paramref name="text"/>, or null.</summary>
    internal string? HotkeyConflict(HotkeyAction action, string text) =>
        text.Length == 0 ? null
            : Hotkeys.FirstOrDefault(h => h.Action != action && string.Equals(h.Text, text, StringComparison.OrdinalIgnoreCase))?.Label;

    internal static string DefaultHotkey(HotkeyAction action) => SettingsStore.Defaults.Hotkeys.GetValueOrDefault(action, "");

    /// <summary>Saves one shortcut; an empty <paramref name="text"/> clears it.</summary>
    internal bool SaveHotkey(HotkeyAction action, string text)
    {
        var map = new Dictionary<HotkeyAction, string>(_settings.Hotkeys) { [action] = text };
        if (!SaveSettings(_settings with { Hotkeys = map })) return false;
        Hotkeys.First(h => h.Action == action).Text = text;
        return true;
    }

    IReadOnlyCollection<HotkeyAction> FailedHotkeys()
    {
        if (!BackgroundMode) return [];
        try { return _failedHotkeys(); }
        catch { return []; }
    }

    internal static string HotkeyLabel(HotkeyAction action) => action switch
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

/// <summary>One monitor setting shown as a card.</summary>
public abstract class ControlItem(PanelViewModel owner, ControlDefinition definition, string? icon, string? description) : Observable
{
    string? _note;

    protected PanelViewModel Owner { get; } = owner;
    public ControlDefinition Definition { get; } = definition;
    public byte Code => Definition.Code;
    public string Name => Definition.Name;
    public string? Icon { get; } = icon;

    /// <summary>Why the last change didn't take (until the next successful change), else the card's description.</summary>
    public string? Description => _note ?? description;

    internal string? Note
    {
        get => _note;
        set { if (Set(ref _note, value)) OnChanged(nameof(Description)); }
    }

    /// <summary>Shows a value read from the monitor (<see cref="uint.MaxValue"/> = unknown). Never writes.</summary>
    public abstract void Sync(uint value, uint max);
}

/// <summary>
/// A slider. Dragging writes live (<see cref="ControlService.DragTo"/>, no read-back); keyboard and click changes are
/// debounced, then written once. Monitor updates are ignored while a change of ours is in progress.
/// </summary>
public sealed class RangeItem(PanelViewModel owner, ControlDefinition definition, string? icon, string? description)
    : ControlItem(owner, definition, icon, description)
{
    static readonly TimeSpan Debounce = TimeSpan.FromMilliseconds(300);

    uint _value, _max = 100;
    bool _known, _dragging;
    int _settling;
    DispatcherTimer? _debounce;

    public double Max => _max;

    public double Value
    {
        get => _value;
        set => UserSet(value);
    }

    public string ValueText => Owner.IsConnected && _known ? _value.ToString() : "—";

    bool Busy => _dragging || _settling > 0 || _debounce?.IsEnabled == true;

    void UserSet(double value)
    {
        var v = (uint)Math.Clamp(Math.Round(value), 0, _max);
        if (v == _value && _known) return;
        _value = v;
        _known = true;
        OnChanged(nameof(Value));
        OnChanged(nameof(ValueText));
        if (!Owner.IsConnected) return;

        if (_dragging)
        {
            Owner.Control.DragTo(Code, v);
            return;
        }
        _debounce ??= new DispatcherTimer(Debounce, DispatcherPriority.Normal, (_, _) => DebounceElapsed(), Dispatcher.CurrentDispatcher);
        _debounce.Stop();
        _debounce.Start();
    }

    internal void BeginDrag()
    {
        if (!Owner.IsConnected || _dragging) return;
        _debounce?.Stop(); // the drag's final write covers a pending keyboard change
        _dragging = true;
        Owner.Control.BeginDrag(Code);
    }

    internal void EndDrag()
    {
        if (!_dragging) return;
        _dragging = false;
        _ = FinishAsync();
    }

    void DebounceElapsed()
    {
        _debounce!.Stop();
        if (_dragging || !Owner.IsConnected) return;
        Owner.Control.BeginDrag(Code);
        _ = FinishAsync();
    }

    async Task FinishAsync()
    {
        WriteResult? result = null;
        _settling++;
        try { result = await Owner.Control.EndDragAsync(Code, _value); }
        catch (Exception ex) { ThinkVisionControl.Core.Logging.FileLog.Write($"Slider write 0x{Code:X2} failed: {ex.Message}"); }
        finally { _settling--; }

        if (Busy) return; // a newer change is on its way
        Owner.SyncFromCache(this);
        Note = result switch
        {
            { Outcome: WriteOutcome.Ok } => null,
            { Outcome: WriteOutcome.Blocked } when result.Message.Contains("remains") =>
                $"Your monitor kept {Name.ToLowerInvariant()} at {_value}" + (Code == PanelViewModel.Brightness ? " — the current colour mode may lock it" : ""),
            { Outcome: WriteOutcome.Blocked } => result.Message,
            _ => Note,
        };
    }

    public override void Sync(uint value, uint max)
    {
        if (max > 0 && max != _max)
        {
            _max = max;
            OnChanged(nameof(Max));
        }
        if (Busy) return;
        var known = value != uint.MaxValue;
        if (known && value != _value)
        {
            _value = value;
            OnChanged(nameof(Value));
        }
        if (known != _known) _known = known;
        OnChanged(nameof(ValueText));
    }

    internal void OnConnectionChanged() => OnChanged(nameof(ValueText));
}

public sealed class EnumOption(byte value, string label) : Observable
{
    bool _isActive, _isThisPc;

    public byte Value { get; } = value;
    public string Label { get; } = label;
    public bool IsActive => _isActive;

    /// <summary>For the input selector: this option is this PC's input.</summary>
    public bool IsThisPc { get => _isThisPc; internal set => Set(ref _isThisPc, value); }

    // Always raised, so a toggle the user flipped snaps back to the real state.
    internal void SetActive(bool active)
    {
        _isActive = active;
        OnChanged(nameof(IsActive));
    }

    public override string ToString() => Label;
}

/// <summary>A choice (ComboBox; the input selector shows its options as radio buttons or tiles).</summary>
public class EnumItem : ControlItem
{
    EnumOption? _selected;

    public EnumItem(PanelViewModel owner, ControlDefinition definition, IReadOnlyList<byte> values, string? icon, string? description)
        : base(owner, definition, icon, description)
    {
        Options = values
            .Select(v => new EnumOption(v, definition.ValueLabels.TryGetValue(v, out var label) ? label : $"0x{v:X2}"))
            .ToList();
        RefreshThisPc(owner.ThisMachineInput);
    }

    public IReadOnlyList<EnumOption> Options { get; }

    public EnumOption? Selected
    {
        get => _selected;
        set { if (value is not null && value != _selected) Choose(value); }
    }

    protected void Choose(EnumOption option)
    {
        if (option == _selected) return;
        Mark(option);
        _ = Owner.WriteAsync(this, option.Value);
    }

    internal void Mark(EnumOption? option)
    {
        _selected = option;
        OnChanged(nameof(Selected));
        foreach (var o in Options) o.SetActive(o == option);
        OnMarked();
    }

    protected virtual void OnMarked() { }

    internal void RefreshThisPc(byte thisPc)
    {
        if (Code != PanelViewModel.Input) return;
        foreach (var o in Options) o.IsThisPc = o.Value == thisPc;
    }

    public override void Sync(uint value, uint max) => Mark(Options.FirstOrDefault(o => o.Value == value));
}

/// <summary>An Off/On choice shown as a ToggleSwitch.</summary>
public sealed class ToggleItem(PanelViewModel owner, ControlDefinition definition, IReadOnlyList<byte> values, string? icon, string? description)
    : EnumItem(owner, definition, values, icon, description)
{
    public bool IsOn
    {
        get => Selected?.Value == 1;
        set
        {
            if (Options.FirstOrDefault(o => o.Value == (value ? 1 : 0)) is { } option) Choose(option);
            else OnChanged();
        }
    }

    protected override void OnMarked() => OnChanged(nameof(IsOn));
}

/// <summary>A one-shot command such as a reset.</summary>
public sealed class ActionItem : ControlItem
{
    public ActionItem(PanelViewModel owner, ControlDefinition definition, string? icon, string? description)
        : base(owner, definition, icon, description) =>
        Run = new RelayCommand(_ => _ = Owner.WriteAsync(this, 1));

    public ICommand Run { get; }

    public override void Sync(uint value, uint max) { }
}

/// <summary>The Day or Night preset card.</summary>
public sealed class PresetItem : Observable
{
    string _values = "";
    string? _note;

    internal PresetItem(string name, string icon, PanelViewModel owner, bool night)
    {
        Name = name;
        Icon = icon;
        IsNight = night;
        ApplyCommand = new RelayCommand(_ => _ = owner.ApplyPresetAsync(this));
        SaveCommand = new RelayCommand(_ => owner.SavePreset(this));
    }

    public string Name { get; }
    public string Icon { get; }
    internal bool IsNight { get; }
    public string SaveText => $"Save current settings as {Name}";
    public ICommand ApplyCommand { get; }
    public ICommand SaveCommand { get; }
    public string Description => _note ?? _values;

    internal string Values
    {
        set { _values = value; OnChanged(nameof(Description)); }
    }

    internal string? Note
    {
        set { _note = value; OnChanged(nameof(Description)); }
    }
}

public sealed class HotkeyItem(HotkeyAction action, string label) : Observable
{
    string _text = "";
    bool _isInUse;

    public HotkeyAction Action { get; } = action;
    public string Label { get; } = label;

    public string Text
    {
        get => _text;
        internal set
        {
            if (!Set(ref _text, value)) return;
            OnChanged(nameof(Keys));
            OnChanged(nameof(IsSet));
            OnChanged(nameof(EditName));
        }
    }

    public IReadOnlyList<string> Keys => KeysOf(_text);
    public bool IsSet => _text.Length > 0;

    /// <summary>Registration failed: another app owns this shortcut.</summary>
    public bool IsInUse
    {
        get => _isInUse;
        internal set { if (Set(ref _isInUse, value)) OnChanged(nameof(Warning)); }
    }

    public string? Warning => _isInUse ? "Another app is using this shortcut" : null;

    public string EditName => $"Edit shortcut for {Label}, currently {(IsSet ? string.Join(" + ", Keys) : "none")}";

    /// <summary>Keycap labels for a shortcut such as "Ctrl+Alt+Up".</summary>
    internal static IReadOnlyList<string> KeysOf(string text) =>
        text.Length == 0
            ? []
            : text.Split('+').Select(k => k switch { "Up" => "↑", "Down" => "↓", "Left" => "←", "Right" => "→", _ => k }).ToList();
}
