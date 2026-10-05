namespace ThinkVisionControl.Avalonia.Platform.Null;

/// <summary>No plug/unplug notifications; the app relies on its own re-checks.</summary>
public sealed class NullMonitorEvents : IMonitorEvents
{
    public event Action? Changed { add { } remove { } }
    public void Dispose() { }
}