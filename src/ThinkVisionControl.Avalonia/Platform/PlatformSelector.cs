using ThinkVisionControl.Avalonia.Platform.Linux;
using ThinkVisionControl.Avalonia.Platform.MacOS;
using ThinkVisionControl.Avalonia.Platform.Windows;
using ThinkVisionControl.Core.Ddc;

namespace ThinkVisionControl.Avalonia.Platform;

/// <summary>Picks the OS service set. Must be called on the UI thread (the Windows set creates message windows).</summary>
public static class PlatformSelector
{
    public static PlatformServices Create(Func<IMonitorLink> createLink, IOsd overlayOsd)
    {
        if (OperatingSystem.IsWindows()) return WindowsPlatform.Create(createLink, overlayOsd);
        if (OperatingSystem.IsMacOS()) return MacPlatform.Create(createLink, overlayOsd);
        if (OperatingSystem.IsLinux()) return LinuxPlatform.Create(createLink, overlayOsd);
        return PlatformServices.CreateNull(createLink);
    }
}
