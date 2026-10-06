using System.Windows;
using System.Windows.Automation;
using System.Windows.Automation.Peers;
using System.Windows.Controls;
using System.Windows.Controls.Primitives;
using System.Windows.Threading;

namespace ThinkVisionControl.Ui.Pages;

public partial class DisplayPage : UserControl
{
    static readonly TimeSpan SavedTipDuration = TimeSpan.FromSeconds(2.5);

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

    // Saving answers under the More button: "Saved as Day", or "Couldn't save Day" with the reason on the card.
    void SavePreset_Click(object sender, RoutedEventArgs e)
    {
        if (sender is not MenuItem { DataContext: PresetItem item, Parent: ContextMenu { PlacementTarget: UIElement button } }) return;
        var saved = item.Save();
        ShowResultTip(button, saved, saved ? $"Saved as {item.Name}" : $"Couldn't save {item.Name}");
    }

    static void ShowResultTip(UIElement target, bool ok, string text)
    {
        var glyph = new TextBlock
        {
            Text = ok ? "" : "", // CheckMark, Error
            FontSize = 16,
            Margin = new Thickness(0, 0, 8, 0),
            VerticalAlignment = VerticalAlignment.Center,
        };
        glyph.SetResourceReference(TextBlock.FontFamilyProperty, "IconFont");
        glyph.SetResourceReference(TextBlock.ForegroundProperty, ok ? "SystemFillColorSuccessBrush" : "SystemFillColorCriticalBrush");
        var tip = new ToolTip
        {
            Content = new StackPanel { Orientation = Orientation.Horizontal, Children = { glyph, new TextBlock { Text = text } } },
            PlacementTarget = target,
            Placement = PlacementMode.Bottom,
            StaysOpen = true,
        };
        tip.IsOpen = true;
        var timer = new DispatcherTimer { Interval = SavedTipDuration };
        timer.Tick += (_, _) => { timer.Stop(); tip.IsOpen = false; };
        timer.Start();

        UIElementAutomationPeer.CreatePeerForElement(target)?.RaiseNotificationEvent(AutomationNotificationKind.ActionCompleted,
            AutomationNotificationProcessing.ImportantMostRecent, text, "PresetSaved");
    }
}
