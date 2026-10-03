using System.Drawing;
using System.Drawing.Drawing2D;
using System.Windows.Forms;

namespace ThinkVisionControl.Tray;

/// <summary>Tray icon: left-click opens the panel, right-click menu has Open and Exit. Grey when the monitor is not responding.</summary>
public sealed class TrayIcon : IDisposable
{
    const string NormalText = "ThinkVision Control";
    const string NotRespondingText = "Monitor not responding — DDC/CI may be off (hold Exit for 10 s)";

    readonly NotifyIcon _icon;
    readonly Icon _normal = CreateIcon(Color.FromArgb(230, 230, 230));
    readonly Icon _grey = CreateIcon(Color.FromArgb(110, 110, 110));

    public TrayIcon(Action open, Action exit)
    {
        var menu = new ContextMenuStrip();
        menu.Items.Add("Open panel", null, (_, _) => open());
        menu.Items.Add("Exit", null, (_, _) => exit());

        _icon = new NotifyIcon { Icon = _grey, Text = NotRespondingText, ContextMenuStrip = menu, Visible = true };
        _icon.MouseClick += (_, e) => { if (e.Button == MouseButtons.Left) open(); };
    }

    public void SetResponding(bool responding)
    {
        _icon.Icon = responding ? _normal : _grey;
        _icon.Text = responding ? NormalText : NotRespondingText;
    }

    public void Dispose()
    {
        _icon.Visible = false;
        _icon.Dispose();
        _normal.Dispose();
        _grey.Dispose();
    }

    // A simple monitor glyph: screen outline plus stand.
    static Icon CreateIcon(Color color)
    {
        using var bmp = new Bitmap(32, 32);
        using (var g = Graphics.FromImage(bmp))
        {
            g.SmoothingMode = SmoothingMode.AntiAlias;
            using var pen = new Pen(color, 3);
            using var brush = new SolidBrush(color);
            g.DrawRectangle(pen, 3, 4, 26, 18);
            g.FillRectangle(brush, 14, 22, 4, 4);
            g.FillRectangle(brush, 9, 26, 14, 3);
        }
        var handle = bmp.GetHicon();
        try { return (Icon)Icon.FromHandle(handle).Clone(); }
        finally { DestroyIcon(handle); }
    }

    [System.Runtime.InteropServices.DllImport("user32.dll")]
    static extern bool DestroyIcon(IntPtr handle);
}
