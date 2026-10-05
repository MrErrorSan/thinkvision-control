using Avalonia.Controls;
using FluentAvalonia.Styling;
using FluentAvalonia.UI.Controls;
using ThinkVisionControl.Avalonia.Theming;

namespace ThinkVisionControl.Avalonia;

public partial class MainWindow : Window
{
    public MainWindow()
    {
        InitializeComponent();
        Backdrop.Apply(this);

        Nav.SelectedItem = NavDisplay;
        Nav.SelectionChanged += (_, e) =>
        {
            if (e.SelectedItem is FANavigationViewItem { Tag: string title }) PageTitle.Text = title;
        };

        // Windows: match the native title bar to the app theme (dark caption in dark mode).
        Opened += (_, _) =>
        {
            if (OperatingSystem.IsWindows())
                global::Avalonia.Application.Current?.Styles.OfType<FluentAvaloniaTheme>().FirstOrDefault()
                    ?.ForceWin32WindowToTheme(this, ActualThemeVariant);
        };
    }
}
