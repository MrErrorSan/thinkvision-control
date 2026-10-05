using System.Runtime.InteropServices;
using System.Runtime.Versioning;
using ThinkVisionControl.Avalonia.Platform.MacOS.Native;
using ThinkVisionControl.Core.Logging;

namespace ThinkVisionControl.Avalonia.Platform.MacOS;

/// <summary>
/// Start at login through <c>SMAppService.mainAppService</c> (ServiceManagement, macOS 13+), called through the
/// Objective-C runtime. It registers the running .app bundle as a login item, so it only works from a bundle
/// (scripts/make-macos-app.sh); from <c>dotnet run</c> or a bare publish folder it reports off and does nothing.
/// When the user has to approve the item, System Settings → Login Items is opened and it reports off until approved.
/// </summary>
[SupportedOSPlatform("macos")]
public sealed class MacAutostart : IAutostart
{
    private const string ServiceManagementLib = "/System/Library/Frameworks/ServiceManagement.framework/ServiceManagement";

    // SMAppServiceStatus (NSInteger)
    private const long StatusNotRegistered = 0, StatusEnabled = 1, StatusRequiresApproval = 2, StatusNotFound = 3;

    private readonly Lazy<IntPtr> _serviceClass = new(LoadServiceClass);
    private readonly Lazy<bool> _inBundle = new(RunsFromAppBundle);

    public bool IsEnabled => Status() == StatusEnabled;

    public void Set(bool enabled)
    {
        var service = MainAppService();
        if (service == IntPtr.Zero) return;
        try
        {
            using var pool = ObjC.Pool();
            var status = ObjC.SendLong(service, ObjC.Selector("status"));
            if (enabled)
            {
                if (status == StatusEnabled) return;
                // - (BOOL)registerAndReturnError:(NSError **)error;
                if (!ObjC.SendBoolWithError(service, ObjC.Selector("registerAndReturnError:"), out var error))
                    FileLog.Write($"SMAppService register failed: {Describe(error)}");
                if (ObjC.SendLong(service, ObjC.Selector("status")) == StatusRequiresApproval)
                {
                    // + (void)openSystemSettingsLoginItems;
                    ObjC.SendVoid(_serviceClass.Value, ObjC.Selector("openSystemSettingsLoginItems"));
                }
            }
            else if (status is StatusEnabled or StatusRequiresApproval)
            {
                // - (BOOL)unregisterAndReturnError:(NSError **)error;
                if (!ObjC.SendBoolWithError(service, ObjC.Selector("unregisterAndReturnError:"), out var error))
                    FileLog.Write($"SMAppService unregister failed: {Describe(error)}");
            }
        }
        catch (Exception ex) when (ex is DllNotFoundException or EntryPointNotFoundException)
        {
            FileLog.Write($"SMAppService: {ex.Message}");
        }
    }

    private long Status()
    {
        var service = MainAppService();
        if (service == IntPtr.Zero) return StatusNotFound;
        using var pool = ObjC.Pool();
        // @property (readonly) SMAppServiceStatus status;
        return ObjC.SendLong(service, ObjC.Selector("status"));
    }

    /// <summary>
    /// <c>+[SMAppService mainAppService]</c> (a shared singleton, not owned), or zero outside a bundle or before macOS 13.
    /// </summary>
    private IntPtr MainAppService()
    {
        if (!_inBundle.Value || _serviceClass.Value == IntPtr.Zero) return IntPtr.Zero;
        // @property (class, readonly) SMAppService *mainAppService;
        return ObjC.SendIntPtr(_serviceClass.Value, ObjC.Selector("mainAppService"));
    }

    private static IntPtr LoadServiceClass()
    {
        // objc_getClass only finds classes from loaded images, so load the framework first.
        if (!NativeLibrary.TryLoad(ServiceManagementLib, out _)) return IntPtr.Zero;
        return ObjC.GetClass("SMAppService"); // nil before macOS 13
    }

    private static bool RunsFromAppBundle()
    {
        try
        {
            using var pool = ObjC.Pool();
            // + (NSBundle *)mainBundle;  - (NSString *)bundlePath;
            var bundle = ObjC.SendIntPtr(ObjC.GetClass("NSBundle"), ObjC.Selector("mainBundle"));
            var path = bundle == IntPtr.Zero ? null : ObjC.ReadNSString(ObjC.SendIntPtr(bundle, ObjC.Selector("bundlePath")));
            return MacRules.IsAppBundlePath(path);
        }
        catch (Exception ex) when (ex is DllNotFoundException or EntryPointNotFoundException)
        {
            return false;
        }
    }

    // - (NSString *)localizedDescription;   (error is autoreleased; read inside the caller's pool)
    private static string Describe(IntPtr error) =>
        error == IntPtr.Zero ? "unknown error" : ObjC.ReadNSString(ObjC.SendIntPtr(error, ObjC.Selector("localizedDescription"))) ?? "unknown error";
}
