using System.Runtime.Versioning;
using ThinkVisionControl.Ddc.MacOS.Native;
using CF = ThinkVisionControl.Ddc.MacOS.Native.CoreFoundation;
using CG = ThinkVisionControl.Ddc.MacOS.Native.CoreGraphics;

namespace ThinkVisionControl.Ddc.MacOS;

/// <summary>One online display as CoreGraphics and CoreDisplay report it.</summary>
public sealed record MacDisplay(uint Id, uint Vendor, uint Model, uint Serial, bool IsBuiltin, string? IoDisplayLocation);

/// <summary>
/// Result of <see cref="AvServiceLocator.Locate"/>. On success <see cref="Service"/> is an owned IOAVServiceRef
/// (CFTypeRef): whoever takes it must CFRelease it. On failure it is IntPtr.Zero and <see cref="Reason"/> says why.
/// </summary>
public sealed record AvServiceLookup(
    IntPtr Service, uint DisplayId, MatchMethod Method, MacPort Port, bool IsMcdp29xx, int ExternalServiceCount, string? Reason)
{
    internal static AvServiceLookup Failed(string reason, uint displayId = 0, int externalServiceCount = 0) =>
        new(IntPtr.Zero, displayId, MatchMethod.None, MacPort.Unknown, false, externalServiceCount, reason);
}

/// <summary>
/// Finds the IOAVService (DDC endpoint) of the target monitor on Apple Silicon. Approach from m1ddc (MIT): the display's
/// IODisplayLocation names its framebuffer (AppleCLCD2 / IOMobileFramebufferShim); in the recursive IOService-plane walk
/// the DCPAVServiceProxy with Location = "External" that follows that framebuffer is the display's DDC service.
/// </summary>
[SupportedOSPlatform("macos")]
public static class AvServiceLocator
{
    private const int MaxDisplays = 16;
    private const string ProxyName = "DCPAVServiceProxy";

    public static unsafe IReadOnlyList<MacDisplay> ListDisplays()
    {
        var ids = stackalloc uint[MaxDisplays];
        uint count;
        if (CG.CGGetOnlineDisplayList(MaxDisplays, ids, &count) != 0) return [];

        var displays = new List<MacDisplay>((int)count);
        for (var i = 0; i < count && i < MaxDisplays; i++)
        {
            var id = ids[i];
            displays.Add(new MacDisplay(id, CG.CGDisplayVendorNumber(id), CG.CGDisplayModelNumber(id),
                CG.CGDisplaySerialNumber(id), CG.CGDisplayIsBuiltin(id) != 0, IoDisplayLocation(id)));
        }
        return displays;
    }

    public static AvServiceLookup Locate(ushort vendor = MacDdcRules.LenovoVendor, ushort model = MacDdcRules.P32p20Model)
    {
        var displays = ListDisplays();
        var target = displays.FirstOrDefault(d => !d.IsBuiltin && MacDdcRules.IsTarget(d.Vendor, d.Model, vendor, model));
        if (target is null)
            return AvServiceLookup.Failed($"No external display with vendor 0x{vendor:X4} model 0x{model:X4} among {displays.Count} online.");

        var adapterId = AdapterRegistryId(target.IoDisplayLocation);
        var proxies = WalkExternalProxies();
        try
        {
            var (index, method) = MacDdcRules.ChooseService([.. proxies.Select(p => p.Info)], adapterId, MacDdcRules.PnpId(vendor), model);
            if (index < 0)
                return AvServiceLookup.Failed(proxies.Count == 0
                    ? "No external DCPAVServiceProxy (DisplayLink, Intel Mac, or a dock that hides DDC?)."
                    : $"{proxies.Count} external DCPAVServiceProxy entries, none tied to display {target.Id}.",
                    target.Id, proxies.Count);

            var (entry, info) = proxies[index];
            var service = IoKit.IOAVServiceCreateWithService(IntPtr.Zero, entry);
            if (service == IntPtr.Zero)
                return AvServiceLookup.Failed("IOAVServiceCreateWithService returned NULL.", target.Id, proxies.Count);

            var mcdp = IsMcdp29xx(entry);
            return new AvServiceLookup(service, target.Id, method, MacDdcRules.ClassifyPort(mcdp, info.Downstream), mcdp, proxies.Count, null);
        }
        finally
        {
            foreach (var (entry, _) in proxies) IoKit.Release(entry);
        }
    }

    /// <summary>CoreDisplay's IODisplayLocation for the display: the IOService path of its framebuffer.</summary>
    private static string? IoDisplayLocation(uint displayId)
    {
        var info = CoreDisplay.CreateInfoDictionary(displayId);
        try { return CF.ToManagedString(CF.GetValue(info, "IODisplayLocation")); }
        finally { CF.Release(info); }
    }

    private static ulong? AdapterRegistryId(string? ioDisplayLocation)
    {
        if (string.IsNullOrEmpty(ioDisplayLocation)) return null;
        var path = CF.CreateString(ioDisplayLocation);
        var adapter = IoKit.IORegistryEntryCopyFromPath(IoKit.kIOMainPortDefault, path);
        CF.Release(path);
        if (adapter == 0) return null;
        try { return IoKit.GetRegistryId(adapter); }
        finally { IoKit.Release(adapter); }
    }

    /// <summary>
    /// Recursive IOService-plane walk from the root. Returns every external DCPAVServiceProxy (retained; caller
    /// releases) with the facts of the framebuffer that precedes it.
    /// </summary>
    private static unsafe List<(uint Entry, ProxyCandidate Info)> WalkExternalProxies()
    {
        var found = new List<(uint, ProxyCandidate)>();
        var root = IoKit.IORegistryGetRootEntry(IoKit.kIOMainPortDefault);
        if (root == 0) return found;

        uint iterator;
        int kr;
        fixed (byte* plane = IoKit.ServicePlane)
            kr = IoKit.IORegistryEntryCreateIterator(root, plane, IoKit.kIORegistryIterateRecursively, &iterator);
        IoKit.Release(root);
        if (kr != 0) return found;

        ProxyCandidate framebuffer = default;
        try
        {
            uint entry;
            while ((entry = IoKit.IOIteratorNext(iterator)) != 0)
            {
                var keep = false;
                try
                {
                    var name = IoKit.GetName(entry);
                    if (IsFramebuffer(entry, name))
                    {
                        framebuffer = ReadFramebuffer(entry);
                        continue;
                    }
                    if (name != ProxyName || IoKit.SearchString(entry, "Location") != "External") continue;
                    found.Add((entry, framebuffer));
                    keep = true;
                }
                finally
                {
                    if (!keep) IoKit.Release(entry);
                }
            }
        }
        finally
        {
            IoKit.Release(iterator);
        }
        return found;
    }

    // m1ddc tests conformance to IOMobileFramebuffer; MonitorControl matches the names. Either marks a framebuffer.
    private static bool IsFramebuffer(uint entry, string? name) =>
        name is "AppleCLCD2" or "IOMobileFramebufferShim" || IoKit.ConformsTo(entry, "IOMobileFramebuffer\0"u8);

    private static ProxyCandidate ReadFramebuffer(uint entry)
    {
        string? manufacturer = null, downstream = null;
        long? product = null;

        var attributes = IoKit.SearchProperty(entry, "DisplayAttributes");
        try
        {
            var productAttributes = CF.GetValue(attributes, "ProductAttributes");
            manufacturer = CF.ToManagedString(CF.GetValue(productAttributes, "ManufacturerID"));
            product = CF.ToInt64(CF.GetValue(productAttributes, "ProductID"));
        }
        finally { CF.Release(attributes); }

        var transport = IoKit.SearchProperty(entry, "Transport");
        try { downstream = CF.ToManagedString(CF.GetValue(transport, "Downstream")); }
        finally { CF.Release(transport); }

        return new ProxyCandidate(IoKit.GetRegistryId(entry), manufacturer, product, downstream);
    }

    /// <summary>m1ddc: the proxy's parent has EPICProviderClass = "AppleDCPMCDP29XX" on M1-era built-in HDMI.</summary>
    private static bool IsMcdp29xx(uint proxy)
    {
        var parent = IoKit.GetParent(proxy);
        if (parent == 0) return false;
        var providerClass = IoKit.CreateProperty(parent, "EPICProviderClass");
        try { return CF.ToManagedString(providerClass) == "AppleDCPMCDP29XX"; }
        finally
        {
            CF.Release(providerClass);
            IoKit.Release(parent);
        }
    }
}
