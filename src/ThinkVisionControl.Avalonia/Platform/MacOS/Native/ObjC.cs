using System.Runtime.InteropServices;
using System.Runtime.Versioning;

namespace ThinkVisionControl.Avalonia.Platform.MacOS.Native;

/// <summary>
/// The Objective-C runtime (libobjc) through plain P/Invoke. <c>objc_msgSend</c> is a trampoline that jumps to the
/// method with the registers untouched, so every distinct method signature needs its own declaration with the exact
/// C types; on arm64 struct returns go through registers too (no <c>_stret</c>). Type mapping (LP64):
/// id/Class/SEL/pointers → IntPtr, NSInteger → long, NSUInteger → ulong, CGFloat → double, BOOL (1 byte) → bool as I1.
/// Every call here must happen on the main (Avalonia UI) thread unless noted, as AppKit requires.
/// </summary>
[SupportedOSPlatform("macos")]
internal static class ObjC
{
    private const string Lib = "/usr/lib/libobjc.A.dylib";

    // Class objc_getClass(const char *name);   (NULL when the class is not loaded)
    [DllImport(Lib, EntryPoint = "objc_getClass")]
    public static extern IntPtr GetClass([MarshalAs(UnmanagedType.LPUTF8Str)] string name);

    // SEL sel_registerName(const char *str);
    [DllImport(Lib, EntryPoint = "sel_registerName")]
    public static extern IntPtr Selector([MarshalAs(UnmanagedType.LPUTF8Str)] string name);

    // void *objc_autoreleasePoolPush(void);
    [DllImport(Lib, EntryPoint = "objc_autoreleasePoolPush")]
    private static extern IntPtr AutoreleasePoolPush();

    // void objc_autoreleasePoolPop(void *context);
    [DllImport(Lib, EntryPoint = "objc_autoreleasePoolPop")]
    private static extern void AutoreleasePoolPop(IntPtr context);

    // id objc_msgSend(id self, SEL op);   e.g. +[NSApplication sharedApplication], -[NSAppearance name]
    [DllImport(Lib, EntryPoint = "objc_msgSend")]
    public static extern IntPtr SendIntPtr(IntPtr receiver, IntPtr selector);

    // id objc_msgSend(id self, SEL op, NSUInteger arg);   e.g. -[NSArray objectAtIndex:]
    [DllImport(Lib, EntryPoint = "objc_msgSend")]
    public static extern IntPtr SendIntPtr(IntPtr receiver, IntPtr selector, ulong arg);

    // id objc_msgSend(id self, SEL op, const char *arg);   e.g. -[NSString initWithUTF8String:]
    [DllImport(Lib, EntryPoint = "objc_msgSend")]
    public static extern IntPtr SendIntPtr(IntPtr receiver, IntPtr selector, [MarshalAs(UnmanagedType.LPUTF8Str)] string arg);

    // BOOL objc_msgSend(id self, SEL op, NSError **error);   e.g. -[SMAppService registerAndReturnError:]
    [DllImport(Lib, EntryPoint = "objc_msgSend")]
    [return: MarshalAs(UnmanagedType.I1)]
    public static extern bool SendBoolWithError(IntPtr receiver, IntPtr selector, out IntPtr error);

    // NSInteger objc_msgSend(id self, SEL op);   e.g. -[SMAppService status]
    // NSUInteger objc_msgSend(id self, SEL op);  e.g. -[NSArray count]  (same register, read as long and cast)
    [DllImport(Lib, EntryPoint = "objc_msgSend")]
    public static extern long SendLong(IntPtr receiver, IntPtr selector);

    // CGFloat objc_msgSend(id self, SEL op);   e.g. -[NSScreen backingScaleFactor]   (returned in d0 on arm64)
    [DllImport(Lib, EntryPoint = "objc_msgSend")]
    public static extern double SendDouble(IntPtr receiver, IntPtr selector);

    // void objc_msgSend(id self, SEL op);   e.g. +[SMAppService openSystemSettingsLoginItems], -[NSObject release]
    [DllImport(Lib, EntryPoint = "objc_msgSend")]
    public static extern void SendVoid(IntPtr receiver, IntPtr selector);

    // void objc_msgSend(id self, SEL op, NSInteger/NSUInteger/BOOL arg);
    //   e.g. -[NSWindow setLevel:(NSInteger)], -[NSWindow setCollectionBehavior:(NSUInteger)],
    //        -[NSWindow setIgnoresMouseEvents:(BOOL)].
    // A BOOL is passed widened to 64 bits (0 or 1): the register then holds exactly what a zero-extended BOOL would,
    // which satisfies Apple's arm64 rule that callers extend sub-32-bit arguments.
    [DllImport(Lib, EntryPoint = "objc_msgSend")]
    public static extern void SendVoid(IntPtr receiver, IntPtr selector, long arg);

    // NSPoint objc_msgSend(id self, SEL op);   e.g. +[NSEvent mouseLocation]
    // NSPoint is two CGFloats (an HFA): d0/d1 on arm64, xmm0/xmm1 on x86-64, so plain objc_msgSend on both.
    [DllImport(Lib, EntryPoint = "objc_msgSend")]
    public static extern NSPoint SendPoint(IntPtr receiver, IntPtr selector);

    // NSRect objc_msgSend(id self, SEL op);   e.g. -[NSScreen frame], -[NSScreen visibleFrame]
    // arm64: four CGFloats (an HFA) come back in d0–d3, so plain objc_msgSend.
    [DllImport(Lib, EntryPoint = "objc_msgSend")]
    private static extern NSRect SendRectArm64(IntPtr receiver, IntPtr selector);

    // void objc_msgSend_stret(NSRect *result, id self, SEL op);   x86-64 only: 32-byte structs return through memory.
    [DllImport(Lib, EntryPoint = "objc_msgSend_stret")]
    private static extern void SendRectStretX64(out NSRect result, IntPtr receiver, IntPtr selector);

    /// <summary>A method returning NSRect, on either architecture (the app ships arm64; x64 is a courtesy).</summary>
    public static NSRect SendRect(IntPtr receiver, IntPtr selector)
    {
        if (RuntimeInformation.ProcessArchitecture == Architecture.X64)
        {
            SendRectStretX64(out var rect, receiver, selector);
            return rect;
        }
        return SendRectArm64(receiver, selector);
    }

    /// <summary><c>[[cls alloc] initWithUTF8String:text]</c>: an owned NSString (+1). Release with <see cref="Release"/>.</summary>
    public static IntPtr CreateNSString(string text)
    {
        var cls = GetClass("NSString");
        var allocated = SendIntPtr(cls, Selector("alloc"));
        return SendIntPtr(allocated, Selector("initWithUTF8String:"), text);
    }

    /// <summary>The managed copy of an NSString (not owned, not released), or null for nil.</summary>
    public static string? ReadNSString(IntPtr nsString)
    {
        if (nsString == IntPtr.Zero) return null;
        // const char *-[NSString UTF8String];   (inner pointer, valid while the string and the current pool live)
        var utf8 = SendIntPtr(nsString, Selector("UTF8String"));
        return utf8 == IntPtr.Zero ? null : Marshal.PtrToStringUTF8(utf8);
    }

    /// <summary>-[NSObject release] for an object this code owns (+1); nil is ignored.</summary>
    public static void Release(IntPtr obj)
    {
        if (obj != IntPtr.Zero) SendVoid(obj, Selector("release"));
    }

    /// <summary>
    /// An autorelease pool for the duration of a <c>using</c>, so autoreleased results (NSError, class-method
    /// getters) are freed even when called from a thread without one. Safe to nest inside AppKit's own pool.
    /// </summary>
    public static AutoreleasePool Pool() => new(AutoreleasePoolPush());

    public readonly struct AutoreleasePool : IDisposable
    {
        private readonly IntPtr _context;

        public AutoreleasePool(IntPtr context) => _context = context;

        public void Dispose()
        {
            if (_context != IntPtr.Zero) AutoreleasePoolPop(_context);
        }
    }
}

/// <summary>CGPoint/NSPoint: two CGFloats (double on 64-bit).</summary>
[StructLayout(LayoutKind.Sequential)]
internal struct NSPoint
{
    public double X, Y;
}

/// <summary>CGRect/NSRect: origin then size, four CGFloats. Cocoa screen coordinates: origin bottom-left, y up.</summary>
[StructLayout(LayoutKind.Sequential)]
internal struct NSRect
{
    public double X, Y, Width, Height;
}
