using System.Runtime.InteropServices;
using System.Runtime.Versioning;
using Avalonia.Media;
using Microsoft.Win32;

namespace ThinkVisionControl.Avalonia.Platform.Windows;

/// <summary>
/// App mode from <c>AppsUseLightTheme</c> and the accent from <c>DWM\AccentColor</c> (ABGR). <see cref="Changed"/> is
/// raised on the UI thread when Windows broadcasts <c>WM_SETTINGCHANGE "ImmersiveColorSet"</c> or a colorization
/// change, which a hidden top-level window receives (a message-only window misses broadcasts).
/// Reading the accent here, rather than leaving it null for FluentAvalonia, makes a live accent change apply too.
/// </summary>
[SupportedOSPlatform("windows")]
public sealed class WindowsThemeInfo : IThemeInfo, IDisposable
{
    const int WmSettingChange = 0x001A;
    const int WmDwmColorizationColorChanged = 0x0320;
    const string PersonalizeKey = @"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize";
    const string DwmKey = @"Software\Microsoft\Windows\DWM";

    readonly Win32MessageWindow _window;

    public WindowsThemeInfo()
    {
        Read();
        _window = new Win32MessageWindow(WndProc, messageOnly: false);
    }

    public bool IsDark { get; private set; }

    public Color? Accent { get; private set; }

    public event Action? Changed;

    public void Dispose() => _window.Dispose();

    void Read()
    {
        using (var key = Registry.CurrentUser.OpenSubKey(PersonalizeKey))
            IsDark = key?.GetValue("AppsUseLightTheme") is int light && light == 0;
        using (var key = Registry.CurrentUser.OpenSubKey(DwmKey))
        {
            // REG_DWORD 0xAABBGGRR; GetValue returns it as a (signed) int.
            Accent = key?.GetValue("AccentColor") is int abgr
                ? Color.FromRgb((byte)abgr, (byte)(abgr >> 8), (byte)(abgr >> 16))
                : null;
        }
    }

    IntPtr? WndProc(int msg, IntPtr wParam, IntPtr lParam)
    {
        var relevant = msg == WmDwmColorizationColorChanged
            || (msg == WmSettingChange && lParam != IntPtr.Zero && Marshal.PtrToStringUni(lParam) == "ImmersiveColorSet");
        if (!relevant) return null;

        var (dark, accent) = (IsDark, Accent);
        Read();
        if (dark != IsDark || accent != Accent) Changed?.Invoke();
        return null;
    }
}
