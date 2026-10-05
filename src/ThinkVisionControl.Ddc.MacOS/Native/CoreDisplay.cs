using System.Runtime.InteropServices;
using System.Runtime.Versioning;

namespace ThinkVisionControl.Ddc.MacOS.Native;

/// <summary>
/// The private <c>CoreDisplay_DisplayCreateInfoDictionary</c> (declared by m1ddc and MonitorControl, both MIT).
/// m1ddc links it with a plain <c>-framework CoreDisplay</c>, which puts it in /System/Library/Frameworks; the
/// PrivateFrameworks path is tried as a fallback. If neither loads, callers get IntPtr.Zero and the locator falls
/// back to matching by framebuffer attributes.
/// </summary>
[SupportedOSPlatform("macos")]
internal static class CoreDisplay
{
    private const string Lib = "/System/Library/Frameworks/CoreDisplay.framework/CoreDisplay";
    private const string PrivateLib = "/System/Library/PrivateFrameworks/CoreDisplay.framework/CoreDisplay";

    // CFDictionaryRef CoreDisplay_DisplayCreateInfoDictionary(CGDirectDisplayID display);   (Create rule; NULL for Sidecar/AirPlay)
    [DllImport(Lib, EntryPoint = "CoreDisplay_DisplayCreateInfoDictionary")]
    private static extern IntPtr CreateInfoDictionaryPublic(uint display);

    [DllImport(PrivateLib, EntryPoint = "CoreDisplay_DisplayCreateInfoDictionary")]
    private static extern IntPtr CreateInfoDictionaryPrivate(uint display);

    private static int _source; // 0 = untried, 1 = public path, 2 = private path, -1 = unavailable

    /// <summary>Owned CFDictionary (CFRelease it), or IntPtr.Zero if unavailable for this display or this OS.</summary>
    public static IntPtr CreateInfoDictionary(uint display)
    {
        if (_source is 0 or 1)
        {
            try
            {
                var dict = CreateInfoDictionaryPublic(display);
                _source = 1;
                return dict;
            }
            catch (Exception ex) when (ex is DllNotFoundException or EntryPointNotFoundException)
            {
                _source = 2;
            }
        }
        if (_source == 2)
        {
            try { return CreateInfoDictionaryPrivate(display); }
            catch (Exception ex) when (ex is DllNotFoundException or EntryPointNotFoundException)
            {
                _source = -1;
            }
        }
        return IntPtr.Zero;
    }
}
