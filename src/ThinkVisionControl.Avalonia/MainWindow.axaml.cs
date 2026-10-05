using Avalonia.Controls;
using Avalonia.Layout;
using Avalonia.Media;
using FluentAvalonia.Styling;
using FluentAvalonia.UI.Controls;
using ThinkVisionControl.Avalonia.Theming;
using ThinkVisionControl.Avalonia.ViewModels;
using ThinkVisionControl.Avalonia.Views;
using ThinkVisionControl.Avalonia.Views.Pages;

namespace ThinkVisionControl.Avalonia;

/// <summary>
/// The settings window (layout A). It always really closes; the shared <see cref="MainViewModel"/> outlives it and the
/// app shell creates a new window on the next show.
/// </summary>
public partial class MainWindow : Window
{
    readonly MainViewModel? _vm;
    readonly Func<DialogSpec, Task<bool>>? _confirm;
    readonly Dictionary<FANavigationViewItem, Control> _pages = [];
    FinderWindow? _finder;
    bool _dialogOpen;

    /// <summary>Designer only: no view model, so the pages show no data.</summary>
    public MainWindow()
    {
        InitializeComponent();
        Backdrop.Apply(this);
        Nav.SelectionChanged += (_, e) => Navigate(e.SelectedItem as FANavigationViewItem);
        Nav.SelectedItem = NavDisplay;
        Navigate(NavDisplay);
        Opened += (_, _) => MatchTitleBarToTheme();
        ActualThemeVariantChanged += (_, _) => MatchTitleBarToTheme();
    }

    public MainWindow(MainViewModel vm) : this()
    {
        DataContext = _vm = vm;
        vm.ConfirmAsync = _confirm = ConfirmAsync;
        Closed += OnClosed;
    }

    /// <summary>Shows the window, restores it if minimized and brings it to the foreground.</summary>
    public void ShowMainWindow()
    {
        _vm?.OnShowing();
        if (WindowState == WindowState.Minimized) WindowState = WindowState.Normal;
        Show();
        Activate();
    }

    /// <summary>Selects a page: "display", "monitor" or "app".</summary>
    public void ShowPage(string page) =>
        Nav.SelectedItem = page switch { "monitor" => NavMonitor, "app" => NavApp, _ => NavDisplay };

    void OnClosed(object? sender, EventArgs e)
    {
        if (_vm is not null && Equals(_vm.ConfirmAsync, _confirm)) _vm.ConfirmAsync = null;
        _finder?.Close();
        _pages.Clear();
        PageHost.Content = null;
    }

    // Windows: the native title bar follows the app theme (dark caption in dark mode).
    void MatchTitleBarToTheme()
    {
        if (OperatingSystem.IsWindows())
            global::Avalonia.Application.Current?.Styles.OfType<FluentAvaloniaTheme>().FirstOrDefault()
                ?.ForceWin32WindowToTheme(this, ActualThemeVariant);
    }

    void Navigate(FANavigationViewItem? item)
    {
        if (item is null) return;
        if (!_pages.TryGetValue(item, out var page))
        {
            page = item == NavMonitor ? new MonitorPage() : item == NavApp ? new AppSettingsPage() : new DisplayPage();
            _pages[item] = page;
        }
        PageHost.Content = page;
    }

    // ---------------------------------------------------------------- dialogs

    Task<bool> ConfirmAsync(DialogSpec spec) =>
        IsVisible && !_dialogOpen ? ConfirmCoreAsync(spec) : Task.FromResult(false);

    async Task<bool> ConfirmCoreAsync(DialogSpec spec) => await ShowDialogAsync(spec) == FAContentDialogResult.Primary;

    async Task<FAContentDialogResult> ShowDialogAsync(DialogSpec spec, Action<FAContentDialog>? setup = null)
    {
        var dialog = new FAContentDialog
        {
            Title = spec.Title,
            Content = spec.Content is string text ? new TextBlock { Text = text, TextWrapping = TextWrapping.Wrap, MaxWidth = 440 } : spec.Content,
            PrimaryButtonText = spec.PrimaryText,
            SecondaryButtonText = spec.SecondaryText ?? "",
            CloseButtonText = spec.CloseText,
            DefaultButton = spec.Default switch
            {
                DialogButton.Primary => FAContentDialogButton.Primary,
                DialogButton.Secondary => FAContentDialogButton.Secondary,
                DialogButton.Close => FAContentDialogButton.Close,
                _ => FAContentDialogButton.None,
            },
        };
        setup?.Invoke(dialog);
        _dialogOpen = true;
        try { return await dialog.ShowAsync(this); }
        finally { _dialogOpen = false; }
    }

    /// <summary>The shortcut editor for one action: Save (once valid), Reset to default, or Cancel.</summary>
    internal async Task EditHotkeyAsync(HotkeyItem item)
    {
        if (_vm is not { } vm || _dialogOpen) return;
        var editor = new ShortcutEditor(item.Label, item.Text, text => vm.HotkeyConflict(item.Action, text));
        vm.SuspendHotkeys();
        var saved = false;
        try
        {
            var result = await ShowDialogAsync(
                new DialogSpec("Edit shortcut", editor, "Save", "Reset to default", "Cancel", DialogButton.Primary),
                dialog =>
                {
                    dialog.IsPrimaryButtonEnabled = false;
                    editor.ValidityChanged += valid => dialog.IsPrimaryButtonEnabled = valid;
                    dialog.Opened += (_, _) => editor.FocusCapture();
                });
            if (result == FAContentDialogResult.Primary)
            {
                saved = vm.SaveHotkey(item.Action, editor.Result);
            }
            else if (result == FAContentDialogResult.Secondary)
            {
                var standard = MainViewModel.DefaultHotkey(item.Action);
                if (vm.HotkeyConflict(item.Action, standard) is { } other)
                    await ShowDialogAsync(new DialogSpec($"{item.Label} can't use its default shortcut",
                        $"{other} already uses {string.Join("+", HotkeyItem.KeysOf(standard))}. Change that shortcut first.", "", null, "OK"));
                else
                    saved = vm.SaveHotkey(item.Action, standard);
            }
        }
        finally
        {
            // A save already told the shell (settingsChanged), which re-registers; otherwise ask it to.
            if (!saved) vm.ResumeHotkeys();
        }
    }

    // ---------------------------------------------------------------- tools

    internal void OpenFinder()
    {
        if (_vm is not { } vm) return;
        if (_finder is null)
        {
            _finder = new FinderWindow(vm.Control, vm.Profiles, vm.RebuildIfControlsChanged);
            _finder.Closed += (_, _) =>
            {
                _finder = null;
                if (IsVisible) Activate();
            };
            _finder.Show(this);
        }
        _finder.Activate();
    }
}
