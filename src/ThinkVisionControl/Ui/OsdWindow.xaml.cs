using System.Runtime.InteropServices;
using System.Windows;
using System.Windows.Interop;
using System.Windows.Media.Animation;
using System.Windows.Threading;

namespace ThinkVisionControl.Ui;

/// <summary>
/// Click-through, non-activating feedback box at the bottom-centre of the primary work area. Fades and slides in
/// (167 ms), stays about 1.2 s after the last update, fades out and raises <see cref="Hidden"/>.
/// </summary>
public partial class OsdWindow : Window
{
    const int GWL_EXSTYLE = -20;
    const long WS_EX_TRANSPARENT = 0x20, WS_EX_TOOLWINDOW = 0x80, WS_EX_LAYERED = 0x80000, WS_EX_NOACTIVATE = 0x8000000;
    const double BottomOffset = 96, SlideDistance = 12;
    static readonly Duration Motion = TimeSpan.FromMilliseconds(167);

    static OsdWindow? _instance;
    readonly DispatcherTimer _hideTimer = new() { Interval = TimeSpan.FromMilliseconds(1200) };
    int _fadeOut; // non-zero while a fade-out is running; bumped to cancel it

    OsdWindow()
    {
        InitializeComponent();
        _hideTimer.Tick += (_, _) => FadeOut();
        SizeChanged += (_, _) => Reposition(ActualWidth, ActualHeight);
    }

    /// <summary>Raised on the UI thread once the OSD has faded out and hidden.</summary>
    public static event Action? Hidden;

    /// <summary>Shows <paramref name="text"/> for about a second. Must be called on the UI thread.</summary>
    public static void ShowMessage(string text) => Present(osd =>
    {
        osd.Message.Text = text;
        osd.Message.Visibility = Visibility.Visible;
        osd.Level.Visibility = Visibility.Collapsed;
    });

    /// <summary>Shows <paramref name="label"/>, the value and a level bar for about a second. UI thread only.</summary>
    public static void ShowLevel(string label, uint value, uint max) => Present(osd =>
    {
        var icon = label.Contains("bright", StringComparison.OrdinalIgnoreCase) ? ""
            : label.Contains("contrast", StringComparison.OrdinalIgnoreCase) ? "" : "";
        osd.LevelIcon.Text = icon;
        osd.LevelIcon.Visibility = icon.Length == 0 ? Visibility.Collapsed : Visibility.Visible;
        osd.LevelLabel.Text = label;
        osd.LevelValue.Text = value.ToString();
        var filled = Math.Min(value, max);
        osd.LevelFilled.Width = new GridLength(filled, GridUnitType.Star);
        osd.LevelRest.Width = new GridLength(max - filled, GridUnitType.Star);
        osd.Message.Visibility = Visibility.Collapsed;
        osd.Level.Visibility = Visibility.Visible;
    });

    static void Present(Action<OsdWindow> update)
    {
        var osd = _instance ??= new OsdWindow();
        update(osd);
        osd._hideTimer.Stop();

        osd.Root.Measure(new Size(double.PositiveInfinity, double.PositiveInfinity));
        osd.Reposition(osd.Root.DesiredSize.Width, osd.Root.DesiredSize.Height);
        if (!osd.IsVisible) osd.PlayEntrance(fresh: true);
        else if (osd._fadeOut != 0) osd.PlayEntrance(fresh: false); // cancel the fade-out from where it is
        osd._fadeOut = 0;
        osd.Show();
        osd._hideTimer.Start();
    }

    void PlayEntrance(bool fresh)
    {
        Root.BeginAnimation(OpacityProperty, new DoubleAnimation { From = fresh ? 0 : null, To = 1, Duration = Motion });
        if (!fresh || !SystemParameters.ClientAreaAnimation) return;
        var ease = new ExponentialEase { EasingMode = EasingMode.EaseOut, Exponent = 7 };
        Slide.BeginAnimation(System.Windows.Media.TranslateTransform.YProperty,
            new DoubleAnimation(SlideDistance, 0, Motion) { EasingFunction = ease });
    }

    void FadeOut()
    {
        _hideTimer.Stop();
        var token = ++_fadeOut;
        var fade = new DoubleAnimation(0, Motion);
        fade.Completed += (_, _) =>
        {
            if (_fadeOut != token) return; // shown again meanwhile
            _fadeOut = 0;
            Hide();
            Hidden?.Invoke();
        };
        Root.BeginAnimation(OpacityProperty, fade);
    }

    // The box's bottom edge sits BottomOffset above the work area; the root margin is room for the shadow.
    void Reposition(double width, double height)
    {
        var area = SystemParameters.WorkArea;
        Left = area.Left + (area.Width - width) / 2;
        Top = area.Bottom - BottomOffset - height + Root.Margin.Bottom;
    }

    protected override void OnSourceInitialized(EventArgs e)
    {
        base.OnSourceInitialized(e);
        var hwnd = new WindowInteropHelper(this).Handle;
        var style = GetExStyle(hwnd) | WS_EX_TRANSPARENT | WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW | WS_EX_LAYERED;
        SetExStyle(hwnd, style);
    }

    static long GetExStyle(IntPtr hwnd) =>
        IntPtr.Size == 8 ? GetWindowLongPtr(hwnd, GWL_EXSTYLE).ToInt64() : GetWindowLong(hwnd, GWL_EXSTYLE);

    static void SetExStyle(IntPtr hwnd, long style)
    {
        if (IntPtr.Size == 8) SetWindowLongPtr(hwnd, GWL_EXSTYLE, new IntPtr(style));
        else SetWindowLong(hwnd, GWL_EXSTYLE, (int)style);
    }

    [DllImport("user32.dll", EntryPoint = "GetWindowLongPtrW")]
    static extern IntPtr GetWindowLongPtr(IntPtr hWnd, int nIndex);

    [DllImport("user32.dll", EntryPoint = "SetWindowLongPtrW")]
    static extern IntPtr SetWindowLongPtr(IntPtr hWnd, int nIndex, IntPtr dwNewLong);

    [DllImport("user32.dll", EntryPoint = "GetWindowLongW")]
    static extern int GetWindowLong(IntPtr hWnd, int nIndex);

    [DllImport("user32.dll", EntryPoint = "SetWindowLongW")]
    static extern int SetWindowLong(IntPtr hWnd, int nIndex, int dwNewLong);
}
