using System.Runtime.Versioning;
using Avalonia;
using Avalonia.Media;
using Avalonia.Styling;
using Avalonia.Threading;
using ThinkVisionControl.Avalonia.Platform.MacOS.Native;

namespace ThinkVisionControl.Avalonia.Platform.MacOS;

/// <summary>
/// Light/dark from <c>NSApp.effectiveAppearance.name</c> (Aqua vs DarkAqua and their variants). No accent:
/// FluentAvalonia already reads the system accent on macOS. <see cref="Changed"/> follows Avalonia's
/// <c>Application.ActualThemeVariantChanged</c>, which tracks the system appearance while the app's requested
/// variant is Default, so nothing polls.
/// </summary>
[SupportedOSPlatform("macos")]
public sealed class MacThemeInfo : IThemeInfo
{
    private Application? _hooked;
    private Action? _changed;

    public bool IsDark
    {
        get
        {
            Hook();
            // AppKit objects are main-thread only; elsewhere fall back to Avalonia's view of the same setting.
            if (!Dispatcher.UIThread.CheckAccess()) return AvaloniaIsDark();
            try
            {
                using var pool = ObjC.Pool();
                // + (NSApplication *)sharedApplication;  @property (readonly, strong) NSAppearance *effectiveAppearance;
                // @property (readonly, copy) NSAppearanceName name;
                var app = ObjC.SendIntPtr(ObjC.GetClass("NSApplication"), ObjC.Selector("sharedApplication"));
                var appearance = app == IntPtr.Zero ? IntPtr.Zero : ObjC.SendIntPtr(app, ObjC.Selector("effectiveAppearance"));
                var name = appearance == IntPtr.Zero ? null : ObjC.ReadNSString(ObjC.SendIntPtr(appearance, ObjC.Selector("name")));
                return name is null ? AvaloniaIsDark() : MacRules.IsDarkAppearanceName(name);
            }
            catch (Exception ex) when (ex is DllNotFoundException or EntryPointNotFoundException)
            {
                return AvaloniaIsDark();
            }
        }
    }

    public Color? Accent => null;

    public event Action? Changed
    {
        add
        {
            _changed += value;
            Hook();
        }
        remove => _changed -= value;
    }

    private static bool AvaloniaIsDark() => Application.Current?.ActualThemeVariant == ThemeVariant.Dark;

    // Application.Current may not exist yet when the platform set is built, so hook on first use.
    private void Hook()
    {
        var app = Application.Current;
        if (app is null || ReferenceEquals(app, _hooked)) return;
        if (_hooked is not null) _hooked.ActualThemeVariantChanged -= OnThemeVariantChanged;
        _hooked = app;
        app.ActualThemeVariantChanged += OnThemeVariantChanged;
    }

    private void OnThemeVariantChanged(object? sender, EventArgs e) => _changed?.Invoke();
}
