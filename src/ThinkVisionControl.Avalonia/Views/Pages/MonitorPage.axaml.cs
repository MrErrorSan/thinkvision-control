using Avalonia.Controls;
using Avalonia.Interactivity;

namespace ThinkVisionControl.Avalonia.Views.Pages;

public partial class MonitorPage : UserControl
{
    public MonitorPage() => InitializeComponent();

    void FindHidden_Click(object? sender, RoutedEventArgs e) => (TopLevel.GetTopLevel(this) as MainWindow)?.OpenFinder();
}
