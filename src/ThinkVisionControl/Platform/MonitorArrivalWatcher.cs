using System.Runtime.InteropServices;
using System.Windows.Threading;
using ThinkVisionControl.Core.Logging;

namespace ThinkVisionControl.Platform;

/// <summary>
/// Calls <c>arrived</c> when a monitor device interface arrives or is removed (a removal also triggers it so the app
/// re-checks its connection). Bursts within 500 ms are coalesced into one call. Create and dispose on the UI thread.
/// </summary>
public sealed class MonitorArrivalWatcher : IDisposable
{
    const int WmDeviceChange = 0x0219;
    const int DbtDeviceArrival = 0x8000;
    const int DbtDeviceRemoveComplete = 0x8004;
    const int DbtDevtypDeviceInterface = 5;
    const int DeviceNotifyWindowHandle = 0;

    static readonly Guid MonitorInterface = new("E6F07B5F-EE97-4a90-B076-33F57BF4EAA7");

    readonly Action _arrived;
    readonly MessageWindow _window;
    readonly DispatcherTimer _debounce;
    IntPtr _notification;

    public MonitorArrivalWatcher(Action arrived)
    {
        _arrived = arrived;
        _debounce = new DispatcherTimer { Interval = TimeSpan.FromMilliseconds(500) };
        _debounce.Tick += OnDebounced;

        // Message-only windows miss broadcast WM_DEVICECHANGE, but RegisterDeviceNotification sends directed messages.
        _window = new MessageWindow(WndProc);

        var filter = new DevBroadcastDeviceInterface
        {
            Size = Marshal.SizeOf<DevBroadcastDeviceInterface>(),
            DeviceType = DbtDevtypDeviceInterface,
            ClassGuid = MonitorInterface,
        };
        _notification = RegisterDeviceNotification(_window.Handle, ref filter, DeviceNotifyWindowHandle);
        if (_notification == IntPtr.Zero)
            FileLog.Write($"RegisterDeviceNotification failed: {Marshal.GetLastWin32Error()}");
    }

    public void Dispose()
    {
        _debounce.Stop();
        if (_notification != IntPtr.Zero)
        {
            UnregisterDeviceNotification(_notification);
            _notification = IntPtr.Zero;
        }
        _window.Dispose();
    }

    void OnDebounced(object? sender, EventArgs e)
    {
        _debounce.Stop();
        _arrived();
    }

    IntPtr? WndProc(int msg, IntPtr wParam, IntPtr lParam)
    {
        if (msg != WmDeviceChange || lParam == IntPtr.Zero) return null;
        var evt = (int)wParam.ToInt64();
        if (evt != DbtDeviceArrival && evt != DbtDeviceRemoveComplete) return null;

        // lParam is a DEV_BROADCAST_HDR; for device interfaces the class GUID follows the 12-byte header.
        if (Marshal.ReadInt32(lParam, 4) != DbtDevtypDeviceInterface) return null;
        if (Marshal.PtrToStructure<Guid>(lParam + 12) != MonitorInterface) return null;

        _debounce.Stop();
        _debounce.Start();
        return new IntPtr(1); // TRUE
    }

    [StructLayout(LayoutKind.Sequential, CharSet = CharSet.Unicode)]
    struct DevBroadcastDeviceInterface
    {
        public int Size;
        public int DeviceType;
        public int Reserved;
        public Guid ClassGuid;
        public char Name;
    }

    [DllImport("user32.dll", SetLastError = true, CharSet = CharSet.Unicode)]
    static extern IntPtr RegisterDeviceNotification(IntPtr hRecipient, ref DevBroadcastDeviceInterface notificationFilter, int flags);

    [DllImport("user32.dll")]
    static extern bool UnregisterDeviceNotification(IntPtr handle);
}
