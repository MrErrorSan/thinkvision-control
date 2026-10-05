using System.Drawing;
using System.Drawing.Imaging;
using System.Windows.Forms;
using Microsoft.Win32;

namespace ThinkVisionControl.Tray;

/// <summary>
/// Tray icon: left-click opens the panel, right-click menu has Open and Exit.
/// Uses the brand glyph for the taskbar's theme and dims it while the monitor is not responding.
/// </summary>
public sealed class TrayIcon : IDisposable
{
    const string NormalText = "ThinkVision Control";
    const string NotRespondingText = "Monitor not responding — DDC/CI may be off (hold Exit for 10 s)";

    readonly NotifyIcon _icon;
    Icon? _normal;
    Icon? _dimmed;
    bool _responding;

    public TrayIcon(Action open, Action exit)
    {
        var menu = new ContextMenuStrip();
        menu.Items.Add("Open panel", null, (_, _) => open());
        menu.Items.Add("Exit", null, (_, _) => exit());

        _icon = new NotifyIcon { Text = NotRespondingText, ContextMenuStrip = menu };
        _icon.MouseClick += (_, e) => { if (e.Button == MouseButtons.Left) open(); };
        LoadIcons();
        _icon.Visible = true;
        SystemEvents.UserPreferenceChanged += OnUserPreferenceChanged;
    }

    public void SetResponding(bool responding)
    {
        _responding = responding;
        _icon.Icon = responding ? _normal : _dimmed;
        _icon.Text = responding ? NormalText : NotRespondingText;
    }

    public void Dispose()
    {
        SystemEvents.UserPreferenceChanged -= OnUserPreferenceChanged;
        _icon.Visible = false;
        _icon.Dispose();
        _normal?.Dispose();
        _dimmed?.Dispose();
    }

    void OnUserPreferenceChanged(object? sender, UserPreferenceChangedEventArgs e)
    {
        if (e.Category is UserPreferenceCategory.General or UserPreferenceCategory.Color) LoadIcons();
    }

    // The taskbar follows "Windows mode" (SystemUsesLightTheme), not the app mode.
    void LoadIcons()
    {
        using var key = Registry.CurrentUser.OpenSubKey(@"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize");
        var lightTaskbar = key?.GetValue("SystemUsesLightTheme") is int value && value != 0;
        var uri = new Uri($"pack://application:,,,/Assets/Tray.{(lightTaskbar ? "OnLight" : "OnDark")}.ico");

        var oldNormal = _normal;
        var oldDimmed = _dimmed;
        using (var stream = System.Windows.Application.GetResourceStream(uri).Stream)
            _normal = new Icon(stream, SystemInformation.SmallIconSize);
        _dimmed = Dim(_normal, 0.45f);
        SetResponding(_responding);
        oldNormal?.Dispose();
        oldDimmed?.Dispose();
    }

    static Icon Dim(Icon source, float opacity)
    {
        using var original = source.ToBitmap();
        using var bmp = new Bitmap(original.Width, original.Height);
        using (var g = Graphics.FromImage(bmp))
        using (var attributes = new ImageAttributes())
        {
            attributes.SetColorMatrix(new ColorMatrix { Matrix33 = opacity });
            g.DrawImage(original, new Rectangle(0, 0, bmp.Width, bmp.Height), 0, 0, original.Width, original.Height,
                GraphicsUnit.Pixel, attributes);
        }
        var handle = bmp.GetHicon();
        try { return (Icon)Icon.FromHandle(handle).Clone(); }
        finally { DestroyIcon(handle); }
    }

    [System.Runtime.InteropServices.DllImport("user32.dll")]
    static extern bool DestroyIcon(IntPtr handle);
}
