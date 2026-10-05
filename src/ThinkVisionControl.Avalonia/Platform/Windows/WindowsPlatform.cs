using System.Runtime.Versioning;
using ThinkVisionControl.Avalonia.Platform.Shared;
using ThinkVisionControl.Core.Ddc;

namespace ThinkVisionControl.Avalonia.Platform.Windows;

/// <summary>
/// The Windows service set. Call on the UI thread: the hotkey, monitor-event and theme windows belong to the calling
/// thread, whose message loop (Avalonia's) must pump them. <see cref="EfficiencyMode"/> is static; the shell calls it.
/// </summary>
[SupportedOSPlatform("windows")]
public static class WindowsPlatform
{
    public static PlatformServices Create(Func<IMonitorLink> createLink, IOsd overlayOsd) => new(
        new WindowsMonitorEvents(),
        new WindowsHotkeys(),
        new AvaloniaTray(WindowsTrayAnchor.FromCursor, static () => true),
        overlayOsd,
        new WindowsAutostart(),
        new WindowsQuickActions(),
        new WindowsSingleInstance(),
        new WindowsThemeInfo(),
        new WindowsSystemSetup(),
        createLink);
}
