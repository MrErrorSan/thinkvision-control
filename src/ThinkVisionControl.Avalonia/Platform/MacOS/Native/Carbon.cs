using System.Runtime.InteropServices;
using System.Runtime.Versioning;

namespace ThinkVisionControl.Avalonia.Platform.MacOS.Native;

/// <summary>
/// Carbon Event Manager hot keys (HIToolbox, re-exported by Carbon.framework). No Accessibility or Input Monitoring
/// permission is involved. Type mapping (LP64): OSStatus (SInt32) → int, OSType/UInt32/OptionBits → uint,
/// ItemCount/ByteCount (unsigned long) → nuint, *Ref → IntPtr.
/// </summary>
[SupportedOSPlatform("macos")]
internal static class Carbon
{
    private const string Lib = "/System/Library/Frameworks/Carbon.framework/Carbon";

    public const int NoErr = 0;

    /// <summary>kEventClassKeyboard = 'keyb'.</summary>
    public const uint KEventClassKeyboard = 0x6B657962;

    /// <summary>kEventHotKeyPressed.</summary>
    public const uint KEventHotKeyPressed = 5;

    /// <summary>kEventParamDirectObject = '----'.</summary>
    public const uint KEventParamDirectObject = 0x2D2D2D2D;

    /// <summary>typeEventHotKeyID = 'hkid'.</summary>
    public const uint TypeEventHotKeyID = 0x686B6964;

    /// <summary>struct EventTypeSpec { OSType eventClass; UInt32 eventKind; };</summary>
    [StructLayout(LayoutKind.Sequential)]
    public struct EventTypeSpec
    {
        public uint EventClass;
        public uint EventKind;
    }

    /// <summary>struct EventHotKeyID { OSType signature; UInt32 id; };   (8 bytes, passed by value in one register)</summary>
    [StructLayout(LayoutKind.Sequential)]
    public struct EventHotKeyID
    {
        public uint Signature;
        public uint Id;
    }

    /// <summary>typedef OSStatus (*EventHandlerProcPtr)(EventHandlerCallRef inHandlerCallRef, EventRef inEvent, void *inUserData);</summary>
    [UnmanagedFunctionPointer(CallingConvention.Cdecl)]
    public delegate int EventHandlerProc(IntPtr handlerCallRef, IntPtr eventRef, IntPtr userData);

    // EventTargetRef GetApplicationEventTarget(void);
    [DllImport(Lib)]
    public static extern IntPtr GetApplicationEventTarget();

    // OSStatus InstallEventHandler(EventTargetRef inTarget, EventHandlerUPP inHandler, ItemCount inNumTypes,
    //                              const EventTypeSpec *inList, void *inUserData, EventHandlerRef *outRef);
    // (EventHandlerUPP is the plain function pointer on Mach-O.)
    [DllImport(Lib)]
    public static extern int InstallEventHandler(IntPtr target, EventHandlerProc handler, nuint numTypes,
        [In] EventTypeSpec[] list, IntPtr userData, out IntPtr handlerRef);

    // OSStatus RemoveEventHandler(EventHandlerRef inHandlerRef);
    [DllImport(Lib)]
    public static extern int RemoveEventHandler(IntPtr handlerRef);

    // OSStatus RegisterEventHotKey(UInt32 inHotKeyCode, UInt32 inHotKeyModifiers, EventHotKeyID inHotKeyID,
    //                              EventTargetRef inTarget, OptionBits inOptions, EventHotKeyRef *outRef);
    [DllImport(Lib)]
    public static extern int RegisterEventHotKey(uint hotKeyCode, uint hotKeyModifiers, EventHotKeyID hotKeyId,
        IntPtr target, uint options, out IntPtr hotKeyRef);

    // OSStatus UnregisterEventHotKey(EventHotKeyRef inHotKey);
    [DllImport(Lib)]
    public static extern int UnregisterEventHotKey(IntPtr hotKeyRef);

    // OSStatus GetEventParameter(EventRef inEvent, EventParamName inName, EventParamType inDesiredType,
    //                            EventParamType *outActualType /* may be NULL */, ByteCount inBufferSize,
    //                            ByteCount *outActualSize /* may be NULL */, void *outData);
    [DllImport(Lib)]
    public static extern int GetEventParameter(IntPtr eventRef, uint name, uint desiredType, IntPtr outActualType,
        nuint bufferSize, IntPtr outActualSize, out EventHotKeyID outData);
}
