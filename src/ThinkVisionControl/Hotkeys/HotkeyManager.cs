using System.Runtime.InteropServices;
using System.Windows.Interop;
using ThinkVisionControl.Core.Settings;

namespace ThinkVisionControl.Hotkeys;

/// <summary>Global hotkeys via RegisterHotKey on a message-only window. Must live on the UI thread.</summary>
public sealed class HotkeyManager : IDisposable
{
    const int WmHotkey = 0x0312;
    const uint ModNoRepeat = 0x4000;

    readonly HwndSource _source;
    readonly Action<HotkeyAction> _onHotkey;
    readonly List<int> _registered = [];
    readonly List<HotkeyAction> _failed = [];

    public HotkeyManager(Action<HotkeyAction> onHotkey)
    {
        _onHotkey = onHotkey;
        _source = new HwndSource(new HwndSourceParameters("ThinkVisionControl.Hotkeys") { ParentWindow = new IntPtr(-3) });
        _source.AddHook(WndProc);
    }

    public IReadOnlyCollection<HotkeyAction> Failed => _failed;

    public void Suspend() => UnregisterAll();

    public void Register(IReadOnlyDictionary<HotkeyAction, string> hotkeys)
    {
        UnregisterAll();
        _failed.Clear();
        foreach (var action in Enum.GetValues<HotkeyAction>())
        {
            if (!hotkeys.TryGetValue(action, out var text) || !HotkeyGesture.TryParse(text, out var gesture)) continue;
            var id = (int)action + 1;
            if (RegisterHotKey(_source.Handle, id, gesture.Modifiers | ModNoRepeat, gesture.VirtualKey))
                _registered.Add(id);
            else
                _failed.Add(action);
        }
    }

    public void Dispose()
    {
        UnregisterAll();
        _source.Dispose();
    }

    void UnregisterAll()
    {
        foreach (var id in _registered) UnregisterHotKey(_source.Handle, id);
        _registered.Clear();
    }

    IntPtr WndProc(IntPtr hwnd, int msg, IntPtr wParam, IntPtr lParam, ref bool handled)
    {
        if (msg == WmHotkey)
        {
            _onHotkey((HotkeyAction)(wParam.ToInt32() - 1));
            handled = true;
        }
        return IntPtr.Zero;
    }

    [DllImport("user32.dll", SetLastError = true)]
    static extern bool RegisterHotKey(IntPtr hWnd, int id, uint fsModifiers, uint vk);

    [DllImport("user32.dll")]
    static extern bool UnregisterHotKey(IntPtr hWnd, int id);
}
