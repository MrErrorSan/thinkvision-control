using Avalonia;
using Avalonia.Animation;
using Avalonia.Controls;
using Avalonia.Input;
using Avalonia.Interactivity;
using Avalonia.Media;
using Avalonia.Styling;
using Avalonia.Threading;
using FluentAvalonia.Styling;
using ThinkVisionControl.Avalonia.ViewModels;

namespace ThinkVisionControl.Avalonia.Views;

/// <summary>
/// Quick Settings-style tray flyout: brightness and contrast, input tiles, colour mode, Day/Night, a status line and
/// "All settings". Light-dismiss on deactivate or Esc; closing only hides it. Create one lazily and reuse it.
/// Acrylic on Windows 11; a solid layer background everywhere else.
/// </summary>
public partial class FlyoutWindow : Window
{
    const double FlyoutWidth = 360, ScreenMargin = 12;
    const long ReopenGuardMs = 400;
    static readonly TimeSpan Entrance = TimeSpan.FromMilliseconds(167);

    readonly MainViewModel? _vm;
    readonly bool _acrylic;
    TaskCompletionSource<bool>? _pending;
    long _hiddenAt;
    bool _shown;

    /// <summary>Designer only.</summary>
    public FlyoutWindow()
    {
        InitializeComponent();
        _acrylic = OperatingSystem.IsWindowsVersionAtLeast(10, 0, 22000);
        if (_acrylic)
        {
            TransparencyLevelHint = [WindowTransparencyLevel.AcrylicBlur, WindowTransparencyLevel.None];
            Background = Brushes.Transparent;
            AcrylicLayer.IsVisible = true;
            ApplyTint();
            ActualThemeVariantChanged += (_, _) => ApplyTint();
        }
        else
        {
            TransparencyLevelHint = [WindowTransparencyLevel.None];
            this.Bind(BackgroundProperty, this.GetResourceObservable("SolidBackgroundFillColorBaseBrush"));
        }

        Root.Transitions = [new DoubleTransition { Property = OpacityProperty, Duration = Entrance }];
        Deactivated += (_, _) =>
        {
            if (!IsVisible) return;
            _hiddenAt = Environment.TickCount64;
            Dismiss();
        };
        Opened += (_, _) =>
        {
            MatchTitleBarToTheme();
            // The compositor refused acrylic (e.g. transparency effects off): use the solid fallback.
            if (_acrylic && ActualTransparencyLevel == WindowTransparencyLevel.None)
            {
                AcrylicLayer.IsVisible = false;
                this.Bind(BackgroundProperty, this.GetResourceObservable("SolidBackgroundFillColorBaseBrush"));
            }
        };
        SizeChanged += OnSizeChanged;
    }

    public FlyoutWindow(MainViewModel vm) : this() => DataContext = _vm = vm;

    /// <summary>
    /// Places the flyout's top-left at <paramref name="origin"/> (screen pixels), shows and activates it; null puts it
    /// in the bottom-right corner of the primary screen's work area.
    /// </summary>
    public void ShowAt(PixelPoint? origin)
    {
        if (IsVisible)
        {
            Activate();
            return;
        }
        // A click on the tray icon while the flyout is open deactivates (hides) it first; don't reopen on that click.
        if (Environment.TickCount64 - _hiddenAt < ReopenGuardMs) return;

        Position = origin ?? DefaultOrigin();
        Root.Opacity = 0;
        Show();
        Activate();
        _shown = true;
        // After the first frame, so the fade-in transition runs.
        Dispatcher.UIThread.Post(() => Root.Opacity = 1, DispatcherPriority.Render);
    }

    /// <summary>The flyout's size in DIPs (fixed width, height measured from the current content), for <c>ITray.FlyoutOrigin</c>.</summary>
    public Size PopupSize
    {
        get
        {
            Root.Measure(new Size(FlyoutWidth, double.PositiveInfinity));
            return new Size(FlyoutWidth, Math.Ceiling(Root.DesiredSize.Height));
        }
    }

    PixelPoint DefaultOrigin()
    {
        var screen = Screens.Primary ?? Screens.All.FirstOrDefault();
        if (screen is null) return new PixelPoint(0, 0);
        var size = PopupSize;
        var scale = screen.Scaling;
        var area = screen.WorkingArea;
        var width = (int)Math.Ceiling(size.Width * scale);
        var height = (int)Math.Ceiling(size.Height * scale);
        var margin = (int)(ScreenMargin * scale);
        return new PixelPoint(area.Right - width - margin, area.Bottom - height - margin);
    }

    // Keep the bottom edge (next to the taskbar) fixed when the content grows or shrinks while open.
    void OnSizeChanged(object? sender, SizeChangedEventArgs e)
    {
        if (!_shown || !e.HeightChanged || e.PreviousSize.Height <= 0) return;
        var delta = (int)Math.Round((e.NewSize.Height - e.PreviousSize.Height) * DesktopScaling);
        Position = new PixelPoint(Position.X, Position.Y - delta);
    }

    protected override void OnKeyDown(KeyEventArgs e)
    {
        base.OnKeyDown(e);
        if (e.Key != Key.Escape || e.Handled) return;
        e.Handled = true;
        Dismiss();
    }

    // Reused: closing by the user (Alt+F4) only hides it. Programmatic closes and app shutdown really close.
    protected override void OnClosing(WindowClosingEventArgs e)
    {
        base.OnClosing(e);
        if (e.CloseReason != WindowCloseReason.WindowClosing || e.IsProgrammatic) return;
        e.Cancel = true;
        Dismiss();
    }

    void Dismiss()
    {
        _shown = false;
        Answer(false); // a question left open must not outlive the flyout
        Hide();
    }

    void ApplyTint()
    {
        if (AcrylicLayer.Material is not { } material) return;
        if (ActualThemeVariant == ThemeVariant.Light)
        {
            // Close to Windows 11 light acrylic (near white), not mid-grey.
            material.TintColor = Color.Parse("#FCFCFC");
            material.FallbackColor = Color.Parse("#F9F9F9");
            material.TintOpacity = 0.9;
            material.MaterialOpacity = 0.85;
        }
        else
        {
            material.TintColor = Color.Parse("#2C2C2C");
            material.FallbackColor = Color.Parse("#2C2C2C");
            material.TintOpacity = 0.8;
            material.MaterialOpacity = 0.7;
        }
    }

    void MatchTitleBarToTheme()
    {
        if (OperatingSystem.IsWindows())
            global::Avalonia.Application.Current?.Styles.OfType<FluentAvaloniaTheme>().FirstOrDefault()
                ?.ForceWin32WindowToTheme(this, ActualThemeVariant);
    }

    // ---------------------------------------------------------------- input, inline confirmation

    void InputTile_Click(object? sender, RoutedEventArgs e)
    {
        if (_vm is { } vm && sender is Control { DataContext: EnumOption option })
            _ = vm.SelectInputAsync(option, ConfirmInlineAsync);
    }

    Task<bool> ConfirmInlineAsync(DialogSpec spec)
    {
        Answer(false);
        ConfirmTitle.Text = spec.Title;
        ConfirmMessage.Text = spec.Content as string;
        ConfirmButton.Content = spec.PrimaryText;
        ConfirmPanel.IsVisible = true;
        CancelButton.Focus();
        _pending = new TaskCompletionSource<bool>();
        return _pending.Task;
    }

    void Answer(bool confirmed)
    {
        ConfirmPanel.IsVisible = false;
        var pending = _pending;
        _pending = null;
        pending?.TrySetResult(confirmed);
    }

    void Confirm_Click(object? sender, RoutedEventArgs e) => Answer(true);
    void CancelConfirm_Click(object? sender, RoutedEventArgs e) => Answer(false);

    void AllSettings_Click(object? sender, RoutedEventArgs e)
    {
        Dismiss();
        _vm?.RequestOpenSettings();
    }
}
