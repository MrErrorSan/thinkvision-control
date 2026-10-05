using System.Windows;
using System.Windows.Controls;

namespace ThinkVisionControl.Ui.Pages;

public partial class AppSettingsPage : UserControl
{
    public AppSettingsPage() => InitializeComponent();

    void EditHotkey_Click(object sender, RoutedEventArgs e)
    {
        if (sender is FrameworkElement { DataContext: HotkeyItem item } && Window.GetWindow(this) is PanelWindow window)
            _ = window.EditHotkeyAsync(item);
    }
}
