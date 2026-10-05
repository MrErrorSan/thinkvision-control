using Avalonia;
using ThinkVisionControl.Core.Control;

namespace ThinkVisionControl.Avalonia.Platform.Null;

/// <summary>No tray host: nothing is shown and there is no flyout.</summary>
public sealed class NullTray : ITray
{
    public bool IsHostAvailable => false;
    public void Show(Func<IReadOnlyList<TrayMenuItem>> menu, Action primaryClick) { }
    public void SetStatus(MonitorStatus status) { }
    public Point? FlyoutOrigin(Size flyout) => null;
    public void Dispose() { }
}