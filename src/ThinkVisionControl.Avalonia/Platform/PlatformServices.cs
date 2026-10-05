using ThinkVisionControl.Avalonia.Platform.Null;
using ThinkVisionControl.Core.Ddc;

namespace ThinkVisionControl.Avalonia.Platform;

/// <summary>The OS services the app uses, one implementation per OS.</summary>
public sealed record PlatformServices(IMonitorEvents MonitorEvents, IHotkeys Hotkeys, ITray Tray, IOsd Osd,
    IAutostart Autostart, IQuickActions QuickActions, ISingleInstance SingleInstance, IThemeInfo Theme, ISystemSetup Setup, Func<IMonitorLink> CreateLink)
{
    /// <summary>The degraded set: everything reports unavailable and does nothing. Only <paramref name="createLink"/> is real.</summary>
    public static PlatformServices CreateNull(Func<IMonitorLink> createLink) => new(
        new NullMonitorEvents(), new NullHotkeys(), new NullTray(), new NullOsd(),
        new NullAutostart(), new NullQuickActions(), new NullSingleInstance(), new NullThemeInfo(), new NullSystemSetup(), createLink);
}