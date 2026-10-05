using Avalonia.Controls;
using Avalonia.Input;
using Avalonia.Interactivity;
using Avalonia.Threading;
using FluentAvalonia.UI.Controls;
using ThinkVisionControl.Avalonia.ViewModels;
using ThinkVisionControl.Core.Settings;

namespace ThinkVisionControl.Avalonia.Views;

/// <summary>
/// "Edit shortcut" dialog content: records the pressed combination, shows it as keycaps and reports whether it can be
/// saved (valid, changed and not used by another action). <see cref="Result"/> is the canonical text; "" = cleared.
/// </summary>
public partial class ShortcutEditor : UserControl
{
    readonly string _original = "";
    readonly Func<string, string?> _conflictFor = _ => null;
    bool _live;

    /// <summary>Designer only.</summary>
    public ShortcutEditor() : this("Brightness up", "Ctrl+Alt+Up", _ => null) { }

    /// <param name="conflictFor">Returns the label of another action that already uses a shortcut text, or null.</param>
    public ShortcutEditor(string label, string current, Func<string, string?> conflictFor)
    {
        InitializeComponent();
        ActionRun.Text = label;
        _original = Result = current;
        _conflictFor = conflictFor;
        Hint.Text = $"Shortcuts start with {StartKeys}, and can include Shift.";
        ShowKeys(HotkeyItem.KeysOf(current), live: false);
        AddHandler(KeyDownEvent, OnKeyDown, RoutingStrategies.Tunnel);
        AddHandler(KeyUpEvent, OnKeyUp, RoutingStrategies.Tunnel);
        CaptureArea.PointerPressed += (_, _) => CaptureArea.Focus();
    }

    public string Result { get; private set; } = "";

    public bool IsValid { get; private set; }

    public event Action<bool>? ValidityChanged;

    /// <summary>Puts keyboard focus on the capture area (call once the dialog is open).</summary>
    public void FocusCapture() => Dispatcher.UIThread.Post(() => CaptureArea.Focus(NavigationMethod.Tab), DispatcherPriority.Input);

    static string StartKeys =>
        OperatingSystem.IsMacOS() ? "Control, Option or Command"
        : OperatingSystem.IsWindows() ? "Ctrl, Alt or the Windows key"
        : "Ctrl, Alt or Super";

    void OnKeyDown(object? sender, KeyEventArgs e)
    {
        if (!CaptureArea.IsFocused) return;
        var key = e.Key;
        var modifiers = e.KeyModifiers;
        // Esc closes the dialog, Enter saves, Tab moves on: never recorded.
        if (key == Key.Escape || (key == Key.Enter && modifiers == KeyModifiers.None)
            || (key == Key.Tab && modifiers is KeyModifiers.None or KeyModifiers.Shift))
            return;
        e.Handled = true;

        uint flags = 0;
        if ((modifiers & KeyModifiers.Control) != 0 || key is Key.LeftCtrl or Key.RightCtrl) flags |= HotkeyGesture.ModControl;
        if ((modifiers & KeyModifiers.Alt) != 0 || key is Key.LeftAlt or Key.RightAlt) flags |= HotkeyGesture.ModAlt;
        if ((modifiers & KeyModifiers.Shift) != 0 || key is Key.LeftShift or Key.RightShift) flags |= HotkeyGesture.ModShift;
        if ((modifiers & KeyModifiers.Meta) != 0 || key is Key.LWin or Key.RWin) flags |= HotkeyGesture.ModWin;
        var names = ModifierNames(flags);

        if (key is Key.LeftCtrl or Key.RightCtrl or Key.LeftAlt or Key.RightAlt or Key.LeftShift or Key.RightShift or Key.LWin or Key.RWin)
        {
            ShowKeys(names, live: true);
            return;
        }

        var keyName = KeyName(key);
        if ((flags & (HotkeyGesture.ModControl | HotkeyGesture.ModAlt | HotkeyGesture.ModWin)) == 0)
        {
            Reject([.. names, keyName], "This shortcut isn't valid", $"Start with {StartKeys}.");
            return;
        }
        if (!HotkeyGesture.TryCreate(flags, VirtualKey(key), out var gesture))
        {
            Reject([.. names, keyName], "This key can't be used",
                "Use a letter, number, arrow, Home, End, Page Up, Page Down, Insert, Delete or function key.");
            return;
        }

        Result = gesture.ToString();
        ShowKeys(HotkeyItem.KeysOf(Result), live: false);
        if (_conflictFor(Result) is { } other)
        {
            ShowProblem(FAInfoBarSeverity.Warning, $"{other} already uses this shortcut", "Choose a different combination.");
            SetValid(false);
            return;
        }
        HideProblem();
        SetValid(!string.Equals(Result, _original, StringComparison.OrdinalIgnoreCase));
    }

    // Letting go of the modifiers without a key brings back the last recorded combination.
    void OnKeyUp(object? sender, KeyEventArgs e)
    {
        if (CaptureArea.IsFocused && e.KeyModifiers == KeyModifiers.None && _live && !Problem.IsOpen)
            ShowKeys(HotkeyItem.KeysOf(Result), live: false);
    }

    void Clear_Click(object? sender, RoutedEventArgs e)
    {
        Result = "";
        ShowKeys([], live: false);
        HideProblem();
        SetValid(_original.Length > 0);
        CaptureArea.Focus();
    }

    void Reject(IReadOnlyList<string> keys, string title, string message)
    {
        ShowKeys(keys, live: false);
        ShowProblem(FAInfoBarSeverity.Error, title, message);
        SetValid(false);
    }

    void ShowProblem(FAInfoBarSeverity severity, string title, string message)
    {
        Problem.Severity = severity;
        Problem.Title = title;
        Problem.Message = message;
        Problem.IsOpen = true;
        Problem.IsVisible = true;
    }

    void HideProblem()
    {
        Problem.IsOpen = false;
        Problem.IsVisible = false;
    }

    void ShowKeys(IReadOnlyList<string> keys, bool live)
    {
        _live = live;
        KeysView.ItemsSource = keys;
        KeysView.Opacity = live ? 0.75 : 1;
        Placeholder.IsVisible = keys.Count == 0;
        ClearButton.IsEnabled = Result.Length > 0;
    }

    void SetValid(bool valid)
    {
        IsValid = valid;
        ValidityChanged?.Invoke(valid);
    }

    static List<string> ModifierNames(uint flags)
    {
        var names = new List<string>(4);
        if ((flags & HotkeyGesture.ModControl) != 0) names.Add("Ctrl");
        if ((flags & HotkeyGesture.ModAlt) != 0) names.Add(HotkeyItem.AltKeyName);
        if ((flags & HotkeyGesture.ModShift) != 0) names.Add("Shift");
        if ((flags & HotkeyGesture.ModWin) != 0) names.Add(HotkeyItem.SuperKeyName);
        return names;
    }

    /// <summary>The Win32 virtual-key code <see cref="HotkeyGesture"/> stores for an Avalonia key; 0 = not supported.</summary>
    static uint VirtualKey(Key key) => key switch
    {
        >= Key.A and <= Key.Z => 0x41u + (uint)(key - Key.A),
        >= Key.D0 and <= Key.D9 => 0x30u + (uint)(key - Key.D0),
        >= Key.F1 and <= Key.F24 => 0x70u + (uint)(key - Key.F1),
        Key.PageUp => 0x21,
        Key.PageDown => 0x22,
        Key.End => 0x23,
        Key.Home => 0x24,
        Key.Left => 0x25,
        Key.Up => 0x26,
        Key.Right => 0x27,
        Key.Down => 0x28,
        Key.Insert => 0x2D,
        Key.Delete => 0x2E,
        _ => 0,
    };

    static string KeyName(Key key) => key switch
    {
        >= Key.D0 and <= Key.D9 => ((char)('0' + (key - Key.D0))).ToString(),
        _ => key.ToString(),
    };
}
