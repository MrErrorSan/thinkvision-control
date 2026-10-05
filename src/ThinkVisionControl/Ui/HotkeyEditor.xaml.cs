using System.Windows.Controls;
using System.Windows.Input;
using System.Windows.Threading;
using ThinkVisionControl.Core.Settings;
using ThinkVisionControl.Ui.Controls;

namespace ThinkVisionControl.Ui;

/// <summary>
/// "Edit shortcut" dialog content: records the pressed combination, shows it as keycaps and reports whether it can be
/// saved (valid, changed and not used by another action). <see cref="Result"/> is the canonical text; "" = cleared.
/// </summary>
public partial class HotkeyEditor : UserControl
{
    readonly string _original;
    readonly Func<string, string?> _conflictFor;
    bool _isValid;

    /// <param name="conflictFor">Returns the label of another action that already uses a shortcut text, or null.</param>
    public HotkeyEditor(string label, string current, Func<string, string?> conflictFor)
    {
        InitializeComponent();
        ActionRun.Text = label;
        _original = Result = current;
        _conflictFor = conflictFor;
        ShowKeys(HotkeyItem.KeysOf(current), live: false);
        Loaded += (_, _) => Dispatcher.BeginInvoke(DispatcherPriority.Input, () => CaptureArea.Focus());
    }

    public string Result { get; private set; }

    public bool IsValid => _isValid;

    public event Action<bool>? ValidityChanged;

    void CaptureArea_MouseDown(object sender, MouseButtonEventArgs e) => CaptureArea.Focus();

    void OnPreviewKeyDown(object sender, KeyEventArgs e)
    {
        if (!CaptureArea.IsKeyboardFocused) return;
        var key = e.Key == Key.System ? e.SystemKey : e.Key;
        var modifiers = Keyboard.Modifiers;
        // Esc closes the dialog, Enter saves, Tab moves on: never recorded.
        if (key == Key.Escape || (key == Key.Enter && modifiers == ModifierKeys.None)
            || (key == Key.Tab && modifiers is ModifierKeys.None or ModifierKeys.Shift))
            return;
        e.Handled = true;

        uint flags = 0;
        if ((modifiers & ModifierKeys.Control) != 0) flags |= HotkeyGesture.ModControl;
        if ((modifiers & ModifierKeys.Alt) != 0) flags |= HotkeyGesture.ModAlt;
        if ((modifiers & ModifierKeys.Shift) != 0) flags |= HotkeyGesture.ModShift;
        if ((modifiers & ModifierKeys.Windows) != 0) flags |= HotkeyGesture.ModWin;
        var names = ModifierNames(flags);

        if (key is Key.LeftCtrl or Key.RightCtrl or Key.LeftAlt or Key.RightAlt or Key.LeftShift or Key.RightShift or Key.LWin or Key.RWin)
        {
            ShowKeys(names, live: true);
            return;
        }

        var virtualKey = (uint)KeyInterop.VirtualKeyFromKey(key);
        if ((flags & (HotkeyGesture.ModControl | HotkeyGesture.ModAlt | HotkeyGesture.ModWin)) == 0)
        {
            Reject([.. names, key.ToString()], "This shortcut isn't valid", "Start with Ctrl, Alt or the Windows key.");
            return;
        }
        if (!HotkeyGesture.TryCreate(flags, virtualKey, out var gesture))
        {
            Reject([.. names, key.ToString()], "This key can't be used",
                "Use a letter, number, arrow, Home, End, Page Up, Page Down, Insert, Delete or function key.");
            return;
        }

        Result = gesture.ToString();
        ShowKeys(HotkeyItem.KeysOf(Result), live: false);
        if (_conflictFor(Result) is { } other)
        {
            ShowProblem(InfoBarSeverity.Warning, $"{other} already uses this shortcut", "Choose a different combination.");
            SetValid(false);
            return;
        }
        Problem.IsOpen = false;
        SetValid(!string.Equals(Result, _original, StringComparison.OrdinalIgnoreCase));
    }

    // Letting go of modifiers without a key brings back the last recorded combination.
    void OnPreviewKeyUp(object sender, KeyEventArgs e)
    {
        if (CaptureArea.IsKeyboardFocused && Keyboard.Modifiers == ModifierKeys.None && KeysView.Opacity < 1 && !Problem.IsOpen)
            ShowKeys(HotkeyItem.KeysOf(Result), live: false);
    }

    void Clear_Click(object sender, System.Windows.RoutedEventArgs e)
    {
        Result = "";
        ShowKeys([], live: false);
        Problem.IsOpen = false;
        SetValid(_original.Length > 0);
        CaptureArea.Focus();
    }

    void Reject(IReadOnlyList<string> keys, string title, string message)
    {
        ShowKeys(keys, live: false);
        ShowProblem(InfoBarSeverity.Error, title, message);
        SetValid(false);
    }

    void ShowProblem(InfoBarSeverity severity, string title, string message)
    {
        Problem.Severity = severity;
        Problem.Title = title;
        Problem.Message = message;
        Problem.IsOpen = true;
    }

    void ShowKeys(IReadOnlyList<string> keys, bool live)
    {
        KeysView.ItemsSource = keys;
        KeysView.Opacity = live ? 0.75 : 1;
        Placeholder.Visibility = keys.Count == 0 ? System.Windows.Visibility.Visible : System.Windows.Visibility.Collapsed;
        ClearButton.IsEnabled = Result.Length > 0;
    }

    void SetValid(bool valid)
    {
        _isValid = valid;
        ValidityChanged?.Invoke(valid);
    }

    static List<string> ModifierNames(uint flags)
    {
        var names = new List<string>(4);
        if ((flags & HotkeyGesture.ModControl) != 0) names.Add("Ctrl");
        if ((flags & HotkeyGesture.ModAlt) != 0) names.Add("Alt");
        if ((flags & HotkeyGesture.ModShift) != 0) names.Add("Shift");
        if ((flags & HotkeyGesture.ModWin) != 0) names.Add("Win");
        return names;
    }
}
