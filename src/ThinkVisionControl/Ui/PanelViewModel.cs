using System.Collections.ObjectModel;
using System.ComponentModel;
using System.Runtime.CompilerServices;
using System.Windows;
using System.Windows.Input;
using System.Windows.Threading;
using ThinkVisionControl.Core.Control;
using ThinkVisionControl.Core.Profile;
using ThinkVisionControl.Core.Settings;

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

public sealed class PanelViewModel : Observable
{
    const byte Brightness = 0x10, Contrast = 0x12, ColourMode = 0x14, Input = 0x60, CustomColourMode = 0x12;
    static readonly byte[] RgbGains = [0x16, 0x18, 0x1A];
    static readonly byte[] ColourModeOrder = [0x0B, 0x01, 0x0F, 0x06, 0x0E, 0x05, 0x08, 0x12];
    const string NotFoundText = "ThinkVision P32p-20 not connected — plug it in and the app connects automatically.";
    const string NotRespondingText = "Monitor not responding — DDC/CI may be off (hold Exit for 10 s)";
    const string InUseText = "In use by another app";

    readonly ControlService _control;
    readonly SettingsStore _settingsStore;
    readonly Func<IReadOnlyCollection<HotkeyAction>> _failedHotkeys;
    readonly Action<AppSettings> _settingsChanged;
    readonly Dispatcher _dispatcher = Dispatcher.CurrentDispatcher;
    readonly Dictionary<byte, ControlItem> _items = [];
    readonly List<string> _busyOperations = [];
    AppSettings _settings;
    bool _isResponding;
    MonitorStatus _monitorStatus;
    bool _isConnecting;
    string _status = "";
    bool _statusIsError;
    int _activeOperations;
    string _busyMessage = "";
    bool _controlsBuilt;

    public PanelViewModel(ControlService control, SettingsStore settingsStore, ProfileStore profiles,
        Func<IReadOnlyCollection<HotkeyAction>> failedHotkeys, Action<AppSettings> settingsChanged)
    {
        _control = control;
        _settingsStore = settingsStore;
        Profiles = profiles;
        _failedHotkeys = failedHotkeys;
        _settingsChanged = settingsChanged;
        _settings = settingsStore.Load();
        _monitorStatus = control.Status;

        control.ValueChanged += (code, value) => _dispatcher.InvokeAsync(() => OnValueChanged(code, value));
        control.RespondingChanged += responding => _dispatcher.InvokeAsync(() => OnRespondingChanged(responding));
        control.StatusChanged += status => _dispatcher.InvokeAsync(() => OnStatusChanged(status));
    }

    internal ControlService Control => _control;
    internal ProfileStore Profiles { get; }

    /// <summary>Asks the user to confirm a guarded write. The window replaces this to own the dialog.</summary>
    internal Func<string, bool> Confirm { get; set; } = message =>
        MessageBox.Show(message, "ThinkVision Control", MessageBoxButton.YesNo, MessageBoxImage.Warning, MessageBoxResult.No)
            == MessageBoxResult.Yes;

    public ObservableCollection<ControlItem> MainItems { get; } = [];
    public ObservableCollection<ControlItem> AdvancedItems { get; } = [];
    public ObservableCollection<ControlItem> ActionItems { get; } = [];
    public ObservableCollection<HotkeyItem> Hotkeys { get; } = [];

    public IReadOnlyList<InputChoice> InputChoices { get; } =
        [new(0x11, "HDMI"), new(0x0F, "DisplayPort"), new(0x31, "USB-C")];

    public bool IsResponding
    {
        get => _isResponding;
        private set { if (Set(ref _isResponding, value)) OnChanged(nameof(RespondingText)); }
    }

    public string RespondingText => _monitorStatus switch
    {
        MonitorStatus.NotFound => NotFoundText,
        MonitorStatus.NotResponding => NotRespondingText,
        _ => "",
    };

    /// <summary>Shown inside the empty controls card, so it never looks blank.</summary>
    public string EmptyStateText => _monitorStatus switch
    {
        MonitorStatus.Connecting => "Looking for the monitor…",
        MonitorStatus.NotFound => "No ThinkVision P32p-20 found. Connect it with HDMI, DisplayPort or USB-C; the controls appear as soon as it is detected.",
        MonitorStatus.NotResponding => "The monitor is connected but not answering. If the Exit button was held for 10 seconds, DDC/CI is off — hold Exit for 10 seconds again to turn it back on.",
        _ => "Reading monitor settings…",
    };

    public bool ShowEmptyState => MainItems.Count == 0;

    public bool IsBusy => _activeOperations > 0 || _isConnecting;
    public string BusyMessage
    {
        get => _isConnecting ? "Connecting to monitor…" : _busyMessage;
        private set => Set(ref _busyMessage, value);
    }

    public string Status { get => _status; private set => Set(ref _status, value); }
    public bool StatusIsError { get => _statusIsError; private set => Set(ref _statusIsError, value); }

    public byte ThisMachineInput
    {
        get => _settings.ThisMachineInput;
        set
        {
            if (value == _settings.ThisMachineInput) return;
            SaveSettings(_settings with { ThisMachineInput = value });
            OnChanged();
        }
    }

    public bool StartWithWindows
    {
        get => _settings.StartWithWindows;
        set
        {
            if (value == _settings.StartWithWindows) return;
            SaveSettings(_settings with { StartWithWindows = value });
            OnChanged();
        }
    }

    public bool BackgroundMode
    {
        get => _settings.BackgroundMode;
        set
        {
            if (value == _settings.BackgroundMode) return;
            if (SaveSettings(_settings with { BackgroundMode = value }))
                OnChanged();
        }
    }

    /// <summary>Rebuilds every control from the current profile and cache, then refreshes the Main codes in the background.</summary>
    public void OnShowing()
    {
        Rebuild();
        RefreshInBackground(MainItems);
    }

    public void OnAdvancedExpanded() => RefreshInBackground(AdvancedItems);

    internal void SetConnecting(bool connecting)
    {
        if (_isConnecting == connecting) return;
        _isConnecting = connecting;
        OnChanged(nameof(IsBusy));
        OnChanged(nameof(BusyMessage));
    }

    void OnStatusChanged(MonitorStatus status)
    {
        _monitorStatus = status;
        OnChanged(nameof(RespondingText));
        OnChanged(nameof(EmptyStateText));
    }

    void OnRespondingChanged(bool responding)
    {
        IsResponding = responding;
        if (!responding || _controlsBuilt || _control.Capabilities is null) return;

        Rebuild();
        RefreshInBackground(MainItems.Concat(AdvancedItems));
    }

    void Rebuild()
    {
        MainItems.Clear();
        AdvancedItems.Clear();
        ActionItems.Clear();
        _items.Clear();
        SetStatus("", false);
        IsResponding = _control.IsResponding;

        var caps = _control.Capabilities;
        _controlsBuilt = caps is not null;
        if (caps is not null)
        {
            var visible = _control.Controls.Values
                .Where(d => d.Section != PanelSection.Hidden && caps.Supports(d.Code)
                            && d.Safety is not (SafetyClass.NeverWrite or SafetyClass.ReadOnlyUntilNamed))
                // Input sits right under brightness/contrast, colour mode is followed by its RGB gains.
                .OrderBy(d => d.Code == Input ? 0x13 : d.Code);

            foreach (var def in visible)
            {
                ControlItem item = def.Kind switch
                {
                    ControlKind.Range => new RangeItem(this, def),
                    ControlKind.Action => new ActionItem(this, def),
                    _ when def.Code == Input => new ToggleRowItem(this, def, AllowedValues(caps, def)),
                    _ => new EnumItem(this, def, AllowedValues(caps, def)),
                };
                _items[def.Code] = item;
                (item is ActionItem ? ActionItems : def.Section == PanelSection.Main ? MainItems : AdvancedItems).Add(item);
            }
        }

        OnChanged(nameof(ShowEmptyState));
        SyncAll();
        RebuildHotkeys();
        OnChanged(nameof(ThisMachineInput));
        OnChanged(nameof(StartWithWindows));
    }

    static IReadOnlyList<byte> AllowedValues(MonitorCapabilities caps, ControlDefinition def)
    {
        if (def.Code == ColourMode)
            return ColourModeOrder.Where(value => caps.AllowsValue(ColourMode, value)).ToList();

        return caps.Vcp.TryGetValue(def.Code, out var values) && values is not null
            ? values
            : def.ValueLabels.Keys.Order().ToList();
    }

    void RefreshInBackground(IEnumerable<ControlItem> items)
    {
        var codes = items.Where(i => i is not ActionItem).Select(i => i.Code).ToList();
        if (codes.Count > 0) _ = RefreshAsync(codes);
    }

    async Task RefreshAsync(List<byte> codes)
    {
        BeginBusy("Reading monitor settings…");
        try { await Task.Run(() => _control.RefreshAsync(codes)); }
        catch { /* surfaced through RespondingChanged */ }
        finally
        {
            SyncAll();
            EndBusy("Reading monitor settings…");
        }
    }

    void OnValueChanged(byte code, uint value)
    {
        if (!_items.TryGetValue(code, out var item)) return;
        item.Sync(value, _control.TryGetCached(code, out var r) ? r.Max : 0);
        if (code == ColourMode) UpdateRgbVisibility();
    }

    internal async Task ApplyRangeAsync(RangeItem range)
    {
        if (!range.TryStartApply(out var value)) return;

        var success = false;
        var busyMessage = $"Applying {range.Name}…";
        BeginBusy(busyMessage);
        try
        {
            var result = await Guard(() => _control.SetAsync(range.Code, value));
            if (result?.Outcome == WriteOutcome.NeedsConfirmation)
                result = Confirm(result.Message) ? await Guard(() => _control.SetAsync(range.Code, value, confirmed: true)) : null;

            Report(result);
            success = result?.Outcome == WriteOutcome.Ok;
        }
        finally
        {
            range.FinishApply(success);
            SyncFromCache(range);
            EndBusy(busyMessage);
        }
    }

    void SyncAll()
    {
        foreach (var item in _items.Values) SyncFromCache(item);
        UpdateRgbVisibility();
    }

    void SyncFromCache(ControlItem item)
    {
        if (_control.TryGetCached(item.Code, out var r)) item.Sync(r.Current, r.Max);
        else if (item is EnumItem) item.Sync(uint.MaxValue, 0); // unknown: clear any selection the user just made
    }

    void UpdateRgbVisibility()
    {
        var custom = _control.TryGetCached(ColourMode, out var r) && r.Current == CustomColourMode;
        foreach (var code in RgbGains)
            if (_items.TryGetValue(code, out var item)) item.IsShown = custom;
    }

    uint? Cached(byte code) => _control.TryGetCached(code, out var r) ? r.Current : null;

    internal async Task WriteAsync(byte code, uint value)
    {
        var busyMessage = _items.TryGetValue(code, out var item) ? $"Applying {item.Name}…" : "Applying monitor setting…";
        BeginBusy(busyMessage);
        try
        {
            var result = await Guard(() => _control.SetAsync(code, value));
            if (result?.Outcome == WriteOutcome.NeedsConfirmation)
                result = Confirm(result.Message) ? await Guard(() => _control.SetAsync(code, value, confirmed: true)) : null;
            Report(result);
            if (_items.TryGetValue(code, out item)) SyncFromCache(item);
            if (code == ColourMode) UpdateRgbVisibility();
        }
        finally
        {
            EndBusy(busyMessage);
        }
    }

    public async Task ApplyPresetAsync(bool night)
    {
        var preset = night ? _settings.Night : _settings.Day;
        var busyMessage = $"Applying {preset.Name} preset…";
        BeginBusy(busyMessage);
        try
        {
            Report(await Guard(() => _control.ApplyPresetAsync(preset)));
            SyncAll();
        }
        finally
        {
            EndBusy(busyMessage);
        }
    }

    public void SavePreset(bool night)
    {
        var old = night ? _settings.Night : _settings.Day;
        var preset = old with
        {
            Brightness = Cached(Brightness) ?? old.Brightness,
            Contrast = Cached(Contrast) ?? old.Contrast,
            ColourMode = Cached(ColourMode) is { } mode ? (byte)mode : old.ColourMode,
        };
        if (SaveSettings(night ? _settings with { Night = preset } : _settings with { Day = preset }))
            SetStatus($"{(night ? "Night" : "Day")} preset saved: brightness {preset.Brightness}, contrast {preset.Contrast}.", false);
    }

    public async Task RestoreBackupAsync()
    {
        const string busyMessage = "Restoring monitor backup…";
        BeginBusy(busyMessage);
        try
        {
            var result = await Guard(() => _control.RestoreLatestBackupAsync());
            if (result?.Outcome == WriteOutcome.Ok) SetStatus(result.Message.Length > 0 ? result.Message : "Backup restored.", false);
            else Report(result);
            SyncAll();
        }
        finally
        {
            EndBusy(busyMessage);
        }
    }

    void BeginBusy(string message)
    {
        _busyOperations.Add(message);
        _activeOperations++;
        BusyMessage = _busyOperations[^1];
        OnChanged(nameof(IsBusy));
    }

    void EndBusy(string message)
    {
        var index = _busyOperations.LastIndexOf(message);
        if (index >= 0) _busyOperations.RemoveAt(index);
        _activeOperations = Math.Max(0, _activeOperations - 1);
        BusyMessage = _busyOperations.LastOrDefault() ?? "";
        OnChanged(nameof(IsBusy));
    }

    void RebuildHotkeys()
    {
        var failed = FailedHotkeys();
        Hotkeys.Clear();
        foreach (var action in Enum.GetValues<HotkeyAction>())
            Hotkeys.Add(new HotkeyItem(action, HotkeyLabel(action),
                _settings.Hotkeys.TryGetValue(action, out var text) ? text : "",
                failed.Contains(action) ? InUseText : null));
    }

    public bool SaveHotkeys()
    {
        var map = new Dictionary<HotkeyAction, string>();
        var valid = true;
        foreach (var item in Hotkeys)
        {
            item.Error = null;
            var text = item.Text.Trim();
            if (text.Length == 0) continue;
            if (HotkeyGesture.TryParse(text, out var gesture))
            {
                map[item.Action] = item.Text = gesture.ToString();
            }
            else
            {
                item.Error = "Choose a letter, number, arrow, Home/End, or function key with a modifier.";
                valid = false;
            }
        }

        foreach (var item in Hotkeys.Where(h => h.Error is null && map.ContainsKey(h.Action)))
        {
            if (map.Count(kv => string.Equals(kv.Value, map[item.Action], StringComparison.OrdinalIgnoreCase)) <= 1) continue;
            item.Error = "Used for more than one action";
            valid = false;
        }

        if (!valid)
        {
            SetStatus("Hotkeys not saved — fix the marked entries.", true);
            return false;
        }

        if (!SaveSettings(_settings with { Hotkeys = map })) return false;
        var failed = FailedHotkeys();
        foreach (var item in Hotkeys) item.Error = failed.Contains(item.Action) ? InUseText : null;
        SetStatus(failed.Count == 0 ? "Hotkeys saved." : "Hotkeys saved, but some are in use by another app.", failed.Count > 0);
        return true;
    }

    IReadOnlyCollection<HotkeyAction> FailedHotkeys()
    {
        try { return _failedHotkeys(); }
        catch { return []; }
    }

    static string HotkeyLabel(HotkeyAction action) => action switch
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
        HotkeyAction.SwitchToMe => "Switch to this machine",
        HotkeyAction.OpenPanel => "Open panel",
        _ => action.ToString(),
    };

    bool SaveSettings(AppSettings settings)
    {
        try
        {
            _settingsStore.Save(settings);
            _settings = settings;
            _settingsChanged(settings);
            return true;
        }
        catch (Exception ex)
        {
            SetStatus("Could not save settings: " + ex.Message, true);
            return false;
        }
    }

    async Task<WriteResult?> Guard(Func<Task<WriteResult>> write)
    {
        try { return await write(); }
        catch (Exception ex)
        {
            SetStatus(ex.Message, true);
            return null;
        }
    }

    void Report(WriteResult? result)
    {
        if (result is null) return;
        if (result.Outcome == WriteOutcome.Ok) SetStatus("", false);
        else SetStatus(result.Message, true);
    }

    void SetStatus(string text, bool isError)
    {
        Status = text;
        StatusIsError = isError;
    }
}

public abstract class ControlItem(PanelViewModel owner, ControlDefinition definition) : Observable
{
    bool _isShown = true;

    protected PanelViewModel Owner { get; } = owner;
    public ControlDefinition Definition { get; } = definition;
    public byte Code => Definition.Code;
    public string Name => Definition.Name;

    public bool IsShown { get => _isShown; set => Set(ref _isShown, value); }

    /// <summary>Shows a value read from the monitor. Never writes.</summary>
    public abstract void Sync(uint value, uint max);
}

public sealed class RangeItem(PanelViewModel owner, ControlDefinition definition) : ControlItem(owner, definition)
{
    uint _value, _max = 100;
    uint? _pendingValue;
    uint _applyingValue;
    bool _isApplying;

    public double Max => _max;
    public double Value
    {
        get => _pendingValue ?? _value;
        set => UserSet(value);
    }
    public string ValueText => _pendingValue is { } pending ? $"{pending} (current {_value})" : _value.ToString();
    public bool CanApply => _pendingValue.HasValue && !_isApplying;
    public bool IsApplying => _isApplying;

    /// <summary>Stages a user-selected value without writing to the monitor.</summary>
    public void UserSet(double value)
    {
        var v = (uint)Math.Clamp(Math.Round(value), 0, _max);
        uint? next = v == _value ? null : v;
        if (_pendingValue == next) return;
        _pendingValue = next;
        OnChanged(nameof(Value));
        OnChanged(nameof(ValueText));
        OnChanged(nameof(CanApply));
    }

    public override void Sync(uint value, uint max)
    {
        if (max > 0 && max != _max)
        {
            _max = max;
            OnChanged(nameof(Max));
        }
        SetMonitorValue(value);
    }

    public void SyncFromMonitor(uint value, uint max)
    {
        if (max > 0 && max != _max)
        {
            _max = max;
            OnChanged(nameof(Max));
        }
        SetMonitorValue(value);
    }

    internal bool TryStartApply(out uint value)
    {
        value = 0;
        if (_pendingValue is not { } pending || _isApplying) return false;
        _applyingValue = value = pending;
        _isApplying = true;
        OnChanged(nameof(IsApplying));
        OnChanged(nameof(CanApply));
        return true;
    }

    internal void FinishApply(bool succeeded)
    {
        if (succeeded && _pendingValue == _applyingValue) _pendingValue = null;
        _isApplying = false;
        OnChanged(nameof(IsApplying));
        OnChanged(nameof(Value));
        OnChanged(nameof(ValueText));
        OnChanged(nameof(CanApply));
    }

    void SetMonitorValue(uint value)
    {
        if (value == _value) return;
        _value = value;
        OnChanged(nameof(Value));
        OnChanged(nameof(ValueText));
    }

    void SetValue(uint value)
    {
        if (value == _value) return;
        _value = value;
        OnChanged(nameof(Value));
        OnChanged(nameof(ValueText));
    }
}

public sealed class EnumOption(byte value, string label) : Observable
{
    bool _isActive;

    public byte Value { get; } = value;
    public string Label { get; } = label;
    public bool IsActive => _isActive;

    // Always raised, so a ToggleButton the user un-toggled snaps back to the real state.
    internal void SetActive(bool active)
    {
        _isActive = active;
        OnChanged(nameof(IsActive));
    }

    public override string ToString() => Label;
}

public class EnumItem : ControlItem
{
    EnumOption? _selected;

    public EnumItem(PanelViewModel owner, ControlDefinition definition, IReadOnlyList<byte> values) : base(owner, definition)
    {
        Options = values
            .Select(v => new EnumOption(v, definition.ValueLabels.TryGetValue(v, out var label) ? label : $"0x{v:X2}"))
            .ToList();
        Select = new RelayCommand(p => { if (p is EnumOption option) Choose(option); });
    }

    public IReadOnlyList<EnumOption> Options { get; }
    public ICommand Select { get; }

    public EnumOption? Selected
    {
        get => _selected;
        set { if (value is not null && value != _selected) Choose(value); }
    }

    void Choose(EnumOption option)
    {
        var changed = option != _selected;
        Mark(option);
        if (changed) _ = Owner.WriteAsync(Code, option.Value);
    }

    void Mark(EnumOption? option)
    {
        _selected = option;
        OnChanged(nameof(Selected));
        foreach (var o in Options) o.SetActive(o == option);
    }

    public override void Sync(uint value, uint max) => Mark(Options.FirstOrDefault(o => o.Value == value));
}

/// <summary>An enum shown as a row of toggle buttons (the input selector).</summary>
public sealed class ToggleRowItem(PanelViewModel owner, ControlDefinition definition, IReadOnlyList<byte> values)
    : EnumItem(owner, definition, values);

public sealed class ActionItem : ControlItem
{
    public ActionItem(PanelViewModel owner, ControlDefinition definition) : base(owner, definition) =>
        Run = new RelayCommand(_ => _ = Owner.WriteAsync(Code, 1));

    public ICommand Run { get; }

    public override void Sync(uint value, uint max) { }
}

public sealed class HotkeyItem(HotkeyAction action, string label, string text, string? error) : Observable
{
    string _text = text;
    string? _error = error;

    public HotkeyAction Action { get; } = action;
    public string Label { get; } = label;
    public string Text { get => _text; set => Set(ref _text, value ?? ""); }
    public string? Error { get => _error; set => Set(ref _error, value); }
}
