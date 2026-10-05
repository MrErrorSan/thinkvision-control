using System.Windows;
using System.Windows.Input;
using ThinkVisionControl.Core.Settings;

namespace ThinkVisionControl.Ui;

public partial class HotkeyCaptureWindow : Window
{
    public HotkeyCaptureWindow(string action)
    {
        InitializeComponent();
        ActionText.Text = action;
        Loaded += (_, _) => Focus();
    }

    public HotkeyGesture Gesture { get; private set; }

    void Window_PreviewKeyDown(object sender, KeyEventArgs e)
    {
        var key = e.Key == Key.System ? e.SystemKey : e.Key;
        if (key == Key.Escape)
        {
            DialogResult = false;
            e.Handled = true;
            return;
        }

        if (key is Key.LeftCtrl or Key.RightCtrl or Key.LeftAlt or Key.RightAlt
            or Key.LeftShift or Key.RightShift or Key.LWin or Key.RWin)
        {
            e.Handled = true;
            return;
        }

        var modifiers = Keyboard.Modifiers;
        uint flags = 0;
        if ((modifiers & ModifierKeys.Control) != 0) flags |= HotkeyGesture.ModControl;
        if ((modifiers & ModifierKeys.Alt) != 0) flags |= HotkeyGesture.ModAlt;
        if ((modifiers & ModifierKeys.Shift) != 0) flags |= HotkeyGesture.ModShift;
        if ((modifiers & ModifierKeys.Windows) != 0) flags |= HotkeyGesture.ModWin;

        var virtualKey = (uint)KeyInterop.VirtualKeyFromKey(key);
        if (!HotkeyGesture.TryCreate(flags, virtualKey, out var gesture))
        {
            HintText.Text = "Use a letter, number, arrow, Home/End, or function key with a modifier.";
            e.Handled = true;
            return;
        }

        Gesture = gesture;
        DialogResult = true;
        e.Handled = true;
    }
}
