using Avalonia;
using Avalonia.Animation;
using Avalonia.Controls;
using Avalonia.Layout;
using Avalonia.Media;
using Avalonia.Threading;
using ThinkVisionControl.Avalonia.Platform;
using ThinkVisionControl.Avalonia.ViewModels;

namespace ThinkVisionControl.Avalonia.Views;

/// <summary>
/// IOsd as a small topmost, non-activating overlay window at the bottom centre of the primary screen's work area.
/// Fades in (167 ms), stays about 1.2 s after the last update, fades out and raises <see cref="Hidden"/>.
/// Must be used on the UI thread.
/// </summary>
public sealed class OverlayOsd : IOsd
{
    const double BottomOffset = 96, ShadowRoom = 24;
    static readonly TimeSpan Motion = TimeSpan.FromMilliseconds(167), Hold = TimeSpan.FromMilliseconds(1200);

    /// <summary>Set by the platform layer to apply OS-specific window tweaks (e.g. click-through on macOS); called once per window.</summary>
    public static Action<Window>? ConfigureWindow { get; set; }

    readonly DispatcherTimer _hideTimer = new() { Interval = Hold };
    readonly DispatcherTimer _fadeTimer = new() { Interval = Motion };
    Window? _window;
    Border? _card;
    TextBlock? _message, _levelIcon, _levelLabel, _levelValue;
    Panel? _level;
    ColumnDefinition? _filled, _rest;

    public OverlayOsd()
    {
        _hideTimer.Tick += (_, _) => FadeOut();
        _fadeTimer.Tick += (_, _) => FadeOutCompleted();
    }

    public event Action? Hidden;

    public void ShowMessage(string text) => Present(() =>
    {
        _message!.Text = text;
        _message.IsVisible = true;
        _level!.IsVisible = false;
    });

    public void ShowLevel(string label, uint value, uint max) => Present(() =>
    {
        var icon = label.Contains("bright", StringComparison.OrdinalIgnoreCase) ? Glyphs.Brightness
            : label.Contains("contrast", StringComparison.OrdinalIgnoreCase) ? Glyphs.Contrast : "";
        _levelIcon!.Text = icon;
        _levelIcon.IsVisible = icon.Length > 0;
        _levelLabel!.Text = label;
        _levelValue!.Text = value.ToString();
        var filled = Math.Min(value, max);
        _filled!.Width = new GridLength(Math.Max(filled, 0.0001), GridUnitType.Star);
        _rest!.Width = new GridLength(Math.Max(max - filled, 0.0001), GridUnitType.Star);
        _message!.IsVisible = false;
        _level!.IsVisible = true;
    });

    void Present(Action update)
    {
        var window = _window ??= Create();
        update();
        _hideTimer.Stop();
        _fadeTimer.Stop();

        Reposition();
        if (!window.IsVisible)
        {
            _card!.Opacity = 0;
            window.Show();
            Dispatcher.UIThread.Post(() => _card.Opacity = 1, DispatcherPriority.Render);
        }
        else
        {
            _card!.Opacity = 1; // cancels a fade-out from where it is
        }
        _hideTimer.Start();
    }

    void FadeOut()
    {
        _hideTimer.Stop();
        _card!.Opacity = 0;
        _fadeTimer.Start();
    }

    void FadeOutCompleted()
    {
        _fadeTimer.Stop();
        _window?.Hide();
        Hidden?.Invoke();
    }

    // The card's bottom edge sits BottomOffset above the work area; the window margin is room for the shadow.
    void Reposition()
    {
        var window = _window!;
        var screen = window.Screens.Primary ?? window.Screens.All.FirstOrDefault();
        if (screen is null) return;
        var root = (Control)window.Content!;
        root.Measure(Size.Infinity);
        var scale = screen.Scaling;
        var area = screen.WorkingArea;
        var width = root.DesiredSize.Width * scale;
        var height = root.DesiredSize.Height * scale;
        window.Position = new PixelPoint(
            (int)(area.X + (area.Width - width) / 2),
            (int)(area.Bottom - (BottomOffset - _card!.Margin.Bottom) * scale - height));
    }

    Window Create()
    {
        _message = new TextBlock
        {
            FontSize = 18, LineHeight = 24, TextWrapping = TextWrapping.Wrap, TextAlignment = TextAlignment.Center,
        };
        _message.Classes.Add("BodyLarge");

        _levelIcon = new TextBlock { FontFamily = Converters.IconFont, FontSize = 16, VerticalAlignment = VerticalAlignment.Center, Margin = new Thickness(0, 0, 12, 0) };
        _levelValue = new TextBlock { FontSize = 18, LineHeight = 24, FontWeight = FontWeight.SemiBold, VerticalAlignment = VerticalAlignment.Center, Margin = new Thickness(16, 0, 0, 0) };
        _levelLabel = new TextBlock { FontSize = 14, VerticalAlignment = VerticalAlignment.Center, TextTrimming = TextTrimming.CharacterEllipsis };
        _levelLabel.Bind(TextBlock.ForegroundProperty, _levelLabel.GetResourceObservable("TextFillColorSecondaryBrush"));
        DockPanel.SetDock(_levelIcon, Dock.Left);
        DockPanel.SetDock(_levelValue, Dock.Right);

        _filled = new ColumnDefinition(1, GridUnitType.Star);
        _rest = new ColumnDefinition(1, GridUnitType.Star);
        var track = new Border { CornerRadius = new CornerRadius(2) };
        track.Bind(Border.BackgroundProperty, track.GetResourceObservable("ControlStrongStrokeColorDefaultBrush"));
        Grid.SetColumnSpan(track, 2);
        var fill = new Border { CornerRadius = new CornerRadius(2) };
        fill.Bind(Border.BackgroundProperty, fill.GetResourceObservable("AccentFillColorDefaultBrush"));
        var bar = new Grid { Height = 4, Margin = new Thickness(0, 10, 0, 0), ColumnDefinitions = { _filled, _rest }, Children = { track, fill } };

        _level = new StackPanel
        {
            Children = { new DockPanel { Children = { _levelIcon, _levelValue, _levelLabel } }, bar },
        };

        _card = new Border
        {
            CornerRadius = new CornerRadius(8),
            BorderThickness = new Thickness(1),
            Padding = new Thickness(16, 12),
            MinWidth = 240,
            MaxWidth = 420,
            Margin = new Thickness(ShadowRoom),
            BoxShadow = BoxShadows.Parse("0 4 16 0 #48000000"),
            Child = new StackPanel { Children = { _message, _level } },
            Transitions = [new DoubleTransition { Property = Visual.OpacityProperty, Duration = Motion }],
            IsHitTestVisible = false,
        };
        _card.Bind(Border.BackgroundProperty, _card.GetResourceObservable("SolidBackgroundFillColorBaseBrush"));
        _card.Bind(Border.BorderBrushProperty, _card.GetResourceObservable("SurfaceStrokeColorDefaultBrush"));

        var window = new Window
        {
            Title = "ThinkVision Control OSD",
            WindowDecorations = WindowDecorations.None,
            TransparencyLevelHint = [WindowTransparencyLevel.Transparent],
            Background = Brushes.Transparent,
            Topmost = true,
            ShowActivated = false,
            ShowInTaskbar = false,
            CanResize = false,
            Focusable = false,
            IsHitTestVisible = false,
            SizeToContent = SizeToContent.WidthAndHeight,
            WindowStartupLocation = WindowStartupLocation.Manual,
            Content = _card,
        };
        window.Opened += (_, _) =>
        {
            if (OperatingSystem.IsWindows()) MakeClickThrough(window);
            // No compositor (e.g. a bare X11 session): the transparent shadow margin would paint black, so drop it.
            if (window.ActualTransparencyLevel == WindowTransparencyLevel.None)
            {
                _card.Margin = default;
                _card.BoxShadow = default;
                Reposition();
            }
        };
        ConfigureWindow?.Invoke(window);
        return window;
    }

    // Windows: the OSD never takes focus or a taskbar button, and clicks pass through it (as the WPF OsdWindow).
    // Click-through needs WS_EX_LAYERED; that is only safe on a composition-rendered window (no redirection bitmap),
    // where full layered alpha leaves Avalonia's per-pixel transparency intact.
    [System.Runtime.Versioning.SupportedOSPlatform("windows")]
    static void MakeClickThrough(Window window)
    {
        if (window.TryGetPlatformHandle()?.Handle is not { } hwnd || hwnd == IntPtr.Zero) return;
        const int GwlExStyle = -20;
        const long WsExTransparent = 0x20, WsExToolWindow = 0x80, WsExLayered = 0x80000, WsExNoRedirectionBitmap = 0x200000,
            WsExNoActivate = 0x8000000;
        const uint LwaAlpha = 0x2;
        var style = GetWindowLongPtr(hwnd, GwlExStyle).ToInt64() | WsExToolWindow | WsExNoActivate;
        var composited = (style & WsExNoRedirectionBitmap) != 0;
        if (composited) style |= WsExLayered | WsExTransparent;
        SetWindowLongPtr(hwnd, GwlExStyle, new IntPtr(style));
        if (composited) SetLayeredWindowAttributes(hwnd, 0, 255, LwaAlpha);
    }

    [System.Runtime.InteropServices.DllImport("user32.dll", EntryPoint = "GetWindowLongPtrW")]
    static extern IntPtr GetWindowLongPtr(IntPtr hWnd, int nIndex);

    [System.Runtime.InteropServices.DllImport("user32.dll", EntryPoint = "SetWindowLongPtrW")]
    static extern IntPtr SetWindowLongPtr(IntPtr hWnd, int nIndex, IntPtr dwNewLong);

    [System.Runtime.InteropServices.DllImport("user32.dll")]
    static extern bool SetLayeredWindowAttributes(IntPtr hWnd, uint crKey, byte bAlpha, uint dwFlags);
}
