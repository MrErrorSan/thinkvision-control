using System.Runtime.Versioning;
using ThinkVisionControl.Avalonia.Platform.Null;
using ThinkVisionControl.Avalonia.Platform.Shared;
using ThinkVisionControl.Core.Ddc;

namespace ThinkVisionControl.Avalonia.Platform.Linux;

/// <summary>
/// The Linux service set. The app itself always runs on Avalonia's X11 backend (XWayland on Wayland sessions);
/// only hotkeys and the OSD switch to Wayland-specific paths, because there an X11 client can neither grab keys
/// globally nor keep an overlay on top.
/// </summary>
[SupportedOSPlatform("linux")]
public static class LinuxPlatform
{
    public static PlatformServices Create(Func<IMonitorLink> createLink, IOsd overlayOsd)
    {
        var wayland = IsWaylandSession(Environment.GetEnvironmentVariable("XDG_SESSION_TYPE"), Environment.GetEnvironmentVariable("WAYLAND_DISPLAY"));
        var x11 = !string.IsNullOrEmpty(Environment.GetEnvironmentVariable("DISPLAY"));

        IHotkeys hotkeys = wayland ? new PortalHotkeys() : x11 ? new X11Hotkeys() : new NullHotkeys();
        IOsd osd = wayland ? new NotificationOsd() : overlayOsd;
        var tray = new AvaloniaTray(hostAvailable: SniHost.IsAvailable);

        return new PlatformServices(new LinuxMonitorEvents(), hotkeys, tray, osd, new LinuxAutostart(), new LinuxQuickActions(),
            new UnixSingleInstance(), new LinuxThemeInfo(), new LinuxSystemSetup(), createLink);
    }

    /// <summary>A Wayland session: <c>XDG_SESSION_TYPE=wayland</c>, or <c>WAYLAND_DISPLAY</c> set.</summary>
    public static bool IsWaylandSession(string? sessionType, string? waylandDisplay) =>
        string.Equals(sessionType, "wayland", StringComparison.OrdinalIgnoreCase) || !string.IsNullOrEmpty(waylandDisplay);
}
