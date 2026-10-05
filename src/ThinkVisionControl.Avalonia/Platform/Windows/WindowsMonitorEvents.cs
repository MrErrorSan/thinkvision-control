using System.Runtime.InteropServices;
using System.Runtime.Versioning;
using Avalonia.Threading;
using ThinkVisionControl.Core.Logging;

namespace ThinkVisionControl.Avalonia.Platform.Windows;

/// <summary>
/// Raises <see cref="Changed"/> on the UI thread when a monitor device interface arrives or is removed, the display
/// configuration changes, or the machine resumes. Bursts within 500 ms are coalesced into one event.
/// <para>
/// One hidden top-level window takes all three: <c>WM_DISPLAYCHANGE</c> and <c>WM_POWERBROADCAST</c> are broadcast
/// to top-level windows only (a message-only window misses them), and <c>RegisterDeviceNotification</c> sends its
/// <c>WM_DEVICECHANGE</c> directly to the window. Create and dispose on the UI thread.
/// </para>
/// </summary>
[SupportedOSPlatform("windows")]
public sealed class WindowsMonitorEvents : IMonitorEvents
{
    const int WmDeviceChange = 0x0219;
    const int WmDisplayChange = 0x007E;
    const int WmPowerBroadcast = 0x0218;
    const int PbtApmResumeSuspend = 0x0007;
    const int PbtApmResumeAutomatic = 0x0012;
    const int DbtDeviceArrival = 0x8000;
    const int DbtDeviceRemoveComplete = 0x8004;
    const int DbtDevtypDeviceInterface = 5;
    const int DeviceNotifyWindowHandle = 0;

    static readonly Guid MonitorInterface = new("E6F07B5F-EE97-4a90-B076-33F57BF4EAA7");

    readonly Win32MessageWindow _window;
    DispatcherTimer? _debounce; // created on first use, inside Avalonia's message loop
    IntPtr _notification;

    public WindowsMonitorEvents()
    {
        _window = new Win32MessageWindow(WndProc, messageOnly: false);
        var filter = new DevBroadcastDeviceInterface
        {
            Size = Marshal.SizeOf<DevBroadcastDeviceInterface>(),
            DeviceType = DbtDevtypDeviceInterface,
            ClassGuid = MonitorInterface,
        };
        _notification = RegisterDeviceNotificationW(_window.Handle, ref filter, DeviceNotifyWindowHandle);
        if (_notification == IntPtr.Zero)
            FileLog.Write($"RegisterDeviceNotification failed: {Marshal.GetLastWin32Error()}");
    }

    public event Action? Changed;

    public void Dispose()
    {
        _debounce?.Stop();
        if (_notification != IntPtr.Zero)
        {
            UnregisterDeviceNotification(_notification);
            _notification = IntPtr.Zero;
        }
        _window.Dispose();
    }

    void Schedule()
    {
        if (_debounce is null)
        {
            _debounce = new DispatcherTimer { Interval = TimeSpan.FromMilliseconds(500) };
            _debounce.Tick += (_, _) =>
            {
                _debounce.Stop();
                Changed?.Invoke();
            };
        }
        _debounce.Stop();
        _debounce.Start();
    }

    IntPtr? WndProc(int msg, IntPtr wParam, IntPtr lParam)
    {
        switch (msg)
        {
            case WmDisplayChange:
                Schedule();
                return null;
            case WmPowerBroadcast:
                if ((int)wParam.ToInt64() is PbtApmResumeAutomatic or PbtApmResumeSuspend) Schedule();
                return null;
            case WmDeviceChange when lParam != IntPtr.Zero:
                var evt = (int)wParam.ToInt64();
                if (evt != DbtDeviceArrival && evt != DbtDeviceRemoveComplete) return null;
                // lParam is a DEV_BROADCAST_HDR; for device interfaces the class GUID follows the 12-byte header.
                if (Marshal.ReadInt32(lParam, 4) != DbtDevtypDeviceInterface) return null;
                if (Marshal.PtrToStructure<Guid>(lParam + 12) != MonitorInterface) return null;
                Schedule();
                return new IntPtr(1); // TRUE
            default:
                return null;
        }
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
    static extern IntPtr RegisterDeviceNotificationW(IntPtr hRecipient, ref DevBroadcastDeviceInterface notificationFilter, int flags);

    [DllImport("user32.dll")]
    static extern bool UnregisterDeviceNotification(IntPtr handle);
}
