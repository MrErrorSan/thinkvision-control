using System.Runtime.InteropServices;
using System.Runtime.Versioning;
using Avalonia;
using ThinkVisionControl.Core.Logging;

namespace ThinkVisionControl.Avalonia.Platform.Windows;

/// <summary>
/// Positions the flyout next to the tray icon the way shell flyouts are: <c>CalculatePopupWindowPosition</c> keeps it
/// inside the work area of the icon's monitor, opening away from the taskbar. Avalonia's <c>TrayIcon</c> doesn't
/// expose the HWND/uID it registered, so <c>Shell_NotifyIconGetRect</c> isn't usable; the anchor is the cursor,
/// which has just clicked the icon.
/// <para>
/// Input: the flyout size in DIPs. Output: the top-left in <b>physical screen pixels</b> (Avalonia makes the process
/// per-monitor DPI aware, so Win32 coordinates are physical); the shell converts it with
/// <c>new PixelPoint((int)p.X, (int)p.Y)</c>. Null on failure.
/// </para>
/// </summary>
[SupportedOSPlatform("windows")]
public static class WindowsTrayAnchor
{
    // Windows 11 shell flyouts float this far (DIPs) from the taskbar and the screen edge.
    const double Gap = 12;

    const uint TpmLeftAlign = 0x0, TpmTopAlign = 0x0, TpmCenterAlign = 0x4, TpmRightAlign = 0x8;
    const uint TpmVCenterAlign = 0x10, TpmBottomAlign = 0x20, TpmVertical = 0x40, TpmWorkArea = 0x10000;
    const uint MonitorDefaultToNearest = 2;

    public static Point? FromCursor(Size flyout)
    {
        try
        {
            if (!GetCursorPos(out var cursor)) return null;
            return Place(new Rect32 { Left = cursor.X, Top = cursor.Y, Right = cursor.X + 1, Bottom = cursor.Y + 1 }, flyout);
        }
        catch (Exception ex)
        {
            FileLog.Write($"WindowsTrayAnchor: {ex.GetType().Name}: {ex.Message}");
            return null;
        }
    }

    static Point? Place(Rect32 anchor, Size flyout)
    {
        var monitor = MonitorFromRect(ref anchor, MonitorDefaultToNearest);
        var info = new MonitorInfo { Size = Marshal.SizeOf<MonitorInfo>() };
        if (monitor == IntPtr.Zero || !GetMonitorInfoW(monitor, ref info)) return null;

        var scale = Dpi(monitor) / 96.0;
        var size = new NativeSize { Cx = (int)Math.Ceiling(flyout.Width * scale), Cy = (int)Math.Ceiling(flyout.Height * scale) };
        var gap = (int)Math.Round(Gap * scale);
        var work = info.Work;
        var point = new NativePoint { X = (anchor.Left + anchor.Right) / 2, Y = (anchor.Top + anchor.Bottom) / 2 };

        // The taskbar is on whichever side the anchor lies outside the work area; open away from it. The excluded
        // rect runs from the anchor to the work-area edge so the popup clears the taskbar, not just the icon button.
        var exclude = anchor;
        uint flags = TpmWorkArea;
        if (point.X < work.Left) { flags |= TpmLeftAlign | TpmVCenterAlign; exclude.Right = Math.Max(exclude.Right, work.Left); }
        else if (point.X >= work.Right) { flags |= TpmRightAlign | TpmVCenterAlign; exclude.Left = Math.Min(exclude.Left, work.Right); }
        else if (point.Y < work.Top) { flags |= TpmCenterAlign | TpmTopAlign | TpmVertical; exclude.Bottom = Math.Max(exclude.Bottom, work.Top); }
        else { flags |= TpmCenterAlign | TpmBottomAlign | TpmVertical; exclude.Top = Math.Min(exclude.Top, work.Bottom); }
        exclude = new Rect32 { Left = exclude.Left - gap, Top = exclude.Top - gap, Right = exclude.Right + gap, Bottom = exclude.Bottom + gap };

        if (!CalculatePopupWindowPosition(ref point, ref size, flags, ref exclude, out var popup)) return null;

        // Keep the shell's margin from the work-area edges.
        var x = Clamp(popup.Left, work.Left + gap, work.Right - gap - size.Cx);
        var y = Clamp(popup.Top, work.Top + gap, work.Bottom - gap - size.Cy);
        return new Point(x, y);
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
    struct Rect32 { public int Left, Top, Right, Bottom; }

    [StructLayout(LayoutKind.Sequential)]
    struct NativePoint { public int X, Y; }

    [StructLayout(LayoutKind.Sequential)]
    struct NativeSize { public int Cx, Cy; }

    [StructLayout(LayoutKind.Sequential)]
    struct MonitorInfo
    {
        public int Size;
        public Rect32 Monitor;
        public Rect32 Work;
        public uint Flags;
    }

    [DllImport("user32.dll")]
    static extern bool CalculatePopupWindowPosition(ref NativePoint anchorPoint, ref NativeSize windowSize, uint flags, ref Rect32 excludeRect, out Rect32 popupWindowPosition);

    [DllImport("user32.dll")]
    static extern bool GetCursorPos(out NativePoint point);

    [DllImport("user32.dll")]
    static extern IntPtr MonitorFromRect(ref Rect32 rect, uint flags);

    [DllImport("user32.dll", CharSet = CharSet.Unicode)]
    static extern bool GetMonitorInfoW(IntPtr monitor, ref MonitorInfo info);

    [DllImport("user32.dll")]
    static extern IntPtr GetThreadDpiAwarenessContext();

    [DllImport("user32.dll")]
    static extern int GetAwarenessFromDpiAwarenessContext(IntPtr context);

    [DllImport("user32.dll")]
    static extern uint GetDpiForSystem();

    [DllImport("shcore.dll")]
    static extern int GetDpiForMonitor(IntPtr monitor, int dpiType, out uint dpiX, out uint dpiY);
}
