using System.Windows.Input;
using Avalonia.Threading;
using ThinkVisionControl.Core.Control;
using ThinkVisionControl.Core.Logging;
using ThinkVisionControl.Core.Profile;
using ThinkVisionControl.Core.Settings;

namespace ThinkVisionControl.Avalonia.ViewModels;

/// <summary>One monitor setting shown as a card.</summary>
public abstract class ControlItem(MainViewModel owner, ControlDefinition definition, string? icon, string? description) : Observable
{
    string? _note;

    protected MainViewModel Owner { get; } = owner;
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
public sealed class RangeItem(MainViewModel owner, ControlDefinition definition, string? icon, string? description)
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
        if (_debounce is null)
        {
            _debounce = new DispatcherTimer { Interval = Debounce };
            _debounce.Tick += (_, _) => DebounceElapsed();
        }
        _debounce.Stop();
        _debounce.Start();
    }

    /// <summary>The pointer went down on the slider (thumb or track): writes go live until <see cref="EndDrag"/>.</summary>
    public void BeginDrag()
    {
        if (!Owner.IsConnected || _dragging) return;
        _debounce?.Stop(); // the drag's final write covers a pending keyboard change
        _dragging = true;
        Owner.Control.BeginDrag(Code);
    }

    public void EndDrag()
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
        catch (Exception ex) { FileLog.Write($"Slider write 0x{Code:X2} failed: {ex.Message}"); }
        finally { _settling--; }

        if (Busy) return; // a newer change is on its way
        Owner.SyncFromCache(this);
        Note = result switch
        {
            { Outcome: WriteOutcome.Ok } => null,
            { Outcome: WriteOutcome.Blocked } when result.Message.Contains("remains") =>
                $"Your monitor kept {Name.ToLowerInvariant()} at {_value}" + (Code == MainViewModel.Brightness ? " — the current colour mode may lock it" : ""),
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

    // Always raised, so a radio button or tile the user clicked snaps back to the real state.
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

    public EnumItem(MainViewModel owner, ControlDefinition definition, IReadOnlyList<byte> values, string? icon, string? description)
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
        if (Code != MainViewModel.Input) return;
        foreach (var o in Options) o.IsThisPc = o.Value == thisPc;
    }

    public override void Sync(uint value, uint max) => Mark(Options.FirstOrDefault(o => o.Value == value));
}

/// <summary>An Off/On choice shown as a ToggleSwitch.</summary>
public sealed class ToggleItem(MainViewModel owner, ControlDefinition definition, IReadOnlyList<byte> values, string? icon, string? description)
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

    public string StateText => IsOn ? "On" : "Off";

    protected override void OnMarked()
    {
        OnChanged(nameof(IsOn));
        OnChanged(nameof(StateText));
    }
}

/// <summary>A row in "Reset and restore": a name, a description and one button.</summary>
public interface IActionRow
{
    string Name { get; }
    string? Description { get; }
    string ButtonText { get; }
    ICommand Run { get; }
}

/// <summary>A one-shot command such as a reset.</summary>
public sealed class ActionItem : ControlItem, IActionRow
{
    public ActionItem(MainViewModel owner, ControlDefinition definition, string? icon, string? description)
        : base(owner, definition, icon, description) =>
        Run = new RelayCommand(_ => _ = Owner.WriteAsync(this, 1));

    public string ButtonText => "Reset";
    public ICommand Run { get; }

    public override void Sync(uint value, uint max) { }
}

/// <summary>"Restore backup": puts back the settings saved before the last reset.</summary>
public sealed class RestoreRow(ICommand run) : Observable, IActionRow
{
    string? _note;

    public string Name => "Restore backup";
    public string? Description => _note ?? "Put back the settings saved before the last reset";
    public string ButtonText => "Restore";
    public ICommand Run { get; } = run;

    internal string? Note
    {
        set { _note = value; OnChanged(nameof(Description)); }
    }
}

/// <summary>The Day or Night preset card.</summary>
public sealed class PresetItem : Observable
{
    string _values = "";
    string? _note;

    internal PresetItem(string name, string icon, MainViewModel owner, bool night)
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
    public string MoreName => $"More options for {Name}";
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
            OnChanged(nameof(IsNotSet));
            OnChanged(nameof(EditName));
        }
    }

    public IReadOnlyList<string> Keys => KeysOf(_text);
    public bool IsSet => _text.Length > 0;
    public bool IsNotSet => _text.Length == 0;

    /// <summary>Registration failed: another app owns this shortcut.</summary>
    public bool IsInUse
    {
        get => _isInUse;
        internal set { if (Set(ref _isInUse, value)) OnChanged(nameof(Warning)); }
    }

    public string? Warning => _isInUse ? "Another app is using this shortcut" : null;

    public string EditName => $"Edit shortcut for {Label}, currently {(IsSet ? string.Join(" + ", Keys) : "none")}";

    /// <summary>Keycap labels for a shortcut such as "Ctrl+Alt+Up" (the Win modifier is named as this OS names it).</summary>
    public static IReadOnlyList<string> KeysOf(string text) =>
        text.Length == 0
            ? []
            : text.Split('+').Select(k => k switch
            {
                "Up" => "↑", "Down" => "↓", "Left" => "←", "Right" => "→",
                "Win" => SuperKeyName, "Alt" => AltKeyName,
                _ => k,
            }).ToList();

    /// <summary>What this OS calls the key stored as "Win".</summary>
    public static string SuperKeyName => OperatingSystem.IsMacOS() ? "Cmd" : OperatingSystem.IsWindows() ? "Win" : "Super";

    /// <summary>What this OS calls the key stored as "Alt".</summary>
    public static string AltKeyName => OperatingSystem.IsMacOS() ? "Option" : "Alt";
}
