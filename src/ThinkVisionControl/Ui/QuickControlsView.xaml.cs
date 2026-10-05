using System.Windows;
using System.Windows.Controls;
using ThinkVisionControl.Ui.Controls;

namespace ThinkVisionControl.Ui;

/// <summary>
/// Quick controls shown in the tray flyout. DataContext is the shared <see cref="PanelViewModel"/>. Switching the
/// input away from this PC asks inline rather than in a dialog.
/// </summary>
public partial class QuickControlsView : UserControl
{
    TaskCompletionSource<bool>? _pending;

    public QuickControlsView()
    {
        InitializeComponent();
        // A question left open must not outlive the flyout.
        IsVisibleChanged += (_, e) => { if (!(bool)e.NewValue) Answer(false); };
    }

    void InputTile_Click(object sender, RoutedEventArgs e)
    {
        if (DataContext is PanelViewModel vm && sender is FrameworkElement { DataContext: EnumOption option })
            _ = vm.SelectInputAsync(option, ConfirmInlineAsync);
    }

    Task<bool> ConfirmInlineAsync(DialogSpec spec)
    {
        Answer(false);
        ConfirmTitle.Text = spec.Title;
        ConfirmMessage.Text = spec.Content as string;
        ConfirmButton.Content = spec.PrimaryText;
        ConfirmPanel.Visibility = Visibility.Visible;
        CancelButton.Focus();
        _pending = new TaskCompletionSource<bool>();
        return _pending.Task;
    }

    void Answer(bool confirmed)
    {
        ConfirmPanel.Visibility = Visibility.Collapsed;
        var pending = _pending;
        _pending = null;
        pending?.TrySetResult(confirmed);
    }

    void Confirm_Click(object sender, RoutedEventArgs e) => Answer(true);
    void CancelConfirm_Click(object sender, RoutedEventArgs e) => Answer(false);

    void AllSettings_Click(object sender, RoutedEventArgs e) => (DataContext as PanelViewModel)?.RequestOpenSettings();
}
