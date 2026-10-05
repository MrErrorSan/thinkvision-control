namespace ThinkVisionControl.Avalonia.Platform;

/// <summary>Raised when a monitor is plugged, unplugged or reconfigured, or the machine wakes.</summary>
public interface IMonitorEvents : IDisposable
{
    event Action? Changed;
}