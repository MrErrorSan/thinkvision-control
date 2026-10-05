using System.Runtime.InteropServices;
using System.Runtime.Versioning;
using Avalonia.Threading;
using ThinkVisionControl.Avalonia.Platform.MacOS.Native;
using ThinkVisionControl.Core.Logging;

namespace ThinkVisionControl.Avalonia.Platform.MacOS;

/// <summary>
/// Raises <see cref="Changed"/> (on the UI thread, debounced 500 ms) after a display is added, removed or
/// reconfigured, and after the system wakes. After either, the DCPAVServiceProxy behind the monitor can change,
/// so the app must look the service up again (ddc_macos research, Q2).
/// <list type="bullet">
/// <item>Displays: <c>CGDisplayRegisterReconfigurationCallback</c>. It fires once per display before a change
/// (flags contain <c>kCGDisplayBeginConfigurationFlag</c>; ignored) and once after; only the "after" calls count.</item>
/// <item>Wake: <c>IORegisterForSystemPower</c> (IOKit) instead of <c>NSWorkspaceDidWakeNotification</c>, which
/// needs an Objective-C block or an observer class. <c>kIOMessageSystemHasPoweredOn</c> means awake. The
/// registration obliges us to answer <c>kIOMessageCanSystemSleep</c>/<c>kIOMessageSystemWillSleep</c> with
/// <c>IOAllowPowerChange</c>, which is done straight away (never veto).</item>
/// </list>
/// Both callbacks are delivered on the main run loop, which Avalonia's AppKit loop runs.
/// </summary>
[SupportedOSPlatform("macos")]
public sealed class MacMonitorEvents : IMonitorEvents
{
    private static readonly TimeSpan Debounce = TimeSpan.FromMilliseconds(500);

    // Static so the native thunks outlive any instance; the instance is found through the GCHandle in userInfo.
    private static readonly SystemNotifications.DisplayReconfigurationCallback DisplayCallback = OnDisplayReconfigured;
    private static readonly SystemNotifications.ServiceInterestCallback PowerCallback = OnPowerMessage;

    private readonly Timer _debounce;
    private GCHandle _self;
    private bool _displayRegistered;
    private uint _rootPort;          // io_connect_t from IORegisterForSystemPower
    private uint _powerNotifier;     // io_object_t notifier
    private IntPtr _notifyPort;      // IONotificationPortRef
    private IntPtr _runLoopSource;   // CFRunLoopSourceRef (owned by the notify port)
    private IntPtr _commonModes;
    private bool _disposed;

    public event Action? Changed;

    public MacMonitorEvents()
    {
        _debounce = new Timer(_ => Dispatcher.UIThread.Post(() => { if (!_disposed) Changed?.Invoke(); }));
        _self = GCHandle.Alloc(this);
        var userInfo = GCHandle.ToIntPtr(_self);

        try
        {
            var error = SystemNotifications.CGDisplayRegisterReconfigurationCallback(DisplayCallback, userInfo);
            _displayRegistered = error == 0;
            if (!_displayRegistered) FileLog.Write($"CGDisplayRegisterReconfigurationCallback failed: {error}");
        }
        catch (Exception ex) when (ex is DllNotFoundException or EntryPointNotFoundException)
        {
            FileLog.Write($"CoreGraphics unavailable: {ex.Message}");
        }

        try
        {
            _rootPort = SystemNotifications.IORegisterForSystemPower(userInfo, out _notifyPort, PowerCallback, out _powerNotifier);
            if (_rootPort == 0)
            {
                FileLog.Write("IORegisterForSystemPower failed");
                return;
            }
            _runLoopSource = SystemNotifications.IONotificationPortGetRunLoopSource(_notifyPort);
            _commonModes = SystemNotifications.CommonModes();
            SystemNotifications.CFRunLoopAddSource(SystemNotifications.CFRunLoopGetMain(), _runLoopSource, _commonModes);
        }
        catch (Exception ex) when (ex is DllNotFoundException or EntryPointNotFoundException)
        {
            FileLog.Write($"IOKit power notifications unavailable: {ex.Message}");
        }
    }

    public void Dispose()
    {
        if (_disposed) return;
        _disposed = true;

        if (_displayRegistered)
            SystemNotifications.CGDisplayRemoveReconfigurationCallback(DisplayCallback, GCHandle.ToIntPtr(_self));

        var powerRegistered = _rootPort != 0;
        if (powerRegistered)
        {
            if (_runLoopSource != IntPtr.Zero)
                SystemNotifications.CFRunLoopRemoveSource(SystemNotifications.CFRunLoopGetMain(), _runLoopSource, _commonModes);
            SystemNotifications.IODeregisterForSystemPower(ref _powerNotifier);
            SystemNotifications.IOServiceClose(_rootPort);
            if (_notifyPort != IntPtr.Zero) SystemNotifications.IONotificationPortDestroy(_notifyPort);
            _rootPort = 0;
        }

        _debounce.Dispose();
        // If Dispose runs off the main thread a callback may be in flight with the handle; leaking one GCHandle at
        // shutdown is harmless, freeing it under a running callback is not. Freed only when nothing was registered.
        if (!_displayRegistered && !powerRegistered && _self.IsAllocated) _self.Free();
    }

    private void Schedule()
    {
        if (_disposed) return;
        try { _debounce.Change(Debounce, Timeout.InfiniteTimeSpan); }
        catch (ObjectDisposedException) { }
    }

    private static MacMonitorEvents? From(IntPtr userInfo) =>
        userInfo == IntPtr.Zero ? null : GCHandle.FromIntPtr(userInfo).Target as MacMonitorEvents;

    private static void OnDisplayReconfigured(uint display, uint flags, IntPtr userInfo)
    {
        try
        {
            if ((flags & SystemNotifications.KCGDisplayBeginConfigurationFlag) != 0) return;
            From(userInfo)?.Schedule();
        }
        catch (Exception ex)
        {
            FileLog.Write($"Display callback: {ex.GetType().Name}: {ex.Message}");
        }
    }

    private static void OnPowerMessage(IntPtr refcon, uint service, uint messageType, IntPtr messageArgument)
    {
        try
        {
            var self = From(refcon);
            switch (messageType)
            {
                case SystemNotifications.KIOMessageCanSystemSleep:
                case SystemNotifications.KIOMessageSystemWillSleep:
                    // Required acknowledgement; without it sleep is delayed by 30 s. notificationID = messageArgument.
                    if (self is { _rootPort: not 0 }) SystemNotifications.IOAllowPowerChange(self._rootPort, messageArgument);
                    break;
                case SystemNotifications.KIOMessageSystemHasPoweredOn:
                    self?.Schedule();
                    break;
            }
        }
        catch (Exception ex)
        {
            FileLog.Write($"Power callback: {ex.GetType().Name}: {ex.Message}");
        }
    }
}
