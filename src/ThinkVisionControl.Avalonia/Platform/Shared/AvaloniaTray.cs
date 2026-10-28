using Avalonia;
using Avalonia.Controls;
using Avalonia.Media;
using Avalonia.Media.Imaging;
using Avalonia.Platform;
using Avalonia.Threading;
using ThinkVisionControl.Core.Control;
using ThinkVisionControl.Core.Logging;

namespace ThinkVisionControl.Avalonia.Platform.Shared;

/// <summary>
/// ITray on Avalonia's TrayIcon (Windows notification area, macOS menu bar, Linux StatusNotifierItem). OS-neutral:
/// the per-OS part comes in through <paramref name="hostAvailable"/>.
/// <list type="bullet">
/// <item>The icon is the app icon; NotFound and NotResponding get a badge at the bottom right (rendered here).</item>
/// <item>Tooltip "ThinkVision Control – &lt;status&gt;".</item>
/// <item><c>Clicked</c> runs the primary action; the native menu is rebuilt from the menu delegate on every
/// <see cref="SetStatus"/> and whenever the menu asks (<c>NativeMenu.NeedsUpdate</c>). NativeMenu has no bold
/// default item, so the default item goes first.</item>
/// </list>
/// Methods may be called from any thread; the work is marshalled to the UI thread.
/// </summary>
/// <param name="hostAvailable">False when nothing can show the icon.</param>
public sealed class AvaloniaTray(Func<bool> hostAvailable) : ITray
{
    const string AppName = "ThinkVision Control";
    const int BadgedIconSize = 32;
    static readonly Uri IconUri = new("avares://thinkvision-control/Assets/ThinkVisionControl.ico");

    readonly Dictionary<MonitorStatus, WindowIcon?> _badged = [];
    TrayIcon? _icon;
    NativeMenu? _nativeMenu;
    WindowIcon? _baseIcon;
    Bitmap? _baseBitmap;
    Func<IReadOnlyList<TrayMenuItem>>? _menu;
    Action? _primaryClick;
    MonitorStatus _status = MonitorStatus.Connecting;
    bool _disposed;

    public bool IsHostAvailable => hostAvailable();

    public void Show(Func<IReadOnlyList<TrayMenuItem>> menu, Action primaryClick) => OnUiThread(() =>
    {
        if (_disposed) return;
        (_menu, _primaryClick) = (menu, primaryClick);
        if (_icon is null)
        {
            _nativeMenu = new NativeMenu();
            _nativeMenu.NeedsUpdate += (_, _) => RebuildMenu();
            _icon = new TrayIcon { Menu = _nativeMenu };
            _icon.Clicked += (_, _) => _primaryClick?.Invoke();

            // Registering with the application ties the icon's lifetime to it (removed on exit).
            var app = Application.Current ?? throw new InvalidOperationException("The Avalonia application isn't running.");
            var icons = TrayIcon.GetIcons(app);
            if (icons is null) TrayIcon.SetIcons(app, [_icon]);
            else icons.Add(_icon);
        }
        RebuildMenu();
        ApplyStatus();
        _icon.IsVisible = true;
    });

    public void SetStatus(MonitorStatus status) => OnUiThread(() =>
    {
        _status = status;
        if (_icon is null || _disposed) return;
        ApplyStatus();
        RebuildMenu();
    });

    public void Dispose() => OnUiThread(() =>
    {
        if (_disposed) return;
        _disposed = true;
        if (_icon is not null)
        {
            _icon.IsVisible = false;
            if (Application.Current is { } app) TrayIcon.GetIcons(app)?.Remove(_icon);
            _icon.Dispose();
            _icon = null;
        }
        _baseBitmap?.Dispose();
        _baseBitmap = null;
    });

    static void OnUiThread(Action action)
    {
        if (Dispatcher.UIThread.CheckAccess()) action();
        else Dispatcher.UIThread.Post(action);
    }

    void RebuildMenu()
    {
        if (_nativeMenu is null || _menu is null) return;
        IReadOnlyList<TrayMenuItem> items;
        try
        {
            items = _menu();
        }
        catch (Exception ex)
        {
            FileLog.Write($"Tray menu: {ex.GetType().Name}: {ex.Message}");
            return;
        }

        _nativeMenu.Items.Clear();
        foreach (var item in items.Where(i => i.IsDefault).Concat(items.Where(i => !i.IsDefault)))
        {
            if (item.IsSeparator)
            {
                // No leading, trailing or doubled separators.
                if (_nativeMenu.Items.Count > 0 && _nativeMenu.Items[^1] is not NativeMenuItemSeparator)
                    _nativeMenu.Items.Add(new NativeMenuItemSeparator());
                continue;
            }
            var native = new NativeMenuItem(item.Label) { IsEnabled = item.Run is not null };
            if (item.Run is { } run) native.Click += (_, _) => run();
            _nativeMenu.Items.Add(native);
        }
        if (_nativeMenu.Items.Count > 0 && _nativeMenu.Items[^1] is NativeMenuItemSeparator)
            _nativeMenu.Items.RemoveAt(_nativeMenu.Items.Count - 1);
    }

    void ApplyStatus()
    {
        if (_icon is null) return;
        _icon.ToolTipText = AppName + " – " + _status switch
        {
            MonitorStatus.Connected => "P32p-20",
            MonitorStatus.Connecting => "Connecting",
            MonitorStatus.NotFound => "Not connected",
            _ => "Not responding",
        };
        _icon.Icon = (_status is MonitorStatus.NotFound or MonitorStatus.NotResponding ? Badged(_status) : null) ?? BaseIcon();
    }

    WindowIcon? BaseIcon()
    {
        if (_baseIcon is not null) return _baseIcon;
        try
        {
            // The .ico keeps every size, so each OS picks the one that fits its tray.
            using var stream = AssetLoader.Open(IconUri);
            return _baseIcon = new WindowIcon(stream);
        }
        catch (Exception ex)
        {
            FileLog.Write($"Tray icon: {ex.GetType().Name}: {ex.Message}");
            return null;
        }
    }

    WindowIcon? Badged(MonitorStatus status)
    {
        if (_badged.TryGetValue(status, out var cached)) return cached;
        WindowIcon? icon = null;
        try
        {
            if (LoadBaseBitmap() is { } bitmap)
            {
                using var rendered = RenderBadged(bitmap, status, BadgedIconSize);
                // Round-trip through PNG so the WindowIcon owns its pixels independently of the render target.
                using var png = new MemoryStream();
                rendered.Save(png, PngBitmapEncoderOptions.Default);
                png.Position = 0;
                icon = new WindowIcon(png);
            }
        }
        catch (Exception ex)
        {
            FileLog.Write($"Tray badge: {ex.GetType().Name}: {ex.Message}");
        }
        _badged[status] = icon; // null (failure) is cached too: fall back to the plain icon without retrying
        return icon;
    }

    /// <summary>The largest image in the .ico, decoded portably: the PNG-compressed entry if there is one.</summary>
    Bitmap? LoadBaseBitmap()
    {
        if (_baseBitmap is not null) return _baseBitmap;
        using var stream = AssetLoader.Open(IconUri);
        using var buffer = new MemoryStream();
        stream.CopyTo(buffer);
        var bytes = buffer.ToArray();
        return _baseBitmap = PngEntry(bytes) is { } png ? new Bitmap(new MemoryStream(png)) : new Bitmap(new MemoryStream(bytes));
    }

    /// <summary>The largest PNG-compressed image in an ICO file (ICONDIR + 16-byte ICONDIRENTRYs), or null.</summary>
    static byte[]? PngEntry(byte[] ico)
    {
        if (ico.Length < 6 || BitConverter.ToUInt16(ico, 2) != 1) return null;
        int count = BitConverter.ToUInt16(ico, 4), best = -1, bestWidth = 0;
        for (var i = 0; i < count && 6 + 16 * (i + 1) <= ico.Length; i++)
        {
            var entry = 6 + 16 * i;
            var width = ico[entry] == 0 ? 256 : ico[entry];
            var offset = (int)BitConverter.ToUInt32(ico, entry + 12);
            var size = (int)BitConverter.ToUInt32(ico, entry + 8);
            if (offset < 0 || size < 8 || offset + size > ico.Length || ico[offset] != 0x89 || ico[offset + 1] != (byte)'P') continue;
            if (width > bestWidth) (best, bestWidth) = (i, width);
        }
        if (best < 0) return null;
        var at = 6 + 16 * best;
        return ico.AsSpan((int)BitConverter.ToUInt32(ico, at + 12), (int)BitConverter.ToUInt32(ico, at + 8)).ToArray();
    }

    /// <summary>
    /// The icon with a status badge at the bottom right, cut out from the icon by a transparent ring: a × in a grey
    /// circle for "not connected", a ! in an amber circle for "not responding". The colours read on light and dark
    /// trays alike, since the shared layer doesn't know the tray's theme.
    /// </summary>
    static RenderTargetBitmap RenderBadged(Bitmap baseIcon, MonitorStatus status, int size)
    {
        var target = new RenderTargetBitmap(new PixelSize(size, size), new Vector(96, 96));
        var notResponding = status == MonitorStatus.NotResponding;
        var fill = new SolidColorBrush(notResponding ? Color.FromRgb(0xFF, 0xB9, 0x00) : Color.FromRgb(0x76, 0x76, 0x76));
        var mark = notResponding ? Colors.Black : Colors.White;

        var d = Math.Max(7, Math.Round(size * 0.5));
        var badge = new Rect(size - d, size - d, d, d);
        var gap = Math.Max(1, size / 16.0);
        var full = new Rect(0, 0, size, size);

        using (var context = target.CreateDrawingContext())
        {
            using (context.PushRenderOptions(new RenderOptions { BitmapInterpolationMode = BitmapInterpolationMode.HighQuality }))
            using (context.PushGeometryClip(new CombinedGeometry(GeometryCombineMode.Exclude, new RectangleGeometry(full),
                       new EllipseGeometry(badge.Inflate(gap)))))
            {
                context.DrawImage(baseIcon, new Rect(baseIcon.Size), full);
            }

            context.DrawEllipse(fill, null, badge);
            var pen = new Pen(new SolidColorBrush(mark), Math.Max(1.5, d / 7), lineCap: PenLineCap.Round);
            double cx = badge.X + d / 2, cy = badge.Y + d / 2;
            if (notResponding)
            {
                context.DrawLine(pen, new Point(cx, badge.Y + d * 0.24), new Point(cx, badge.Y + d * 0.56));
                context.DrawLine(pen, new Point(cx, badge.Y + d * 0.76), new Point(cx, badge.Y + d * 0.77));
            }
            else
            {
                var r = d * 0.2;
                context.DrawLine(pen, new Point(cx - r, cy - r), new Point(cx + r, cy + r));
                context.DrawLine(pen, new Point(cx - r, cy + r), new Point(cx + r, cy - r));
            }
        }
        return target;
    }
}
