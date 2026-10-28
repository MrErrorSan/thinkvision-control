using System.Runtime.Versioning;
using ThinkVisionControl.Avalonia.Platform.Shared;
using ThinkVisionControl.Avalonia.Views;
using ThinkVisionControl.Core.Ddc;

namespace ThinkVisionControl.Avalonia.Platform.MacOS;

/// <summary>The macOS platform set. Build it on the UI (main) thread, after Avalonia has started.</summary>
[SupportedOSPlatform("macos")]
public static class MacPlatform
{
    public static PlatformServices Create(Func<IMonitorLink> createLink, IOsd overlayOsd)
    {
        OverlayOsd.ConfigureWindow = MacWindowTweaks.MakeOverlay;
        return new PlatformServices(
            MonitorEvents: new MacMonitorEvents(),
            Hotkeys: new MacHotkeys(),
            // The menu bar always exists.
            Tray: new AvaloniaTray(hostAvailable: () => true),
            Osd: overlayOsd,
            Autostart: new MacAutostart(),
            QuickActions: new MacQuickActions(),
            SingleInstance: new UnixSingleInstance(),
            Theme: new MacThemeInfo(),
            Setup: new MacSystemSetup(),
            CreateLink: createLink);
    }
}
