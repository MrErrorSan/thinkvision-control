using Microsoft.Win32;
using System.Windows;
using System.Windows.Threading;

namespace ThinkVisionControl.Ui;

/// <summary>
/// Reports whether the taskbar and apps use the light theme, and raises <see cref="Changed"/> on the UI thread when
/// either changes. The Fluent theme (<c>ThemeMode="System"</c>) owns the palette, Mica and the title-bar colour.
/// </summary>
public sealed class WindowsThemeManager : IDisposable
{
    const string PersonalizeKey = @"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize";

    // Legacy: remove once pages/tray-flyout land. Brush keys older windows still use, aliased to Fluent brushes.
    static readonly (string Legacy, string Fluent)[] LegacyBrushes =
    [
        ("TextBrush", "TextFillColorPrimaryBrush"),
        ("MutedTextBrush", "TextFillColorSecondaryBrush"),
        ("AccentBrush", "AccentFillColorDefaultBrush"),
        ("SurfaceBrush", "SolidBackgroundFillColorQuarternaryBrush"),
        ("LineBrush", "CardStrokeColorDefaultBrush"),
        ("GoodBrush", "SystemFillColorSuccessBrush"),
        ("OffBrush", "SystemFillColorSolidNeutralBrush"),
    ];

    readonly Application _application;
    readonly DispatcherTimer _settle;
    bool _disposed;

    public WindowsThemeManager(Application application)
    {
        _application = application;
        // WPF swaps its Fluent dictionaries when its own windows see the setting change, which can land after
        // SystemEvents tells us; refresh once more after a short settle so the legacy aliases pick up the new set.
        _settle = new DispatcherTimer(TimeSpan.FromMilliseconds(750), DispatcherPriority.Background, (_, _) => Refresh(), application.Dispatcher);
        (IsTaskbarLight, IsAppLight) = Read();
        AliasLegacyBrushes();
        SystemEvents.UserPreferenceChanged += UserPreferenceChanged;
    }

    /// <summary>True when the taskbar (and notification area) uses the light theme.</summary>
    public bool IsTaskbarLight { get; private set; }

    /// <summary>True when apps use the light theme.</summary>
    public bool IsAppLight { get; private set; }

    /// <summary>Raised on the UI thread after <see cref="IsTaskbarLight"/> or <see cref="IsAppLight"/> changes.</summary>
    public event Action? Changed;

    public void Dispose()
    {
        if (_disposed) return;
        _disposed = true;
        _settle.Stop();
        SystemEvents.UserPreferenceChanged -= UserPreferenceChanged;
    }

    void UserPreferenceChanged(object? sender, UserPreferenceChangedEventArgs e)
    {
        if (_disposed || e.Category is not (UserPreferenceCategory.General or UserPreferenceCategory.Color
                or UserPreferenceCategory.VisualStyle)) return;
        _application.Dispatcher.BeginInvoke(DispatcherPriority.Background, () =>
        {
            Refresh();
            _settle.Stop();
            _settle.Start();
        });
    }

    void Refresh()
    {
        _settle.Stop();
        if (_disposed) return;
        AliasLegacyBrushes();
        var (taskbar, app) = Read();
        if (taskbar == IsTaskbarLight && app == IsAppLight) return;
        (IsTaskbarLight, IsAppLight) = (taskbar, app);
        Changed?.Invoke();
    }

    void AliasLegacyBrushes()
    {
        foreach (var (legacy, fluent) in LegacyBrushes)
            if (_application.TryFindResource(fluent) is { } brush) _application.Resources[legacy] = brush;
    }

    static (bool Taskbar, bool App) Read()
    {
        using var key = Registry.CurrentUser.OpenSubKey(PersonalizeKey);
        // Missing values mean the Windows defaults: dark taskbar, light apps.
        return (key?.GetValue("SystemUsesLightTheme", 0) is int taskbar && taskbar != 0,
                key?.GetValue("AppsUseLightTheme", 1) is int app && app != 0);
    }
}
