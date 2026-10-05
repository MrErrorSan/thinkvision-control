using System.Runtime.InteropServices;
using System.Runtime.Versioning;
using ThinkVisionControl.Core.Logging;

namespace ThinkVisionControl.Avalonia.Platform.Windows;

/// <summary>
/// A raw Win32 window that only receives messages; its owning thread's message loop (Avalonia's, on the UI thread)
/// pumps it. Create and dispose on the UI thread.
/// <para>
/// <paramref name="messageOnly"/> true gives an <c>HWND_MESSAGE</c> window: cheapest, gets directed messages
/// (<c>WM_HOTKEY</c>, <c>RegisterDeviceNotification</c>). Broadcasts (<c>WM_DISPLAYCHANGE</c>, <c>WM_SETTINGCHANGE</c>,
/// <c>WM_POWERBROADCAST</c>) only reach top-level windows, so pass false for a hidden, never-shown top-level window.
/// </para>
/// </summary>
[SupportedOSPlatform("windows")]
public sealed class Win32MessageWindow : IDisposable
{
    const string ClassName = "ThinkVisionControl.Avalonia.MessageWindow";
    const uint WsPopup = 0x80000000;
    const uint WsExToolWindow = 0x00000080;
    static readonly IntPtr HwndMessage = new(-3);

    // One class for every instance; the delegate is static so the GC never collects the native callback.
    static readonly WndProcDelegate Proc = StaticWndProc;
    static readonly Dictionary<IntPtr, Win32MessageWindow> Windows = [];
    static readonly Lock Gate = new();
    static bool _registered;

    readonly Func<int, IntPtr, IntPtr, IntPtr?> _hook;

    /// <param name="hook">(msg, wParam, lParam) → a result to mark the message handled, or null for DefWindowProc.</param>
    public Win32MessageWindow(Func<int, IntPtr, IntPtr, IntPtr?> hook, bool messageOnly = true)
    {
        _hook = hook;
        var instance = GetModuleHandleW(IntPtr.Zero);
        lock (Gate)
        {
            if (!_registered)
            {
                var wc = new WndClassEx
                {
                    Size = (uint)Marshal.SizeOf<WndClassEx>(),
                    WndProc = Marshal.GetFunctionPointerForDelegate(Proc),
                    Instance = instance,
                    ClassName = ClassName,
                };
                // ERROR_CLASS_ALREADY_EXISTS (1410) is fine: the class is already registered in this process.
                if (RegisterClassExW(ref wc) == 0 && Marshal.GetLastWin32Error() != 1410)
                    throw new InvalidOperationException($"RegisterClassExW failed: {Marshal.GetLastWin32Error()}");
                _registered = true;
            }
        }

        Handle = CreateWindowExW(messageOnly ? 0 : WsExToolWindow, ClassName, "ThinkVision Control", messageOnly ? 0 : WsPopup,
            0, 0, 0, 0, messageOnly ? HwndMessage : IntPtr.Zero, IntPtr.Zero, instance, IntPtr.Zero);
        if (Handle == IntPtr.Zero)
            throw new InvalidOperationException($"CreateWindowExW failed: {Marshal.GetLastWin32Error()}");
        lock (Gate) Windows[Handle] = this;
    }

    public IntPtr Handle { get; private set; }

    public void Dispose()
    {
        if (Handle == IntPtr.Zero) return;
        lock (Gate) Windows.Remove(Handle);
        DestroyWindow(Handle);
        Handle = IntPtr.Zero;
    }

    static IntPtr StaticWndProc(IntPtr hwnd, uint msg, IntPtr wParam, IntPtr lParam)
    {
        Win32MessageWindow? window;
        lock (Gate) Windows.TryGetValue(hwnd, out window);
        if (window is not null)
        {
            try
            {
                if (window._hook((int)msg, wParam, lParam) is { } result) return result;
            }
            catch (Exception ex)
            {
                // Never let a managed exception unwind through the native message loop.
                FileLog.Write($"Win32MessageWindow: message 0x{msg:X4}: {ex.GetType().Name}: {ex.Message}");
            }
        }
        return DefWindowProcW(hwnd, msg, wParam, lParam);
    }

    [UnmanagedFunctionPointer(CallingConvention.Winapi)]
    delegate IntPtr WndProcDelegate(IntPtr hwnd, uint msg, IntPtr wParam, IntPtr lParam);

    [StructLayout(LayoutKind.Sequential, CharSet = CharSet.Unicode)]
    struct WndClassEx
    {
        public uint Size;
        public uint Style;
        public IntPtr WndProc;
        public int ClassExtra;
        public int WindowExtra;
        public IntPtr Instance;
        public IntPtr Icon;
        public IntPtr Cursor;
        public IntPtr Background;
        public string? MenuName;
        public string ClassName;
        public IntPtr SmallIcon;
    }

    [DllImport("kernel32.dll")]
    static extern IntPtr GetModuleHandleW(IntPtr moduleName);

    [DllImport("user32.dll", SetLastError = true, CharSet = CharSet.Unicode)]
    static extern ushort RegisterClassExW(ref WndClassEx wc);

    [DllImport("user32.dll", SetLastError = true, CharSet = CharSet.Unicode)]
    static extern IntPtr CreateWindowExW(uint exStyle, string className, string windowName, uint style, int x, int y, int width, int height,
        IntPtr parent, IntPtr menu, IntPtr instance, IntPtr param);

    [DllImport("user32.dll")]
    static extern bool DestroyWindow(IntPtr hwnd);

    [DllImport("user32.dll")]
    static extern IntPtr DefWindowProcW(IntPtr hwnd, uint msg, IntPtr wParam, IntPtr lParam);
}
