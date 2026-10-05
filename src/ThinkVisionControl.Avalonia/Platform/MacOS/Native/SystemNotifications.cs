using System.Runtime.InteropServices;
using System.Runtime.Versioning;

namespace ThinkVisionControl.Avalonia.Platform.MacOS.Native;

/// <summary>
/// Display reconfiguration (CoreGraphics) and system power (IOKit) notifications, plus the CoreFoundation run-loop
/// calls that deliver them. Type mapping (LP64): CGError/IOReturn (int32) → int, CGDirectDisplayID/
/// CGDisplayChangeSummaryFlags/natural_t/mach_port_t (uint32) → uint, intptr_t → nint, *Ref and void* → IntPtr.
/// </summary>
[SupportedOSPlatform("macos")]
internal static class SystemNotifications
{
    private const string CoreGraphicsLib = "/System/Library/Frameworks/CoreGraphics.framework/CoreGraphics";
    private const string IoKitLib = "/System/Library/Frameworks/IOKit.framework/IOKit";
    private const string CoreFoundationLib = "/System/Library/Frameworks/CoreFoundation.framework/CoreFoundation";

    // ---- CoreGraphics -------------------------------------------------------------------------------------------

    /// <summary>kCGDisplayBeginConfigurationFlag = (1 &lt;&lt; 0): set on the "before" call, never on the "after" one.</summary>
    public const uint KCGDisplayBeginConfigurationFlag = 1;

    /// <summary>typedef void (*CGDisplayReconfigurationCallBack)(CGDirectDisplayID display, CGDisplayChangeSummaryFlags flags, void *userInfo);</summary>
    [UnmanagedFunctionPointer(CallingConvention.Cdecl)]
    public delegate void DisplayReconfigurationCallback(uint display, uint flags, IntPtr userInfo);

    // CGError CGDisplayRegisterReconfigurationCallback(CGDisplayReconfigurationCallBack callback, void *userInfo);
    [DllImport(CoreGraphicsLib)]
    public static extern int CGDisplayRegisterReconfigurationCallback(DisplayReconfigurationCallback callback, IntPtr userInfo);

    // CGError CGDisplayRemoveReconfigurationCallback(CGDisplayReconfigurationCallBack callback, void *userInfo);
    [DllImport(CoreGraphicsLib)]
    public static extern int CGDisplayRemoveReconfigurationCallback(DisplayReconfigurationCallback callback, IntPtr userInfo);

    // ---- IOKit power management (IOKit/pwr_mgt/IOPMLib.h, IOKit/IOMessage.h) ---------------------------------------

    // iokit_common_msg(x) = sys_iokit (0xE0000000) | sub_iokit_common (0) | x
    /// <summary>kIOMessageCanSystemSleep: idle sleep is being considered; must be answered with IOAllowPowerChange.</summary>
    public const uint KIOMessageCanSystemSleep = 0xE0000270;

    /// <summary>kIOMessageSystemWillSleep: sleep is coming; must be answered with IOAllowPowerChange (else a 30 s delay).</summary>
    public const uint KIOMessageSystemWillSleep = 0xE0000280;

    /// <summary>kIOMessageSystemHasPoweredOn: the system finished waking.</summary>
    public const uint KIOMessageSystemHasPoweredOn = 0xE0000300;

    /// <summary>typedef void (*IOServiceInterestCallback)(void *refcon, io_service_t service, natural_t messageType, void *messageArgument);</summary>
    [UnmanagedFunctionPointer(CallingConvention.Cdecl)]
    public delegate void ServiceInterestCallback(IntPtr refcon, uint service, uint messageType, IntPtr messageArgument);

    // io_connect_t IORegisterForSystemPower(void *refcon, IONotificationPortRef *thePortRef,
    //                                       IOServiceInterestCallback callback, io_object_t *notifier);
    // Returns MACH_PORT_NULL (0) on failure.
    [DllImport(IoKitLib)]
    public static extern uint IORegisterForSystemPower(IntPtr refcon, out IntPtr notifyPort, ServiceInterestCallback callback, out uint notifier);

    // IOReturn IODeregisterForSystemPower(io_object_t *notifier);
    [DllImport(IoKitLib)]
    public static extern int IODeregisterForSystemPower(ref uint notifier);

    // IOReturn IOAllowPowerChange(io_connect_t kernelPort, intptr_t notificationID);
    [DllImport(IoKitLib)]
    public static extern int IOAllowPowerChange(uint kernelPort, nint notificationId);

    // kern_return_t IOServiceClose(io_connect_t connect);
    [DllImport(IoKitLib)]
    public static extern int IOServiceClose(uint connect);

    // CFRunLoopSourceRef IONotificationPortGetRunLoopSource(IONotificationPortRef notify);   (Get rule: not owned)
    [DllImport(IoKitLib)]
    public static extern IntPtr IONotificationPortGetRunLoopSource(IntPtr notifyPort);

    // void IONotificationPortDestroy(IONotificationPortRef notify);
    [DllImport(IoKitLib)]
    public static extern void IONotificationPortDestroy(IntPtr notifyPort);

    // ---- CoreFoundation run loop -----------------------------------------------------------------------------------

    // CFRunLoopRef CFRunLoopGetMain(void);
    [DllImport(CoreFoundationLib)]
    public static extern IntPtr CFRunLoopGetMain();

    // void CFRunLoopAddSource(CFRunLoopRef rl, CFRunLoopSourceRef source, CFRunLoopMode mode);
    [DllImport(CoreFoundationLib)]
    public static extern void CFRunLoopAddSource(IntPtr runLoop, IntPtr source, IntPtr mode);

    // void CFRunLoopRemoveSource(CFRunLoopRef rl, CFRunLoopSourceRef source, CFRunLoopMode mode);
    [DllImport(CoreFoundationLib)]
    public static extern void CFRunLoopRemoveSource(IntPtr runLoop, IntPtr source, IntPtr mode);

    /// <summary>
    /// The value of the exported global <c>const CFRunLoopMode kCFRunLoopCommonModes</c> (a CFStringRef):
    /// the export is the address of the variable, so it is dereferenced once.
    /// </summary>
    public static IntPtr CommonModes()
    {
        var lib = NativeLibrary.Load(CoreFoundationLib);
        return Marshal.ReadIntPtr(NativeLibrary.GetExport(lib, "kCFRunLoopCommonModes"));
    }
}
