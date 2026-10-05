using Avalonia.Controls;
using Avalonia.Interactivity;
using ThinkVisionControl.Avalonia.ViewModels;

namespace ThinkVisionControl.Avalonia.Views.Pages;

public partial class DisplayPage : UserControl
{
    public DisplayPage() => InitializeComponent();

    void Input_Click(object? sender, RoutedEventArgs e)
    {
        if (DataContext is MainViewModel vm && sender is Control { DataContext: EnumOption option })
            _ = vm.SelectInputAsync(option);
    }
}
