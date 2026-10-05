namespace ThinkVisionControl.Avalonia.Platform.Null;

/// <summary>Start-with-session is not available.</summary>
public sealed class NullAutostart : IAutostart
{
    public bool IsEnabled => false;
    public void Set(bool enabled) { }
}