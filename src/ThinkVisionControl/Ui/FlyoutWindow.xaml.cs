using System.ComponentModel;
using System.Runtime.InteropServices;
using System.Windows;
using System.Windows.Input;
using System.Windows.Interop;
using System.Windows.Media;
using System.Windows.Media.Animation;

namespace ThinkVisionControl.Ui;

/// <summary>
/// Windows 11 Quick Settings-style tray flyout hosting <see cref="QuickControlsView"/>: acrylic
/// (<c>DWMSBT_TRANSIENTWINDOW</c>), rounded corners, light-dismiss on deactivate or Esc. Create one lazily and reuse it.
/// </summary>
public partial class FlyoutWindow : Window
{
    const double FlyoutWidth = 360, SlideDistance = 12;
    const long ReopenGuardMs = 400;
    static readonly Duration Entrance = TimeSpan.FromMilliseconds(167);

    const int DwmwaUseImmersiveDarkMode = 20, DwmwaWindowCornerPreference = 33, DwmwaSystemBackdropType = 38;
    const int DwmwcpRound = 2, DwmsbtTransientWindow = 3;
    const int GwlStyle = -16, WsCaption = 0x00C00000, WsSysMenu = 0x00080000;
    const uint SwpFrameChanged = 0x20, SwpNoSizeMoveZOrderActivate = 0x1 | 0x2 | 0x4 | 0x10;

    static FlyoutWindow? _current;
    long _hiddenAt;
    bool _shown;

    public FlyoutWindow(PanelViewModel vm, Action openSettings)
    {
        InitializeComponent();
        OpenSettings = openSettings;
        QuickControls.DataContext = vm;
        _current = this;
        SizeChanged += OnSizeChanged;
    }

    /// <summary>Opens the full window; the app shell invokes it (e.g. from the view model's "All settings" request).</summary>
    public Action OpenSettings { get; }

    /// <summary>The flyout's size in DIPs: fixed width, height measured from the current content.</summary>
    public static Size PopupSize
    {
        get
        {
            if (_current?.Content is not UIElement root) return new Size(FlyoutWidth, 480);
            root.Measure(new Size(FlyoutWidth, double.PositiveInfinity));
            return new Size(FlyoutWidth, Math.Ceiling(root.DesiredSize.Height));
        }
    }

    /// <summary>Places the flyout's top-left at <paramref name="origin"/> (DIPs), shows and activates it.</summary>
    public void ShowAt(Point origin)
    {
        if (IsVisible)
        {
            Activate();
            return;
        }
        // A click on the tray icon while the flyout is open deactivates (hides) it first; don't reopen on that click.
        if (Environment.TickCount64 - _hiddenAt < ReopenGuardMs) return;

        BeginAnimation(TopProperty, null);
        Left = origin.X;
        Top = origin.Y;
        PlayEntrance(origin.Y);
        Show();
        ApplyChrome();
        Activate();
        _shown = true;
    }

    protected override void OnSourceInitialized(EventArgs e)
    {
        base.OnSourceInitialized(e);
        ApplyChrome();
    }

    protected override void OnDeactivated(EventArgs e)
    {
        base.OnDeactivated(e);
        if (!IsVisible) return;
        _hiddenAt = Environment.TickCount64;
        Dismiss();
    }

    protected override void OnKeyDown(KeyEventArgs e)
    {
        base.OnKeyDown(e);
        if (e.Key != Key.Escape || e.Handled) return;
        e.Handled = true;
        Dismiss();
    }

    // Reused: closing (Alt+F4) only hides it. WPF ignores the cancel while the application is shutting down.
    protected override void OnClosing(CancelEventArgs e)
    {
        base.OnClosing(e);
        e.Cancel = true;
        Dismiss();
    }

    void Dismiss()
    {
        _shown = false;
        Hide();
    }

    // Keep the bottom edge (next to the taskbar) fixed when the content grows or shrinks while open.
    void OnSizeChanged(object sender, SizeChangedEventArgs e)
    {
        if (_shown && e.HeightChanged && e.PreviousSize.Height > 0) Top -= e.NewSize.Height - e.PreviousSize.Height;
    }

    void PlayEntrance(double top)
    {
        QuickControls.BeginAnimation(OpacityProperty, null);
        if (!SystemParameters.ClientAreaAnimation) return;
        var ease = new ExponentialEase { EasingMode = EasingMode.EaseOut, Exponent = 7 };
        BeginAnimation(TopProperty, new DoubleAnimation(top + SlideDistance, top, Entrance) { EasingFunction = ease, FillBehavior = FillBehavior.Stop });
        QuickControls.BeginAnimation(OpacityProperty, new DoubleAnimation(0, 1, Entrance) { FillBehavior = FillBehavior.Stop });
    }

    // DWM draws the acrylic, the corners and the border. Re-applied on every show because WPF's Fluent theme
    // re-applies its own backdrop and dark-mode attributes when the system theme changes.
    void ApplyChrome()
    {
        var hwnd = new WindowInteropHelper(this).Handle;
        if (hwnd == IntPtr.Zero) return;

        // DWM draws no system backdrop on a window with neither WS_CAPTION nor WS_THICKFRAME: it stays on the solid
        // fallback even while active. Add the caption (WindowChrome keeps it out of view) and drop the system menu
        // so DWM draws no caption buttons.
        var style = GetWindowLong(hwnd, GwlStyle);
        var wanted = (style | WsCaption) & ~WsSysMenu;
        if (wanted != style)
        {
            SetWindowLong(hwnd, GwlStyle, wanted);
            SetWindowPos(hwnd, IntPtr.Zero, 0, 0, 0, 0, SwpFrameChanged | SwpNoSizeMoveZOrderActivate);
        }

        var dark = TryFindResource("TextFillColorPrimaryBrush") is SolidColorBrush text && text.Color.R > 0x80 ? 1 : 0;
        DwmSetWindowAttribute(hwnd, DwmwaUseImmersiveDarkMode, ref dark, sizeof(int));
        var corner = DwmwcpRound;
        DwmSetWindowAttribute(hwnd, DwmwaWindowCornerPreference, ref corner, sizeof(int));

        var backdrop = DwmsbtTransientWindow;
        var acrylic = !SystemParameters.HighContrast && Environment.OSVersion.Version.Build >= 22621
            && DwmSetWindowAttribute(hwnd, DwmwaSystemBackdropType, ref backdrop, sizeof(int)) == 0;
        if (acrylic)
        {
            var margins = new Margins { Left = -1, Right = -1, Top = -1, Bottom = -1 };
            acrylic = DwmExtendFrameIntoClientArea(hwnd, ref margins) == 0;
        }

        if (acrylic && HwndSource.FromHwnd(hwnd)?.CompositionTarget is { } target)
        {
            target.BackgroundColor = Colors.Transparent;
            Background = Brushes.Transparent;
            FallbackChrome.ClearValue(BackgroundProperty);
            FallbackChrome.ClearValue(BorderBrushProperty);
            FallbackChrome.BorderThickness = new Thickness(0);
        }
        else
        {
            FallbackChrome.SetResourceReference(BackgroundProperty, "SolidBackgroundFillColorBaseBrush");
            FallbackChrome.SetResourceReference(BorderBrushProperty, "SurfaceStrokeColorDefaultBrush");
            FallbackChrome.BorderThickness = new Thickness(1);
        }
    }

    [StructLayout(LayoutKind.Sequential)]
    struct Margins { public int Left, Right, Top, Bottom; }

    [DllImport("dwmapi.dll")]
    static extern int DwmSetWindowAttribute(IntPtr hwnd, int attribute, ref int value, int size);

    [DllImport("dwmapi.dll")]
    static extern int DwmExtendFrameIntoClientArea(IntPtr hwnd, ref Margins margins);

    [DllImport("user32.dll", EntryPoint = "GetWindowLongW")]
    static extern int GetWindowLong(IntPtr hwnd, int index);

    [DllImport("user32.dll", EntryPoint = "SetWindowLongW")]
    static extern int SetWindowLong(IntPtr hwnd, int index, int value);

    [DllImport("user32.dll")]
    static extern bool SetWindowPos(IntPtr hwnd, IntPtr insertAfter, int x, int y, int cx, int cy, uint flags);
}
