using System.Windows;
using System.Windows.Controls;
using System.Windows.Input;

namespace ThinkVisionControl.Ui;

public partial class PanelWindow : Window
{
    const double ScreenMargin = 12;

    readonly PanelViewModel _vm;
    FinderWindow? _finder;
    bool _dialogOpen;

    public PanelWindow(PanelViewModel vm)
    {
        InitializeComponent();
        DataContext = _vm = vm;
        vm.Confirm = Confirm;
        Deactivated += (_, _) => { if (!_dialogOpen) Hide(); };
        SizeChanged += (_, _) => Reposition();
        KeyDown += (_, e) => { if (e.Key == Key.Escape) Hide(); };
    }

    /// <summary>Rebuilds from cache, shows bottom-right of the primary work area and takes focus.</summary>
    public void ShowNearTray()
    {
        MaxHeight = SystemParameters.WorkArea.Height - 2 * ScreenMargin;
        _vm.OnShowing();
        if (AdvancedExpander.IsExpanded) _vm.OnAdvancedExpanded();
        Show();
        Reposition();
        Activate();
    }

    void Reposition()
    {
        var area = SystemParameters.WorkArea;
        Left = area.Right - ActualWidth - ScreenMargin;
        Top = area.Bottom - ActualHeight - ScreenMargin;
    }

    bool Confirm(string message)
    {
        _dialogOpen = true;
        try
        {
            var result = IsVisible
                ? MessageBox.Show(this, message, "ThinkVision Control", MessageBoxButton.YesNo, MessageBoxImage.Warning, MessageBoxResult.No)
                : MessageBox.Show(message, "ThinkVision Control", MessageBoxButton.YesNo, MessageBoxImage.Warning, MessageBoxResult.No);
            return result == MessageBoxResult.Yes;
        }
        finally
        {
            _dialogOpen = false;
        }
    }

    // Only user-driven changes write; binding updates and range coercion are ignored.
    void Slider_ValueChanged(object sender, RoutedPropertyChangedEventArgs<double> e)
    {
        if (sender is Slider { DataContext: RangeItem item } slider && (slider.IsMouseCaptureWithin || slider.IsKeyboardFocusWithin))
            item.UserSet(e.NewValue);
    }

    void Advanced_Expanded(object sender, RoutedEventArgs e) => _vm.OnAdvancedExpanded();

    void Day_Click(object sender, RoutedEventArgs e) => _ = _vm.ApplyPresetAsync(night: false);
    void Night_Click(object sender, RoutedEventArgs e) => _ = _vm.ApplyPresetAsync(night: true);
    void SaveDay_Click(object sender, RoutedEventArgs e) => _vm.SavePreset(night: false);
    void SaveNight_Click(object sender, RoutedEventArgs e) => _vm.SavePreset(night: true);
    void Restore_Click(object sender, RoutedEventArgs e) => _ = _vm.RestoreBackupAsync();
    void SaveHotkeys_Click(object sender, RoutedEventArgs e) => _vm.SaveHotkeys();

    void Finder_Click(object sender, RoutedEventArgs e)
    {
        Hide();
        if (_finder is null)
        {
            _finder = new FinderWindow(_vm.Control, _vm.Profiles);
            _finder.Closed += (_, _) => _finder = null;
            _finder.Show();
        }
        _finder.Activate();
    }
}
