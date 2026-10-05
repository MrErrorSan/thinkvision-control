using System.Windows;
using System.Windows.Controls;
using System.Windows.Input;

namespace ThinkVisionControl.Ui;

public partial class PanelWindow : Window
{
    readonly PanelViewModel _vm;
    readonly Action _suspendHotkeys;
    readonly Action _resumeHotkeys;
    FinderWindow? _finder;
    bool _dialogOpen;
    bool _allowClose;

    public PanelWindow(PanelViewModel vm, Action suspendHotkeys, Action resumeHotkeys)
    {
        InitializeComponent();
        DataContext = _vm = vm;
        _suspendHotkeys = suspendHotkeys;
        _resumeHotkeys = resumeHotkeys;
        vm.Confirm = Confirm;
        Closing += (_, e) =>
        {
            if (_allowClose || !_vm.BackgroundMode) return;
            e.Cancel = true;
            Hide();
        };
    }

    /// <summary>Rebuilds from cache, restores the main window and brings it to the foreground.</summary>
    public void ShowMainWindow()
    {
        _vm.OnShowing();
        PanelScrollViewer.ScrollToTop();
        if (WindowState == WindowState.Minimized) WindowState = WindowState.Normal;
        Show();
        Activate();
    }

    internal void SetConnecting(bool connecting) => _vm.SetConnecting(connecting);

    bool Confirm(string message)
    {
        var wasDialogOpen = _dialogOpen;
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
            _dialogOpen = wasDialogOpen;
        }
    }

    void NavigationTabs_SelectionChanged(object sender, SelectionChangedEventArgs e)
    {
        if (!ReferenceEquals(e.Source, NavigationTabs) || NavigationTabs.SelectedIndex != 1) return;
        _vm.OnAdvancedExpanded();
        Dispatcher.BeginInvoke(System.Windows.Threading.DispatcherPriority.Loaded, () =>
        {
            AdvancedItemsControl.UpdateLayout();
            AdvancedScrollViewer.ScrollToTop();
        });
    }

    void Day_Click(object sender, RoutedEventArgs e) => _ = _vm.ApplyPresetAsync(night: false);
    void Night_Click(object sender, RoutedEventArgs e) => _ = _vm.ApplyPresetAsync(night: true);
    void SaveDay_Click(object sender, RoutedEventArgs e) => _vm.SavePreset(night: false);
    void SaveNight_Click(object sender, RoutedEventArgs e) => _vm.SavePreset(night: true);
    void Restore_Click(object sender, RoutedEventArgs e) => _ = _vm.RestoreBackupAsync();
    void ApplyRange_Click(object sender, RoutedEventArgs e)
    {
        if (sender is FrameworkElement { DataContext: RangeItem range })
            _ = _vm.ApplyRangeAsync(range);
    }

    internal void AllowClose() => _allowClose = true;

    void ChangeHotkey_Click(object sender, RoutedEventArgs e)
    {
        if (sender is not FrameworkElement { DataContext: HotkeyItem item }) return;

        var wasDialogOpen = _dialogOpen;
        var saved = false;
        _dialogOpen = true;
        _suspendHotkeys();
        try
        {
            var capture = new HotkeyCaptureWindow(item.Label) { Owner = this };
            if (capture.ShowDialog() != true) return;

            var shortcut = capture.Gesture.ToString();
            if (!Confirm($"Apply {shortcut} for {item.Label}?")) return;

            var previous = item.Text;
            item.Text = shortcut;
            saved = _vm.SaveHotkeys();
            if (!saved) item.Text = previous;
        }
        finally
        {
            _dialogOpen = wasDialogOpen;
            if (!saved) _resumeHotkeys();
        }
    }

    void Finder_Click(object sender, RoutedEventArgs e)
    {
        if (_finder is null)
        {
            _finder = new FinderWindow(_vm.Control, _vm.Profiles) { Owner = this };
            _finder.Closed += (_, _) =>
            {
                _finder = null;
                Activate();
            };
            _finder.Show();
        }
        _finder.Activate();
    }
}
