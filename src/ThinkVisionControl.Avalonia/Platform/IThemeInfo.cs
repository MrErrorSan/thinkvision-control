using Avalonia.Media;

namespace ThinkVisionControl.Avalonia.Platform;

/// <summary>The OS appearance. <see cref="Accent"/> null → the app's fallback (system accent where Avalonia reads one, else ember).</summary>
public interface IThemeInfo
{
    bool IsDark { get; }
    Color? Accent { get; }
    event Action? Changed;
}