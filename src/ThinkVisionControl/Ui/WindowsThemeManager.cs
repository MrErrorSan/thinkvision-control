using Microsoft.Win32;
using System.Runtime.InteropServices;
using System.Windows;
using System.Windows.Interop;
using System.Windows.Media;
using System.Windows.Threading;

namespace ThinkVisionControl.Ui;

/// <summary>Follows the Windows app theme: swaps the brush palette and the native title-bar colour of every window.</summary>
public sealed class WindowsThemeManager : IDisposable
{
    const int DwmwaUseImmersiveDarkMode = 20;
    const string PersonalizeKey = @"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize";

    static readonly IReadOnlyDictionary<string, string> LightPalette = new Dictionary<string, string>
    {
        ["BgBrush"] = "#F3F3F3",
        ["SurfaceBrush"] = "#FFFFFF",
        ["SurfaceHoverBrush"] = "#E9E9E9",
        ["SurfacePressedBrush"] = "#DCDCDC",
        ["LineBrush"] = "#E5E5E5",
        ["AccentBrush"] = "#0067C0",
        ["AccentHoverBrush"] = "#005A9E",
        ["TextBrush"] = "#1A1A1A",
        ["MutedTextBrush"] = "#616161",
        ["DisabledTextBrush"] = "#9D9D9D",
        ["WarnBrush"] = "#9D5D00",
        ["GoodBrush"] = "#107C10",
        ["OffBrush"] = "#777777",
        ["ThumbBrush"] = "#5F5F5F",
    };

    static readonly IReadOnlyDictionary<string, string> DarkPalette = new Dictionary<string, string>
    {
        ["BgBrush"] = "#202020",
        ["SurfaceBrush"] = "#2B2B2B",
        ["SurfaceHoverBrush"] = "#373737",
        ["SurfacePressedBrush"] = "#414141",
        ["LineBrush"] = "#3F3F3F",
        ["AccentBrush"] = "#60CDFF",
        ["AccentHoverBrush"] = "#4CC2FF",
        ["TextBrush"] = "#F3F3F3",
        ["MutedTextBrush"] = "#A0A0A0",
        ["DisabledTextBrush"] = "#737373",
        ["WarnBrush"] = "#F6C344",
        ["GoodBrush"] = "#6CCB7B",
        ["OffBrush"] = "#7C7C7C",
        ["ThumbBrush"] = "#D5D5D5",
    };

    readonly Application _application;
    bool _disposed;
    bool _light = true;

    public WindowsThemeManager(Application application)
    {
        _application = application;
        Apply();
        // Windows does not darken a Win32 title bar on its own; each window must opt in once it has a handle.
        EventManager.RegisterClassHandler(typeof(Window), FrameworkElement.LoadedEvent,
            new RoutedEventHandler((sender, _) => { if (!_disposed) SetTitleBar((Window)sender, !_light); }));
        SystemEvents.UserPreferenceChanged += UserPreferenceChanged;
    }

    public void Dispose()
    {
        if (_disposed) return;
        _disposed = true;
        SystemEvents.UserPreferenceChanged -= UserPreferenceChanged;
    }

    void UserPreferenceChanged(object? sender, UserPreferenceChangedEventArgs e)
    {
        if (_disposed) return;
        _application.Dispatcher.BeginInvoke(DispatcherPriority.Normal, Apply);
    }

    void Apply()
    {
        using var key = Registry.CurrentUser.OpenSubKey(PersonalizeKey);
        var light = key?.GetValue("AppsUseLightTheme", 1) is int value && value != 0;
        _light = light;
        var palette = light ? LightPalette : DarkPalette;
        foreach (var (name, hex) in palette)
        {
            var color = (Color)ColorConverter.ConvertFromString(hex);
            var brush = new SolidColorBrush(color);
            brush.Freeze();
            _application.Resources[name] = brush;
        }

        foreach (Window window in _application.Windows) SetTitleBar(window, !light);
    }

    static void SetTitleBar(Window window, bool dark)
    {
        var handle = new WindowInteropHelper(window).Handle;
        if (handle == IntPtr.Zero) return;
        var value = dark ? 1 : 0;
        DwmSetWindowAttribute(handle, DwmwaUseImmersiveDarkMode, ref value, sizeof(int));
    }

    [DllImport("dwmapi.dll")]
    static extern int DwmSetWindowAttribute(IntPtr hwnd, int attribute, ref int value, int size);
}
