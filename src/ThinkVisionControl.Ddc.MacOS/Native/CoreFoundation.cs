using System.Runtime.InteropServices;
using System.Runtime.Versioning;
using System.Text;

namespace ThinkVisionControl.Ddc.MacOS.Native;

/// <summary>
/// CoreFoundation P/Invoke. Type mapping (LP64, arm64): CF*Ref → IntPtr, CFIndex (signed long) → nint,
/// CFTypeID (unsigned long) → nuint, CFStringEncoding (UInt32) → uint, Boolean (unsigned char) → byte.
/// </summary>
[SupportedOSPlatform("macos")]
internal static unsafe class CoreFoundation
{
    private const string Lib = "/System/Library/Frameworks/CoreFoundation.framework/CoreFoundation";

    public const uint kCFStringEncodingUTF8 = 0x08000100;
    public const nint kCFNumberSInt64Type = 4;

    // CFStringRef CFStringCreateWithCString(CFAllocatorRef alloc, const char *cStr, CFStringEncoding encoding);
    [DllImport(Lib)]
    public static extern IntPtr CFStringCreateWithCString(IntPtr alloc, byte* cStr, uint encoding);

    // Boolean CFStringGetCString(CFStringRef theString, char *buffer, CFIndex bufferSize, CFStringEncoding encoding);
    [DllImport(Lib)]
    public static extern byte CFStringGetCString(IntPtr theString, byte* buffer, nint bufferSize, uint encoding);

    // CFTypeID CFGetTypeID(CFTypeRef cf);
    [DllImport(Lib)]
    public static extern nuint CFGetTypeID(IntPtr cf);

    // CFTypeID CFStringGetTypeID(void);
    [DllImport(Lib)]
    public static extern nuint CFStringGetTypeID();

    // CFTypeID CFNumberGetTypeID(void);
    [DllImport(Lib)]
    public static extern nuint CFNumberGetTypeID();

    // CFTypeID CFDictionaryGetTypeID(void);
    [DllImport(Lib)]
    public static extern nuint CFDictionaryGetTypeID();

    // Boolean CFNumberGetValue(CFNumberRef number, CFNumberType theType, void *valuePtr);   (CFNumberType = CFIndex)
    [DllImport(Lib)]
    public static extern byte CFNumberGetValue(IntPtr number, nint theType, void* valuePtr);

    // const void *CFDictionaryGetValue(CFDictionaryRef theDict, const void *key);   (Get rule: not owned)
    [DllImport(Lib)]
    public static extern IntPtr CFDictionaryGetValue(IntPtr theDict, IntPtr key);

    // void CFRelease(CFTypeRef cf);   (crashes on NULL; use Release)
    [DllImport(Lib)]
    public static extern void CFRelease(IntPtr cf);

    public static void Release(IntPtr cf)
    {
        if (cf != IntPtr.Zero) CFRelease(cf);
    }

    /// <summary>Creates an owned CFString (Create rule); release with <see cref="Release"/>.</summary>
    public static IntPtr CreateString(string value)
    {
        var bytes = Encoding.UTF8.GetBytes(value + "\0");
        fixed (byte* p = bytes)
            return CFStringCreateWithCString(IntPtr.Zero, p, kCFStringEncodingUTF8);
    }

    /// <summary>Copies a CFString (borrowed or owned; not released here) into a managed string. Null if not a string.</summary>
    public static string? ToManagedString(IntPtr cf)
    {
        if (cf == IntPtr.Zero || CFGetTypeID(cf) != CFStringGetTypeID()) return null;
        const int size = 1024;
        var buffer = stackalloc byte[size];
        return CFStringGetCString(cf, buffer, size, kCFStringEncodingUTF8) != 0
            ? Marshal.PtrToStringUTF8((IntPtr)buffer)
            : null;
    }

    /// <summary>Reads a CFNumber (not released here) as a 64-bit integer. Null if not a number.</summary>
    public static long? ToInt64(IntPtr cf)
    {
        if (cf == IntPtr.Zero || CFGetTypeID(cf) != CFNumberGetTypeID()) return null;
        long value;
        return CFNumberGetValue(cf, kCFNumberSInt64Type, &value) != 0 ? value : null;
    }

    public static bool IsDictionary(IntPtr cf) => cf != IntPtr.Zero && CFGetTypeID(cf) == CFDictionaryGetTypeID();

    /// <summary>Looks up a string key in a CFDictionary. The result is borrowed (Get rule): do not release it.</summary>
    public static IntPtr GetValue(IntPtr dictionary, string key)
    {
        if (!IsDictionary(dictionary)) return IntPtr.Zero;
        var cfKey = CreateString(key);
        try { return CFDictionaryGetValue(dictionary, cfKey); }
        finally { Release(cfKey); }
    }
}
