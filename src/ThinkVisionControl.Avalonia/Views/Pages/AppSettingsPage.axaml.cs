using Avalonia.Controls;
using Avalonia.Input.Platform;
using Avalonia.Interactivity;
using ThinkVisionControl.Avalonia.Platform;
using ThinkVisionControl.Avalonia.ViewModels;
using ThinkVisionControl.Core.Logging;

namespace ThinkVisionControl.Avalonia.Views.Pages;

public partial class AppSettingsPage : UserControl
{
    public AppSettingsPage() => InitializeComponent();

    void EditHotkey_Click(object? sender, RoutedEventArgs e)
    {
        if (sender is Control { DataContext: HotkeyItem item } && TopLevel.GetTopLevel(this) is MainWindow window)
            _ = window.EditHotkeyAsync(item);
    }

    async void CopySetup_Click(object? sender, RoutedEventArgs e)
    {
        if (sender is not Button { DataContext: SetupHint { CopyText: { } text } } button) return;
        try
        {
            if (TopLevel.GetTopLevel(this)?.Clipboard is not { } clipboard) return;
            await clipboard.SetTextAsync(text);
            ToolTip.SetTip(button, "Copied");
            ToolTip.SetIsOpen(button, true);
        }
        catch (Exception ex)
        {
            FileLog.Write($"Copy failed: {ex.Message}");
        }
    }
}
