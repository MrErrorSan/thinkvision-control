using System.Runtime.InteropServices;
using System.Windows;
using System.Windows.Interop;
using System.Windows.Threading;

namespace ThinkVisionControl.Ui;

/// <summary>Click-through, non-activating feedback box near the bottom-centre of the primary screen.</summary>
public partial class OsdWindow : Window
{
    const int GWL_EXSTYLE = -20;
    const long WS_EX_TRANSPARENT = 0x20, WS_EX_TOOLWINDOW = 0x80, WS_EX_LAYERED = 0x80000, WS_EX_NOACTIVATE = 0x8000000;
    const double BottomOffset = 120;

    static OsdWindow? _instance;
    readonly DispatcherTimer _hideTimer = new() { Interval = TimeSpan.FromSeconds(1) };

    OsdWindow()
    {
        InitializeComponent();
        _hideTimer.Tick += (_, _) =>
        {
            _hideTimer.Stop();
            Hide();
        };
        SizeChanged += (_, _) => Reposition(ActualWidth, ActualHeight);
    }

    /// <summary>Shows <paramref name="text"/> for about a second. Must be called on the UI thread.</summary>
    public static void ShowMessage(string text)
    {
        var osd = _instance ??= new OsdWindow();
        osd.Message.Text = text;
        osd.Box.Measure(new Size(double.PositiveInfinity, double.PositiveInfinity));
        osd.Reposition(osd.Box.DesiredSize.Width, osd.Box.DesiredSize.Height);
        osd.Show();
        osd._hideTimer.Stop();
        osd._hideTimer.Start();
    }

    void Reposition(double width, double height)
    {
        var area = SystemParameters.WorkArea;
        Left = area.Left + (area.Width - width) / 2;
        Top = area.Bottom - BottomOffset - height;
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
