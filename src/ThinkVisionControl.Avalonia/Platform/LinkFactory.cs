using ThinkVisionControl.Core.Ddc;
using ThinkVisionControl.Ddc.Windows;

namespace ThinkVisionControl.Avalonia.Platform;

/// <summary>The monitor link for the current OS.</summary>
public static class LinkFactory
{
    public static IMonitorLink Create(string edidId)
    {
        if (OperatingSystem.IsWindows()) return new Dxva2MonitorLink(edidId);
        // The Linux (I2C) and macOS (IOAVService) links are added by the ddc-linux and ddc-macos tasks.
        throw new PlatformNotSupportedException($"No monitor link for {System.Runtime.InteropServices.RuntimeInformation.OSDescription} yet.");
    }
}