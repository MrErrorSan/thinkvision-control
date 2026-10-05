using System.Runtime.InteropServices;
using System.Runtime.Versioning;

namespace ThinkVisionControl.Ddc.MacOS.Native;

/// <summary>
/// CoreGraphics display enumeration. Type mapping: CGDirectDisplayID (uint32_t) → uint, CGError (int32_t) → int,
/// boolean_t (4 bytes) → int.
/// </summary>
[SupportedOSPlatform("macos")]
internal static unsafe class CoreGraphics
{
    private const string Lib = "/System/Library/Frameworks/CoreGraphics.framework/CoreGraphics";

    // CGError CGGetOnlineDisplayList(uint32_t maxDisplays, CGDirectDisplayID *onlineDisplays, uint32_t *displayCount);
    [DllImport(Lib)]
    public static extern int CGGetOnlineDisplayList(uint maxDisplays, uint* onlineDisplays, uint* displayCount);

    // uint32_t CGDisplayVendorNumber(CGDirectDisplayID display);
    [DllImport(Lib)]
    public static extern uint CGDisplayVendorNumber(uint display);

    // uint32_t CGDisplayModelNumber(CGDirectDisplayID display);
    [DllImport(Lib)]
    public static extern uint CGDisplayModelNumber(uint display);

    // uint32_t CGDisplaySerialNumber(CGDirectDisplayID display);
    [DllImport(Lib)]
    public static extern uint CGDisplaySerialNumber(uint display);

    // boolean_t CGDisplayIsBuiltin(CGDirectDisplayID display);
    [DllImport(Lib)]
    public static extern int CGDisplayIsBuiltin(uint display);
}
