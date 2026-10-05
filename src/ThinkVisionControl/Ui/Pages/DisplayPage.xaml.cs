using System.Windows;
using System.Windows.Controls;
using System.Windows.Controls.Primitives;

namespace ThinkVisionControl.Ui.Pages;

public partial class DisplayPage : UserControl
{
    public DisplayPage() => InitializeComponent();

    void Input_Click(object sender, RoutedEventArgs e)
    {
        if (DataContext is PanelViewModel vm && sender is FrameworkElement { DataContext: EnumOption option })
            _ = vm.SelectInputAsync(option);
    }

    // The preset card's "More" button opens its menu below it.
    void More_Click(object sender, RoutedEventArgs e)
    {
        if (sender is not Button { ContextMenu: { } menu } button) return;
        menu.PlacementTarget = button;
        menu.Placement = PlacementMode.Bottom;
        menu.IsOpen = true;
    }
}
