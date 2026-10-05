using System.Runtime.InteropServices;
using System.Runtime.Versioning;

namespace ThinkVisionControl.Ddc.MacOS.Native;

/// <summary>
/// IOKit P/Invoke, including the private IOAVService I²C calls used by m1ddc and MonitorControl (both MIT).
/// Type mapping (LP64, arm64): mach_port_t / io_object_t / io_service_t / io_iterator_t / io_registry_entry_t
/// (natural_t = unsigned int) → uint; kern_return_t / IOReturn (int) → int; IOOptionBits (UInt32) → uint;
/// boolean_t (4 bytes) → int; io_name_t (char[128]) and io_string_t (char[512]) → byte*; CFTypeRef → IntPtr.
/// </summary>
[SupportedOSPlatform("macos")]
internal static unsafe class IoKit
{
    private const string Lib = "/System/Library/Frameworks/IOKit.framework/IOKit";

    /// <summary><c>kIOMainPortDefault</c> (formerly kIOMasterPortDefault) is MACH_PORT_NULL.</summary>
    public const uint kIOMainPortDefault = 0;
    public const uint kIORegistryIterateRecursively = 0x1;
    public const int IoNameSize = 128;

    // io_registry_entry_t IORegistryGetRootEntry(mach_port_t mainPort);
    [DllImport(Lib)]
    public static extern uint IORegistryGetRootEntry(uint mainPort);

    // kern_return_t IORegistryEntryCreateIterator(io_registry_entry_t entry, const io_name_t plane,
    //                                             IOOptionBits options, io_iterator_t *iterator);
    [DllImport(Lib)]
    public static extern int IORegistryEntryCreateIterator(uint entry, byte* plane, uint options, uint* iterator);

    // io_object_t IOIteratorNext(io_iterator_t iterator);
    [DllImport(Lib)]
    public static extern uint IOIteratorNext(uint iterator);

    // kern_return_t IOObjectRelease(io_object_t object);
    [DllImport(Lib)]
    public static extern int IOObjectRelease(uint @object);

    // boolean_t IOObjectConformsTo(io_object_t object, const io_name_t className);
    [DllImport(Lib)]
    public static extern int IOObjectConformsTo(uint @object, byte* className);

    // kern_return_t IORegistryEntryGetName(io_registry_entry_t entry, io_name_t name);   (name: char[128] out)
    [DllImport(Lib)]
    public static extern int IORegistryEntryGetName(uint entry, byte* name);

    // kern_return_t IORegistryEntryGetRegistryEntryID(io_registry_entry_t entry, uint64_t *entryID);
    [DllImport(Lib)]
    public static extern int IORegistryEntryGetRegistryEntryID(uint entry, ulong* entryID);

    // kern_return_t IORegistryEntryGetParentEntry(io_registry_entry_t entry, const io_name_t plane,
    //                                             io_registry_entry_t *parent);
    [DllImport(Lib)]
    public static extern int IORegistryEntryGetParentEntry(uint entry, byte* plane, uint* parent);

    // io_registry_entry_t IORegistryEntryCopyFromPath(mach_port_t mainPort, CFStringRef path);   (macOS 10.11+)
    [DllImport(Lib)]
    public static extern uint IORegistryEntryCopyFromPath(uint mainPort, IntPtr path);

    // CFTypeRef IORegistryEntryCreateCFProperty(io_registry_entry_t entry, CFStringRef key,
    //                                           CFAllocatorRef allocator, IOOptionBits options);
    [DllImport(Lib)]
    public static extern IntPtr IORegistryEntryCreateCFProperty(uint entry, IntPtr key, IntPtr allocator, uint options);

    // CFTypeRef IORegistryEntrySearchCFProperty(io_registry_entry_t entry, const io_name_t plane, CFStringRef key,
    //                                           CFAllocatorRef allocator, IOOptionBits options);
    [DllImport(Lib)]
    public static extern IntPtr IORegistryEntrySearchCFProperty(uint entry, byte* plane, IntPtr key, IntPtr allocator, uint options);

    // Private (exported from IOKit, no public header; declarations as in m1ddc and MonitorControl):
    // typedef CFTypeRef IOAVServiceRef;
    // IOAVServiceRef IOAVServiceCreateWithService(CFAllocatorRef allocator, io_service_t service);   (Create rule)
    [DllImport(Lib)]
    public static extern IntPtr IOAVServiceCreateWithService(IntPtr allocator, uint service);

    // IOReturn IOAVServiceReadI2C(IOAVServiceRef service, uint32_t chipAddress, uint32_t offset,
    //                             void *outputBuffer, uint32_t outputBufferSize);
    [DllImport(Lib)]
    public static extern int IOAVServiceReadI2C(IntPtr service, uint chipAddress, uint offset, byte* outputBuffer, uint outputBufferSize);

    // IOReturn IOAVServiceWriteI2C(IOAVServiceRef service, uint32_t chipAddress, uint32_t dataAddress,
    //                              void *inputBuffer, uint32_t inputBufferSize);
    [DllImport(Lib)]
    public static extern int IOAVServiceWriteI2C(IntPtr service, uint chipAddress, uint dataAddress, byte* inputBuffer, uint inputBufferSize);

    // ---- helpers ----

    /// <summary>kIOServicePlane, NUL-terminated.</summary>
    public static ReadOnlySpan<byte> ServicePlane => "IOService\0"u8;

    public static void Release(uint obj)
    {
        if (obj != 0) IOObjectRelease(obj);
    }

    public static string? GetName(uint entry)
    {
        var name = stackalloc byte[IoNameSize];
        return IORegistryEntryGetName(entry, name) == 0 ? Marshal.PtrToStringUTF8((IntPtr)name) : null;
    }

    public static ulong? GetRegistryId(uint entry)
    {
        ulong id;
        return IORegistryEntryGetRegistryEntryID(entry, &id) == 0 ? id : null;
    }

    public static bool ConformsTo(uint entry, ReadOnlySpan<byte> classNameNulTerminated)
    {
        fixed (byte* p = classNameNulTerminated)
            return IOObjectConformsTo(entry, p) != 0;
    }

    /// <summary>Searches the entry, then its IOService-plane children, for a property. Owned result: CFRelease it.</summary>
    public static IntPtr SearchProperty(uint entry, string key)
    {
        var cfKey = CoreFoundation.CreateString(key);
        try
        {
            fixed (byte* plane = ServicePlane)
                return IORegistryEntrySearchCFProperty(entry, plane, cfKey, IntPtr.Zero, kIORegistryIterateRecursively);
        }
        finally { CoreFoundation.Release(cfKey); }
    }

    /// <summary>Reads a property of this entry only. Owned result: CFRelease it.</summary>
    public static IntPtr CreateProperty(uint entry, string key)
    {
        var cfKey = CoreFoundation.CreateString(key);
        try { return IORegistryEntryCreateCFProperty(entry, cfKey, IntPtr.Zero, 0); }
        finally { CoreFoundation.Release(cfKey); }
    }

    public static string? SearchString(uint entry, string key)
    {
        var value = SearchProperty(entry, key);
        try { return CoreFoundation.ToManagedString(value); }
        finally { CoreFoundation.Release(value); }
    }

    public static uint GetParent(uint entry)
    {
        uint parent;
        fixed (byte* plane = ServicePlane)
            return IORegistryEntryGetParentEntry(entry, plane, &parent) == 0 ? parent : 0;
    }
}
