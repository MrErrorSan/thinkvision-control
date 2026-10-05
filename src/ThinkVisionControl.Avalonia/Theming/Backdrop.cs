using Avalonia.Controls;
using Avalonia.Media;

namespace ThinkVisionControl.Avalonia.Theming;

/// <summary>Window background rule (spec §4.1).</summary>
public static class Backdrop
{
    public static void Apply(Window w)
    {
        if (OperatingSystem.IsWindowsVersionAtLeast(10, 0, 22000))
        {
            // Windows 11: Mica, solid if the compositor refuses it.
            w.TransparencyLevelHint = [WindowTransparencyLevel.Mica, WindowTransparencyLevel.None];
            w.Background = Brushes.Transparent;
        }
        else
        {
            // Everywhere else Avalonia would fall back to plain transparency (a see-through window), so ask for
            // none and paint the theme's solid base colour, following light/dark changes.
            w.TransparencyLevelHint = [WindowTransparencyLevel.None];
            w.Bind(Window.BackgroundProperty,w.GetResourceObservable("SolidBackgroundFillColorBaseBrush"));
        }
    }
}
