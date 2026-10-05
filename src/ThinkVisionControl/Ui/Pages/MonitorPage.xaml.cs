using System.Windows;
using System.Windows.Controls;

namespace ThinkVisionControl.Ui.Pages;

public partial class MonitorPage : UserControl
{
    public MonitorPage() => InitializeComponent();

    void FindHidden_Click(object sender, RoutedEventArgs e) => (Window.GetWindow(this) as PanelWindow)?.OpenFinder();
}
