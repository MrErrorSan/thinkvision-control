using Avalonia;
using ThinkVisionControl.Core.Control;

namespace ThinkVisionControl.Avalonia.Platform.Shared;

/// <summary>
/// ITray on Avalonia's TrayIcon (Windows notification area, macOS menu bar, Linux StatusNotifierItem).
/// Placeholder: the platform-windows task implements it; macOS and Linux reuse it.
/// </summary>
public sealed class AvaloniaTray(Func<Size, Point?> flyoutOrigin, Func<bool> hostAvailable) : ITray
{
    public bool IsHostAvailable => hostAvailable();
    public void Show(Func<IReadOnlyList<TrayMenuItem>> menu, Action primaryClick) => throw new NotImplementedException();
    public void SetStatus(MonitorStatus status) => throw new NotImplementedException();
    public Point? FlyoutOrigin(Size flyout) => flyoutOrigin(flyout);
    public void Dispose() { }
}
