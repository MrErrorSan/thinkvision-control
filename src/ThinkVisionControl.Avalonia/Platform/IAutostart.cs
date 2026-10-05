namespace ThinkVisionControl.Avalonia.Platform;

/// <summary>Start with the user session.</summary>
public interface IAutostart
{
    bool IsEnabled { get; }
    void Set(bool enabled);
}