using System.Reflection;
using System.Runtime.InteropServices;
using ThinkVisionControl.Core.Logging;
using Point = System.Windows.Point;
using Size = System.Windows.Size;

namespace ThinkVisionControl.Platform;

/// <summary>
/// Positions a flyout next to the tray icon the way shell flyouts are: <c>Shell_NotifyIconGetRect</c> for the icon,
/// <c>CalculatePopupWindowPosition</c> to keep it inside the work area on the icon's monitor. Results are WPF DIPs.
/// </summary>
public static class TrayAnchor
{
    // Windows 11 shell flyouts float this far (DIPs) from the taskbar and the screen edge.
    const double Gap = 12;

    const uint TpmLeftAlign = 0x0, TpmTopAlign = 0x0, TpmCenterAlign = 0x4, TpmRightAlign = 0x8;
    const uint TpmVCenterAlign = 0x10, TpmBottomAlign = 0x20, TpmVertical = 0x40, TpmWorkArea = 0x10000;
    const uint MonitorDefaultToNearest = 2;

    /// <summary>
    /// Top-left (DIPs) for a popup of <paramref name="popupSize"/> (DIPs) anchored to the tray icon, or null on failure.
    /// <paramref name="trayWindow"/>/<paramref name="iconId"/> are the hWnd/uID the icon was added with; for a WinForms
    /// <see cref="System.Windows.Forms.NotifyIcon"/> get them from <see cref="Identify"/>.
    /// </summary>
    public static Point? PopupOrigin(IntPtr trayWindow, uint iconId, Size popupSize)
    {
        try
        {
            var id = new NotifyIconIdentifier { Size = (uint)Marshal.SizeOf<NotifyIconIdentifier>(), Window = trayWindow, Id = iconId };
            var hr = Shell_NotifyIconGetRect(ref id, out var icon);
            if (hr != 0)
            {
                FileLog.Write($"Shell_NotifyIconGetRect failed: 0x{hr:X8}");
                return null;
            }
            return Place(icon, popupSize);
        }
        catch (Exception ex)
        {
            FileLog.Write($"TrayAnchor: {ex.GetType().Name}: {ex.Message}");
            return null;
        }
    }

    /// <summary>Fallback when the icon rect is unavailable: anchors the popup to the cursor (normally just clicked the icon).</summary>
    public static Point? PopupOriginFromCursor(Size popupSize)
    {
        try
        {
            if (!GetCursorPos(out var cursor)) return null;
            return Place(new Rect { Left = cursor.X, Top = cursor.Y, Right = cursor.X + 1, Bottom = cursor.Y + 1 }, popupSize);
        }
        catch (Exception ex)
        {
            FileLog.Write($"TrayAnchor: {ex.GetType().Name}: {ex.Message}");
            return null;
        }
    }

    /// <summary>
    /// The hWnd and uID WinForms registered <paramref name="icon"/> with. NotifyIcon keeps them private
    /// (.NET 10: <c>_window</c>, a NativeWindow, and <c>_id</c>, a uint; .NET Framework used <c>window</c>/<c>id</c>),
    /// so this reads them by reflection. Returns null if the fields are missing or the window has no handle.
    /// </summary>
    public static (IntPtr hwnd, uint id)? Identify(System.Windows.Forms.NotifyIcon icon)
    {
        try
        {
            const BindingFlags Flags = BindingFlags.Instance | BindingFlags.NonPublic;
            var type = typeof(System.Windows.Forms.NotifyIcon);
            var window = (type.GetField("_window", Flags) ?? type.GetField("window", Flags))?.GetValue(icon) as System.Windows.Forms.NativeWindow;
            var id = (type.GetField("_id", Flags) ?? type.GetField("id", Flags))?.GetValue(icon);
            if (window is null || window.Handle == IntPtr.Zero || id is null) return null;
            return (window.Handle, Convert.ToUInt32(id));
        }
        catch (Exception ex)
        {
            FileLog.Write($"TrayAnchor.Identify: {ex.GetType().Name}: {ex.Message}");
            return null;
        }
    }

    static Point? Place(Rect anchor, Size popupSize)
    {
        var monitor = MonitorFromRect(ref anchor, MonitorDefaultToNearest);
        var info = new MonitorInfo { Size = Marshal.SizeOf<MonitorInfo>() };
        if (monitor == IntPtr.Zero || !GetMonitorInfo(monitor, ref info)) return null;

        var scale = Dpi(monitor) / 96.0;
        var size = new NativeSize { Cx = (int)Math.Ceiling(popupSize.Width * scale), Cy = (int)Math.Ceiling(popupSize.Height * scale) };
        var gap = (int)Math.Round(Gap * scale);
        var work = info.Work;
        var point = new NativePoint { X = (anchor.Left + anchor.Right) / 2, Y = (anchor.Top + anchor.Bottom) / 2 };

        // The taskbar is on whichever side the anchor lies outside the work area; open away from it. The excluded
        // rect runs from the icon to the work-area edge so the popup clears the taskbar, not just the icon button.
        var exclude = anchor;
        uint flags = TpmWorkArea;
        if (point.X < work.Left) { flags |= TpmLeftAlign | TpmVCenterAlign; exclude.Right = Math.Max(exclude.Right, work.Left); }
        else if (point.X >= work.Right) { flags |= TpmRightAlign | TpmVCenterAlign; exclude.Left = Math.Min(exclude.Left, work.Right); }
        else if (point.Y < work.Top) { flags |= TpmCenterAlign | TpmTopAlign | TpmVertical; exclude.Bottom = Math.Max(exclude.Bottom, work.Top); }
        else { flags |= TpmCenterAlign | TpmBottomAlign | TpmVertical; exclude.Top = Math.Min(exclude.Top, work.Bottom); }
        exclude = new Rect { Left = exclude.Left - gap, Top = exclude.Top - gap, Right = exclude.Right + gap, Bottom = exclude.Bottom + gap };

        if (!CalculatePopupWindowPosition(ref point, ref size, flags, ref exclude, out var popup)) return null;

        // Keep the shell's margin from the work-area edges.
        var x = Clamp(popup.Left, work.Left + gap, work.Right - gap - size.Cx);
        var y = Clamp(popup.Top, work.Top + gap, work.Bottom - gap - size.Cy);

        return new Point(x / scale, y / scale);
    }

    static int Clamp(int value, int min, int max) => max < min ? value : Math.Clamp(value, min, max);

    // Win32 coordinates are in this thread's DPI-awareness space: physical pixels only when per-monitor aware.
    static double Dpi(IntPtr monitor)
    {
        try
        {
            switch (GetAwarenessFromDpiAwarenessContext(GetThreadDpiAwarenessContext()))
            {
                case 0: return 96;
                case 1: return GetDpiForSystem();
                default:
                    return GetDpiForMonitor(monitor, 0, out var dpiX, out _) == 0 && dpiX > 0 ? dpiX : GetDpiForSystem();
            }
        }
        catch (EntryPointNotFoundException)
        {
            return 96;
        }
    }

    [StructLayout(LayoutKind.Sequential)]
    struct NotifyIconIdentifier
    {
        public uint Size;
        public IntPtr Window;
        public uint Id;
        public Guid Item;
    }

    [StructLayout(LayoutKind.Sequential)]
    struct Rect { public int Left, Top, Right, Bottom; }

    [StructLayout(LayoutKind.Sequential)]
    struct NativePoint { public int X, Y; }

    [StructLayout(LayoutKind.Sequential)]
    struct NativeSize { public int Cx, Cy; }

    [StructLayout(LayoutKind.Sequential)]
    struct MonitorInfo
    {
        public int Size;
        public Rect Monitor;
        public Rect Work;
        public uint Flags;
    }

    [DllImport("shell32.dll")]
    static extern int Shell_NotifyIconGetRect(ref NotifyIconIdentifier identifier, out Rect iconLocation);

    [DllImport("user32.dll")]
    static extern bool CalculatePopupWindowPosition(ref NativePoint anchorPoint, ref NativeSize windowSize, uint flags, ref Rect excludeRect, out Rect popupWindowPosition);

    [DllImport("user32.dll")]
    static extern bool GetCursorPos(out NativePoint point);

    [DllImport("user32.dll")]
    static extern IntPtr MonitorFromRect(ref Rect rect, uint flags);

    [DllImport("user32.dll", CharSet = CharSet.Unicode)]
    static extern bool GetMonitorInfo(IntPtr monitor, ref MonitorInfo info);

    [DllImport("user32.dll")]
    static extern IntPtr GetThreadDpiAwarenessContext();

    [DllImport("user32.dll")]
    static extern int GetAwarenessFromDpiAwarenessContext(IntPtr context);

    [DllImport("user32.dll")]
    static extern uint GetDpiForSystem();

    [DllImport("shcore.dll")]
    static extern int GetDpiForMonitor(IntPtr monitor, int dpiType, out uint dpiX, out uint dpiY);
}
