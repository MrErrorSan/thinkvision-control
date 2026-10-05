using Avalonia;
using Avalonia.Controls.ApplicationLifetimes;
using Avalonia.Markup.Xaml;
using Avalonia.Media;
using Avalonia.Threading;
using FluentAvalonia.Styling;
using ThinkVisionControl.Avalonia.Platform;

namespace ThinkVisionControl.Avalonia;

public partial class App : Application
{
    /// <summary>Brand accent, used when the OS provides none (Linux without a portal accent).</summary>
    public static readonly Color Ember = Color.FromRgb(0xF0, 0x5A, 0x28);

    /// <summary>The OS services. The scaffold runs on the Null set; the shell task swaps in the per-OS set.</summary>
    public PlatformServices Services { get; private set; } = null!;

    public override void Initialize() => AvaloniaXamlLoader.Load(this);

    public override void OnFrameworkInitializationCompleted()
    {
        Services = PlatformServices.CreateNull(() => LinkFactory.Create("LEN62A2"));
        ApplyAccent(Services.Theme);
        Services.Theme.Changed += () => Dispatcher.UIThread.Post(() => ApplyAccent(Services.Theme));

        if (ApplicationLifetime is IClassicDesktopStyleApplicationLifetime desktop)
        {
            var window = new MainWindow();
            // ShutdownMode is OnExplicitShutdown (tray app); until the shell lands, closing the window exits.
            window.Closed += (_, _) => desktop.Shutdown();
            desktop.MainWindow = window;
            window.Show();
        }

        base.OnFrameworkInitializationCompleted();
    }

    /// <summary>
    /// Accent rule: the platform's accent when it reports one; otherwise the system accent that FluentAvalonia reads
    /// itself on Windows and macOS; otherwise ember.
    /// </summary>
    public void ApplyAccent(IThemeInfo theme)
    {
        var fa = Styles.OfType<FluentAvaloniaTheme>().FirstOrDefault();
        if (fa is null) return;
        var systemProvidesAccent = OperatingSystem.IsWindows() || OperatingSystem.IsMacOS();
        // null → PreferUserAccentColor applies (system accent).
        fa.CustomAccentColor = theme.Accent ?? (systemProvidesAccent ? null : Ember);
    }
}
