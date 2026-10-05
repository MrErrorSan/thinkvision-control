using Avalonia;
using Avalonia.Controls;

namespace ThinkVisionControl.Avalonia;

public static class Program
{
    [STAThread]
    public static void Main(string[] args) =>
        BuildAvaloniaApp().StartWithClassicDesktopLifetime(args, ShutdownMode.OnExplicitShutdown);

    public static AppBuilder BuildAvaloniaApp() =>
        AppBuilder.Configure<App>()
            .UsePlatformDetect()
            // Linux: the X11 backend (XWayland on Wayland sessions). Native Wayland can't position windows
            // or keep them on top, so it stays off; only the portal paths in Platform/ are Wayland-specific.
            .With(new X11PlatformOptions())
            // macOS: a menu-bar app, no Dock icon.
            .With(new MacOSPlatformOptions { ShowInDock = false });
    // Windows: Win32 defaults.
}
