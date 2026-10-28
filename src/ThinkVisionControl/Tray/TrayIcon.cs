using System.Drawing;
using System.Drawing.Drawing2D;
using System.Runtime.InteropServices;
using System.Windows.Forms;
using Microsoft.Win32;
using ThinkVisionControl.Core.Control;
using ThinkVisionControl.Platform;

namespace ThinkVisionControl.Tray;

/// <summary>
/// Tray icon: the brand glyph for the taskbar's theme with a status badge, a "name – status" tooltip, left click for
/// the window, and a Windows 11-styled right-click menu rebuilt on every open.
/// WinForms' NotifyIcon already re-adds itself when Explorer broadcasts "TaskbarCreated" (its hidden top-level
/// window handles that message), so no extra listener is needed here.
/// </summary>
public sealed class TrayIcon : IDisposable
{
    const string AppName = "ThinkVision Control";

    readonly NotifyIcon _icon;
    readonly ContextMenuStrip _menu;
    readonly Font _menuFont = new("Segoe UI", 9f);
    readonly Font _defaultItemFont;
    readonly Action _open, _exit;
    readonly Func<IEnumerable<(string Label, Action Run)>> _presetItems;
    Icon? _glyph, _shown;
    bool _lightTaskbar;
    MonitorStatus _status = MonitorStatus.Connecting;

    public TrayIcon(Action open, Func<IEnumerable<(string Label, Action Run)>> presetItems, Action exit)
    {
        (_open, _exit, _presetItems) = (open, exit, presetItems);
        _defaultItemFont = new Font(_menuFont, FontStyle.Bold);
        _menu = new ContextMenuStrip { ShowImageMargin = false, ShowCheckMargin = false, Font = _menuFont, Padding = new Padding(0, 4, 0, 4) };
        // An empty strip opens with Cancel preset to true; the items are built here, so open anyway.
        _menu.Opening += (_, e) =>
        {
            BuildMenu();
            e.Cancel = false;
        };
        _menu.Opened += (_, _) => RoundCorners(_menu.Handle);

        _icon = new NotifyIcon { ContextMenuStrip = _menu };
        _icon.MouseClick += (_, e) => { if (e.Button == MouseButtons.Left) open(); };
        LoadGlyph();
        _icon.Visible = true;
        SystemEvents.UserPreferenceChanged += OnUserPreferenceChanged;
    }

    /// <summary>Updates the status badge and tooltip.</summary>
    public void SetStatus(MonitorStatus status)
    {
        _status = status;
        var old = _shown;
        _shown = status is MonitorStatus.NotFound or MonitorStatus.NotResponding ? WithBadge(_glyph!, status, _lightTaskbar) : null;
        _icon.Icon = _shown ?? _glyph;
        _icon.Text = AppName + " – " + status switch
        {
            MonitorStatus.Connected => "P32p-20",
            MonitorStatus.Connecting => "Connecting",
            MonitorStatus.NotFound => "Not connected",
            _ => "Not responding",
        };
        old?.Dispose();
    }

    public void Dispose()
    {
        SystemEvents.UserPreferenceChanged -= OnUserPreferenceChanged;
        _icon.Visible = false;
        _icon.Dispose();
        _menu.Dispose();
        _defaultItemFont.Dispose();
        _menuFont.Dispose();
        _glyph?.Dispose();
        _shown?.Dispose();
    }

    void BuildMenu()
    {
        var old = _menu.Items.Cast<ToolStripItem>().ToArray();
        _menu.Items.Clear();
        foreach (var item in old) item.Dispose();

        _menu.Renderer = new MenuRenderer(_lightTaskbar);
        _menu.Items.Add(Item("Open ThinkVision Control", _open, _defaultItemFont));
        _menu.Items.Add(new ToolStripSeparator());
        var presets = _presetItems().ToList();
        foreach (var (label, run) in presets) _menu.Items.Add(Item(label, run));
        if (presets.Count > 0) _menu.Items.Add(new ToolStripSeparator());
        _menu.Items.Add(Item("Exit", _exit));
    }

    static ToolStripMenuItem Item(string text, Action run, Font? font = null)
    {
        var item = new ToolStripMenuItem(text, null, (_, _) => run()) { Padding = new Padding(4, 5, 24, 5) };
        if (font is not null) item.Font = font;
        return item;
    }

    void OnUserPreferenceChanged(object? sender, UserPreferenceChangedEventArgs e)
    {
        if (e.Category is UserPreferenceCategory.General or UserPreferenceCategory.Color) LoadGlyph();
    }

    // The taskbar follows "Windows mode" (SystemUsesLightTheme), not the app mode.
    void LoadGlyph()
    {
        using (var key = Registry.CurrentUser.OpenSubKey(@"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize"))
            _lightTaskbar = key?.GetValue("SystemUsesLightTheme") is int value && value != 0;
        var uri = new Uri($"pack://application:,,,/Assets/Tray.{(_lightTaskbar ? "OnLight" : "OnDark")}.ico");

        var old = _glyph;
        using (var stream = System.Windows.Application.GetResourceStream(uri).Stream)
            _glyph = new Icon(stream, SystemInformation.SmallIconSize);
        SetStatus(_status);
        old?.Dispose();
    }

    /// <summary>
    /// The glyph with a status badge at the bottom right, cut out from the glyph by a 1 px transparent ring:
    /// a × in a taskbar-foreground circle for "not connected", a ! in a caution-amber circle for "not responding".
    /// </summary>
    static Icon WithBadge(Icon glyph, MonitorStatus status, bool lightTaskbar)
    {
        using var bmp = glyph.ToBitmap();
        var size = bmp.Width;
        var d = Math.Max(7f, MathF.Round(size * 0.45f));
        var badge = new RectangleF(size - d, size - d, d, d);
        var foreground = lightTaskbar ? Color.FromArgb(0x1B, 0x1B, 0x1B) : Color.White;
        var caution = lightTaskbar ? Color.FromArgb(0x9D, 0x5D, 0x00) : Color.FromArgb(0xFC, 0xE1, 0x00);
        var mark = lightTaskbar ? Color.White : Color.Black;

        using (var g = Graphics.FromImage(bmp))
        {
            g.SmoothingMode = SmoothingMode.AntiAlias;
            g.PixelOffsetMode = PixelOffsetMode.Half; // pixel centres at .5, so 1 px strokes through the badge centre stay crisp
            g.CompositingMode = CompositingMode.SourceCopy;
            using (var clear = new SolidBrush(Color.Transparent))
                g.FillEllipse(clear, RectangleF.Inflate(badge, 1, 1));
            g.CompositingMode = CompositingMode.SourceOver;
            using (var fill = new SolidBrush(status == MonitorStatus.NotResponding ? caution : foreground))
                g.FillEllipse(fill, badge);

            using var pen = new Pen(mark, Math.Max(1f, d / 7f)) { StartCap = LineCap.Round, EndCap = LineCap.Round };
            float cx = badge.X + d / 2, cy = badge.Y + d / 2;
            if (status == MonitorStatus.NotResponding)
            {
                g.DrawLine(pen, cx, badge.Y + d * 0.22f, cx, badge.Y + d * 0.55f);
                g.DrawLine(pen, cx, badge.Y + d * 0.76f, cx, badge.Y + d * 0.77f);
            }
            else
            {
                var r = d * 0.22f;
                g.DrawLine(pen, cx - r, cy - r, cx + r, cy + r);
                g.DrawLine(pen, cx - r, cy + r, cx + r, cy - r);
            }
        }

        var handle = bmp.GetHicon();
        try { return (Icon)Icon.FromHandle(handle).Clone(); }
        finally { DestroyIcon(handle); }
    }

    static void RoundCorners(IntPtr hwnd)
    {
        var round = 2; // DWMWCP_ROUND; DWM also draws the 1 px border on Windows 11
        DwmSetWindowAttribute(hwnd, 33, ref round, sizeof(int));
    }

    /// <summary>Windows 11-like menu colours: flat background, rounded hover highlight, full-width separators.</summary>
    sealed class MenuRenderer(bool light) : ToolStripProfessionalRenderer
    {
        readonly Color _back = light ? Color.FromArgb(0xF9, 0xF9, 0xF9) : Color.FromArgb(0x2C, 0x2C, 0x2C);
        readonly Color _hover = light ? Color.FromArgb(0xEA, 0xEA, 0xEA) : Color.FromArgb(0x3D, 0x3D, 0x3D);
        readonly Color _text = light ? Color.FromArgb(0x1A, 0x1A, 0x1A) : Color.White;
        readonly Color _line = light ? Color.FromArgb(0xE0, 0xE0, 0xE0) : Color.FromArgb(0x40, 0x40, 0x40);

        protected override void OnRenderToolStripBackground(ToolStripRenderEventArgs e)
        {
            using var brush = new SolidBrush(_back);
            e.Graphics.FillRectangle(brush, e.AffectedBounds);
        }

        protected override void OnRenderToolStripBorder(ToolStripRenderEventArgs e) { }

        protected override void OnRenderImageMargin(ToolStripRenderEventArgs e) { }

        protected override void OnRenderMenuItemBackground(ToolStripItemRenderEventArgs e)
        {
            if (!e.Item.Selected || !e.Item.Enabled) return;
            var r = new Rectangle(4, 1, e.Item.Width - 8, e.Item.Height - 2);
            using var path = new GraphicsPath();
            const int Diameter = 8;
            path.AddArc(r.Left, r.Top, Diameter, Diameter, 180, 90);
            path.AddArc(r.Right - Diameter, r.Top, Diameter, Diameter, 270, 90);
            path.AddArc(r.Right - Diameter, r.Bottom - Diameter, Diameter, Diameter, 0, 90);
            path.AddArc(r.Left, r.Bottom - Diameter, Diameter, Diameter, 90, 90);
            path.CloseFigure();
            using var brush = new SolidBrush(_hover);
            e.Graphics.SmoothingMode = SmoothingMode.AntiAlias;
            e.Graphics.FillPath(brush, path);
        }

        protected override void OnRenderSeparator(ToolStripSeparatorRenderEventArgs e)
        {
            var y = e.Item.Height / 2;
            using var pen = new Pen(_line);
            e.Graphics.DrawLine(pen, 0, y, e.Item.Width, y);
        }

        protected override void OnRenderItemText(ToolStripItemTextRenderEventArgs e)
        {
            e.TextColor = _text;
            base.OnRenderItemText(e);
        }
    }

    [DllImport("user32.dll")]
    static extern bool DestroyIcon(IntPtr handle);

    [DllImport("dwmapi.dll")]
    static extern int DwmSetWindowAttribute(IntPtr hwnd, int attribute, ref int value, int size);
}
