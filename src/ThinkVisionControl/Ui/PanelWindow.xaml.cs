using System.Windows;
using System.Windows.Input;
using ThinkVisionControl.Ui.Controls;
using ThinkVisionControl.Ui.Pages;

namespace ThinkVisionControl.Ui;

/// <summary>
/// The settings window (layout A). It always really closes; the shared <see cref="PanelViewModel"/> outlives it and the
/// app shell creates a new window on the next show.
/// </summary>
public partial class PanelWindow : Window
{
    const double WideWidth = 1008, CompactPaneWidth = 48;

    public static readonly DependencyProperty CurrentPageProperty =
        DependencyProperty.Register(nameof(CurrentPage), typeof(NavigationItem), typeof(PanelWindow),
            new FrameworkPropertyMetadata(null, (d, e) => ((PanelWindow)d).Navigate(e.NewValue as NavigationItem)));

    readonly PanelViewModel _vm;
    readonly Action _suspendHotkeys;
    readonly Action _resumeHotkeys;
    readonly Func<DialogSpec, Task<bool>> _confirm;
    readonly Dictionary<NavigationItem, FrameworkElement> _pages = [];
    FinderWindow? _finder;
    bool _narrow, _collapsed, _overlayOpen;

    public PanelWindow(PanelViewModel vm, Action suspendHotkeys, Action resumeHotkeys)
    {
        InitializeComponent();
        DataContext = _vm = vm;
        _suspendHotkeys = suspendHotkeys;
        _resumeHotkeys = resumeHotkeys;
        vm.ConfirmAsync = _confirm = ConfirmAsync;
        CurrentPage = DisplayItem;
        SizeChanged += (_, _) => UpdatePane();
        PageHost.PreviewMouseDown += (_, _) => ClosePaneOverlay();
        Closed += OnClosed;
        UpdatePane();
    }

    /// <summary>The selected navigation item; setting it shows that page.</summary>
    public NavigationItem? CurrentPage
    {
        get => (NavigationItem?)GetValue(CurrentPageProperty);
        set => SetValue(CurrentPageProperty, value);
    }

    /// <summary>Shows the window, restores it if minimized and brings it to the foreground.</summary>
    public void ShowMainWindow()
    {
        _vm.OnShowing();
        if (WindowState == WindowState.Minimized) WindowState = WindowState.Normal;
        Show();
        Activate();
    }

    void OnClosed(object? sender, EventArgs e)
    {
        if (Equals(_vm.ConfirmAsync, _confirm)) _vm.ConfirmAsync = null;
        _pages.Clear();
        PageHost.Content = null;
    }

    Task<bool> ConfirmAsync(DialogSpec spec) =>
        IsVisible ? ConfirmCoreAsync(spec) : Task.FromResult(false);

    async Task<bool> ConfirmCoreAsync(DialogSpec spec) => await DialogHost.ShowAsync(spec) == ContentDialogResult.Primary;

    // ---------------------------------------------------------------- navigation

    void Navigate(NavigationItem? item)
    {
        if (item is null) return;
        if (!_pages.TryGetValue(item, out var page))
        {
            page = item == MonitorItem ? new MonitorPage() : item == AppItem ? new AppSettingsPage() : new DisplayPage();
            _pages[item] = page;
        }
        PageHost.Content = page;
        ClosePaneOverlay();
    }

    /// <summary>Expanded pane from 1008 px; icons only below that, where the menu button opens it over the content.</summary>
    void UpdatePane()
    {
        _narrow = ActualWidth > 0 && ActualWidth < WideWidth;
        if (!_narrow) _overlayOpen = false;
        Nav.IsCompact = _narrow ? !_overlayOpen : _collapsed;
        PaneOverlay.Visibility = _overlayOpen ? Visibility.Visible : Visibility.Collapsed;
        PageHost.Margin = new Thickness(_narrow || _collapsed ? CompactPaneWidth : Nav.OpenPaneLength, 0, 0, 0);
    }

    void Nav_PaneToggleRequested(object sender, RoutedEventArgs e)
    {
        e.Handled = true;
        if (_narrow) _overlayOpen = !_overlayOpen;
        else _collapsed = !_collapsed;
        UpdatePane();
    }

    void ClosePaneOverlay()
    {
        if (!_overlayOpen) return;
        _overlayOpen = false;
        UpdatePane();
    }

    protected override void OnPreviewKeyDown(KeyEventArgs e)
    {
        base.OnPreviewKeyDown(e);
        if (e.Handled || DialogHost.IsDialogOpen) return;
        if (e.Key == Key.Escape && _overlayOpen)
        {
            ClosePaneOverlay();
            e.Handled = true;
        }
        else if (e.Key == Key.F6)
        {
            // F6 moves focus between the pane and the page.
            if (Nav.IsKeyboardFocusWithin) (PageHost.Content as UIElement)?.MoveFocus(new TraversalRequest(FocusNavigationDirection.First));
            else (CurrentPage ?? DisplayItem).Focus();
            e.Handled = true;
        }
    }

    // ---------------------------------------------------------------- dialogs and tools

    internal async Task EditHotkeyAsync(HotkeyItem item)
    {
        var editor = new HotkeyEditor(item.Label, item.Text, text => _vm.HotkeyConflict(item.Action, text));
        editor.ValidityChanged += valid => DialogHost.IsPrimaryButtonEnabled = valid;
        DialogHost.IsPrimaryButtonEnabled = false;
        _suspendHotkeys();
        try
        {
            var result = await DialogHost.ShowAsync(new DialogSpec("Edit shortcut", editor, "Save", "Reset", "Cancel", DialogButton.Primary));
            if (result == ContentDialogResult.Primary)
            {
                _vm.SaveHotkey(item.Action, editor.Result);
            }
            else if (result == ContentDialogResult.Secondary)
            {
                var standard = PanelViewModel.DefaultHotkey(item.Action);
                if (_vm.HotkeyConflict(item.Action, standard) is { } other)
                    await DialogHost.ShowAsync(new DialogSpec($"{item.Label} can't use its default shortcut",
                        $"{other} already uses {standard}. Change that shortcut first.", "", null, "OK"));
                else
                    _vm.SaveHotkey(item.Action, standard);
            }
        }
        finally
        {
            _resumeHotkeys();
            _vm.RefreshHotkeyWarnings();
        }
    }

    internal void OpenFinder()
    {
        if (_finder is null)
        {
            _finder = new FinderWindow(_vm.Control, _vm.Profiles, _vm.RebuildIfControlsChanged) { Owner = this };
            _finder.Closed += (_, _) =>
            {
                _finder = null;
                if (IsVisible) Activate();
            };
            _finder.Show();
        }
        _finder.Activate();
    }
}
