using Avalonia;
using Avalonia.Media;
using Avalonia.Styling;

namespace ThinkVisionControl.Avalonia.Platform.Null;

/// <summary>No OS theme source: light/dark comes from Avalonia's own detection, and there is no accent.</summary>
public sealed class NullThemeInfo : IThemeInfo
{
    public bool IsDark => Application.Current?.ActualThemeVariant == ThemeVariant.Dark;
    public Color? Accent => null;
    public event Action? Changed { add { } remove { } }
}