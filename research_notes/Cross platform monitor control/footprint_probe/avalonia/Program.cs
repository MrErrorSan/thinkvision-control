using Avalonia;
using Avalonia.Controls;
using Avalonia.Controls.ApplicationLifetimes;
using Avalonia.Themes.Fluent;
using Avalonia.Platform;
using System;
using System.Linq;

class App : Application
{
    public override void Initialize() => Styles.Add(new FluentTheme());
    public override void OnFrameworkInitializationCompleted()
    {
        if (ApplicationLifetime is IClassicDesktopStyleApplicationLifetime d)
        {
            bool tray = Program.Args.Contains("tray");
            if (tray)
            {
                d.ShutdownMode = ShutdownMode.OnExplicitShutdown;
                var menu = new NativeMenu();
                menu.Items.Add(new NativeMenuItem("Open"));
                menu.Items.Add(new NativeMenuItem("Exit"));
                var ti = new TrayIcon { ToolTipText = "bench", Menu = menu,
                    Icon = new WindowIcon(new Avalonia.Media.Imaging.WriteableBitmap(new PixelSize(16,16), new Vector(96,96), Avalonia.Platform.PixelFormat.Bgra8888, AlphaFormat.Premul)) };
                TrayIcon.SetIcons(this, new TrayIcons { ti });
                // a hidden helper window so MainWindowHandle-based timing is not used
            }
            else
            {
                var sp = new StackPanel { Margin = new Thickness(24), Spacing = 12 };
                sp.Children.Add(new TextBlock { Text = "Brightness" });
                sp.Children.Add(new Slider { Minimum = 0, Maximum = 100, Value = 50 });
                sp.Children.Add(new ToggleSwitch { Content = "Auto" });
                sp.Children.Add(new ComboBox { ItemsSource = new[] { "sRGB", "DCI-P3" }, SelectedIndex = 0 });
                sp.Children.Add(new Button { Content = "Apply" });
                d.MainWindow = new Window { Title = "bench", Width = 600, Height = 400, Content = sp,
                    TransparencyLevelHint = new[] { WindowTransparencyLevel.Mica }, Background = Avalonia.Media.Brushes.Transparent };
            }
        }
        base.OnFrameworkInitializationCompleted();
    }
}
static class Program
{
    public static string[] Args = Array.Empty<string>();
    [STAThread]
    public static void Main(string[] args)
    {
        Args = args;
        var b = AppBuilder.Configure<App>().UsePlatformDetect();
        if (args.Contains("sw")) b = b.With(new Win32PlatformOptions { RenderingMode = new[] { Win32RenderingMode.Software } });
        b.StartWithClassicDesktopLifetime(args);
    }
}
