using System.Runtime.InteropServices;
using System.Runtime.Versioning;
using ThinkVisionControl.Core.Settings;

namespace ThinkVisionControl.Avalonia.Platform.Windows;

/// <summary>Global hotkeys via <c>RegisterHotKey</c> on a message-only window. Create, use and dispose on the UI thread; <see cref="Pressed"/> is raised there.</summary>
[SupportedOSPlatform("windows")]
public sealed class WindowsHotkeys : IHotkeys
{
    const int WmHotkey = 0x0312;
    const uint ModNoRepeat = 0x4000;

    readonly Win32MessageWindow _window;
    readonly List<int> _registered = [];

    public WindowsHotkeys() => _window = new Win32MessageWindow(WndProc);

    public bool IsAvailable => true;

    public event Action<HotkeyAction>? Pressed;

    public IReadOnlyCollection<HotkeyAction> Register(IReadOnlyDictionary<HotkeyAction, string> map)
    {
        UnregisterAll();
        var failed = new List<HotkeyAction>();
        foreach (var action in Enum.GetValues<HotkeyAction>())
        {
            if (!map.TryGetValue(action, out var text) || !HotkeyGesture.TryParse(text, out var gesture)) continue;
            var id = (int)action + 1;
            if (RegisterHotKey(_window.Handle, id, gesture.Modifiers | ModNoRepeat, gesture.VirtualKey))
                _registered.Add(id);
            else
                failed.Add(action);
        }
        return failed;
    }

    public void Suspend() => UnregisterAll();

    public void Dispose()
    {
        UnregisterAll();
        _window.Dispose();
    }

    void UnregisterAll()
    {
        foreach (var id in _registered) UnregisterHotKey(_window.Handle, id);
        _registered.Clear();
    }

    IntPtr? WndProc(int msg, IntPtr wParam, IntPtr lParam)
    {
        if (msg != WmHotkey) return null;
        Pressed?.Invoke((HotkeyAction)(wParam.ToInt32() - 1));
        return IntPtr.Zero;
    }

    [DllImport("user32.dll", SetLastError = true)]
    static extern bool RegisterHotKey(IntPtr hWnd, int id, uint fsModifiers, uint vk);

    [DllImport("user32.dll")]
    static extern bool UnregisterHotKey(IntPtr hWnd, int id);
}
