using ThinkVisionControl.Core.Ddc;
using ThinkVisionControl.Ddc.Linux;
using ThinkVisionControl.Ddc.MacOS;
using ThinkVisionControl.Ddc.Windows;

namespace ThinkVisionControl.Avalonia.Platform;

/// <summary>The monitor link for the current OS.</summary>
public static class LinkFactory
{
    /// <param name="edidId">Monitor id as in the profile, e.g. "LEN62A2" (3-letter PNP id + 4 hex digits of product code).</param>
    public static IMonitorLink Create(string edidId)
    {
        if (OperatingSystem.IsWindows()) return new Dxva2MonitorLink(edidId);

        var manufacturer = edidId[..3];
        var product = Convert.ToUInt16(edidId[3..], 16);
        if (OperatingSystem.IsLinux()) return new I2cMonitorLink(manufacturer, product);
        if (OperatingSystem.IsMacOS()) return new IoAvMonitorLink(PnpVendor(manufacturer), product);
        throw new PlatformNotSupportedException($"No monitor link for {System.Runtime.InteropServices.RuntimeInformation.OSDescription}.");
    }

    /// <summary>The PNP id as CoreGraphics reports it ("LEN" → 0x30AE): three 5-bit letters, 'A' = 1.</summary>
    static ushort PnpVendor(string id) => (ushort)(((id[0] - 'A' + 1) << 10) | ((id[1] - 'A' + 1) << 5) | (id[2] - 'A' + 1));
}
