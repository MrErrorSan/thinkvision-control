using System.Windows;
using System.Windows.Automation;
using System.Windows.Controls;
using System.Windows.Controls.Primitives;
using System.Windows.Input;
using System.Windows.Media;
using System.Windows.Media.Animation;

namespace ThinkVisionControl.Ui.Controls;

public enum ContentDialogResult { None, Primary, Secondary }

public enum DialogButton { None, Primary, Secondary, Close }

/// <param name="Title">The question, e.g. "Reset monitor to factory settings?".</param>
/// <param name="Content">Body: a string (wraps) or any element.</param>
/// <param name="PrimaryText">Verb for the action button.</param>
/// <param name="SecondaryText">Optional second action; null or empty hides it.</param>
/// <param name="CloseText">The safe choice, e.g. "Cancel". Esc always means Close.</param>
/// <param name="Default">Accent style, initial focus and Enter.</param>
/// <param name="PrimaryIsDestructive">The primary button never becomes the default: if <paramref name="Default"/> is
/// Primary, the Close button takes the accent style, initial focus and Enter instead.</param>
public sealed record DialogSpec(string Title, object? Content, string PrimaryText, string? SecondaryText, string CloseText,
    DialogButton Default = DialogButton.Close, bool PrimaryIsDestructive = false);

/// <summary>
/// Hosts a window's page (<see cref="ContentControl.Content"/>) and shows WinUI-style content dialogs over it:
/// a smoke scrim and a centred card. Put it at the root of the window.
/// </summary>
/// <remarks>
/// One dialog is visible at a time. Calling <see cref="ShowAsync"/> while a dialog is open queues the new dialog;
/// it opens after the current one closes (first in, first out). If the host unloads (window closed), the open dialog
/// completes with <see cref="ContentDialogResult.None"/>.
/// </remarks>
[TemplatePart(Name = "PART_Overlay", Type = typeof(FrameworkElement))]
[TemplatePart(Name = "PART_Dialog", Type = typeof(FrameworkElement))]
[TemplatePart(Name = "PART_Title", Type = typeof(TextBlock))]
[TemplatePart(Name = "PART_Body", Type = typeof(ContentPresenter))]
[TemplatePart(Name = "PART_PrimaryButton", Type = typeof(Button))]
[TemplatePart(Name = "PART_SecondaryButton", Type = typeof(Button))]
[TemplatePart(Name = "PART_CloseButton", Type = typeof(Button))]
public class ContentDialogHost : ContentControl
{
    FrameworkElement? _overlay, _dialog;
    TextBlock? _title;
    ContentPresenter? _body;
    Button? _primary, _secondary, _close, _default;
    TaskCompletionSource<ContentDialogResult>? _open;
    Task _queue = Task.CompletedTask;
    IInputElement? _restoreFocus;

    static ContentDialogHost()
    {
        DefaultStyleKeyProperty.OverrideMetadata(typeof(ContentDialogHost), new FrameworkPropertyMetadata(typeof(ContentDialogHost)));
        FocusableProperty.OverrideMetadata(typeof(ContentDialogHost), new FrameworkPropertyMetadata(false));
    }

    public ContentDialogHost() => Unloaded += (_, _) => Close(ContentDialogResult.None);

    /// <summary>
    /// Enables the primary button of the open dialog; may change while it is open (e.g. Save stays disabled until a
    /// shortcut is valid). Set it before calling <see cref="ShowAsync"/>; it returns to true when the dialog closes.
    /// </summary>
    public static readonly DependencyProperty IsPrimaryButtonEnabledProperty =
        DependencyProperty.Register(nameof(IsPrimaryButtonEnabled), typeof(bool), typeof(ContentDialogHost), new FrameworkPropertyMetadata(true));

    static readonly DependencyPropertyKey IsDialogOpenPropertyKey =
        DependencyProperty.RegisterReadOnly(nameof(IsDialogOpen), typeof(bool), typeof(ContentDialogHost), new FrameworkPropertyMetadata(false));

    public static readonly DependencyProperty IsDialogOpenProperty = IsDialogOpenPropertyKey.DependencyProperty;

    public bool IsPrimaryButtonEnabled { get => (bool)GetValue(IsPrimaryButtonEnabledProperty); set => SetValue(IsPrimaryButtonEnabledProperty, value); }
    public bool IsDialogOpen => (bool)GetValue(IsDialogOpenProperty);

    public Task<ContentDialogResult> ShowAsync(DialogSpec spec)
    {
        ArgumentNullException.ThrowIfNull(spec);
        VerifyAccess();
        var shown = ShowAfterAsync(_queue, spec);
        _queue = shown.ContinueWith(static _ => { }, CancellationToken.None, TaskContinuationOptions.ExecuteSynchronously, TaskScheduler.Default);
        return shown;
    }

    async Task<ContentDialogResult> ShowAfterAsync(Task previous, DialogSpec spec)
    {
        await previous;
        return await Open(spec);
    }

    public override void OnApplyTemplate()
    {
        foreach (var button in new[] { _primary, _secondary, _close })
            if (button != null) button.Click -= ButtonClicked;
        base.OnApplyTemplate();
        _overlay = GetTemplateChild("PART_Overlay") as FrameworkElement;
        _dialog = GetTemplateChild("PART_Dialog") as FrameworkElement;
        _title = GetTemplateChild("PART_Title") as TextBlock;
        _body = GetTemplateChild("PART_Body") as ContentPresenter;
        _primary = GetTemplateChild("PART_PrimaryButton") as Button;
        _secondary = GetTemplateChild("PART_SecondaryButton") as Button;
        _close = GetTemplateChild("PART_CloseButton") as Button;
        foreach (var button in new[] { _primary, _secondary, _close })
            if (button != null) button.Click += ButtonClicked;
    }

    Task<ContentDialogResult> Open(DialogSpec spec)
    {
        ApplyTemplate();
        if (_overlay is null || _dialog is null || _title is null || _body is null || _primary is null || _secondary is null || _close is null)
            throw new InvalidOperationException("ContentDialogHost template is missing its parts.");

        _title.Text = spec.Title;
        _body.Content = spec.Content;
        _body.Visibility = spec.Content is null ? Visibility.Collapsed : Visibility.Visible;
        AutomationProperties.SetName(_dialog, spec.Title);
        SetButton(_primary, spec.PrimaryText);
        SetButton(_secondary, spec.SecondaryText);
        SetButton(_close, spec.CloseText);

        var defaultButton = spec.Default == DialogButton.Primary && spec.PrimaryIsDestructive ? DialogButton.Close : spec.Default;
        _default = defaultButton switch
        {
            DialogButton.Primary => _primary,
            DialogButton.Secondary => _secondary,
            DialogButton.Close => _close,
            _ => null,
        };
        if (_default is { Visibility: not Visibility.Visible }) _default = null;
        foreach (var button in new[] { _primary, _secondary, _close })
            if (button == _default) button.SetResourceReference(StyleProperty, "AccentButtonStyle");
            else button.ClearValue(StyleProperty);

        _restoreFocus = Keyboard.FocusedElement is Visual focused && IsAncestorOf(focused) ? Keyboard.FocusedElement : null;
        _open = new TaskCompletionSource<ContentDialogResult>(TaskCreationOptions.RunContinuationsAsynchronously);
        SetValue(IsDialogOpenPropertyKey, true);
        _overlay.Visibility = Visibility.Visible;
        Animate();
        _overlay.UpdateLayout();
        // The default button takes focus; if it is disabled, the first focusable element in the dialog does.
        if (_default?.Focus() != true) _dialog.MoveFocus(new TraversalRequest(FocusNavigationDirection.First));
        return _open.Task;
    }

    static void SetButton(Button button, string? text)
    {
        button.Content = text;
        button.Visibility = string.IsNullOrEmpty(text) ? Visibility.Collapsed : Visibility.Visible;
    }

    void Animate()
    {
        if (!SystemParameters.ClientAreaAnimation || _overlay is null || _dialog is null) return;
        var ease = new CubicEase { EasingMode = EasingMode.EaseOut };
        _overlay.BeginAnimation(OpacityProperty, new DoubleAnimation(0, 1, TimeSpan.FromMilliseconds(167)));
        var scale = new ScaleTransform(1.05, 1.05);
        _dialog.RenderTransform = scale;
        var grow = new DoubleAnimation(1.05, 1, TimeSpan.FromMilliseconds(250)) { EasingFunction = ease };
        scale.BeginAnimation(ScaleTransform.ScaleXProperty, grow);
        scale.BeginAnimation(ScaleTransform.ScaleYProperty, grow);
    }

    void ButtonClicked(object sender, RoutedEventArgs e) =>
        Close(sender == _primary ? ContentDialogResult.Primary : sender == _secondary ? ContentDialogResult.Secondary : ContentDialogResult.None);

    void Close(ContentDialogResult result)
    {
        if (_open is null) return;
        var open = _open;
        _open = null;
        _default = null;
        if (_overlay != null) _overlay.Visibility = Visibility.Collapsed;
        if (_body != null) _body.Content = null;
        SetValue(IsDialogOpenPropertyKey, false);
        SetCurrentValue(IsPrimaryButtonEnabledProperty, true);
        if (_restoreFocus is UIElement { IsVisible: true } restore) restore.Focus();
        else (Content as UIElement)?.MoveFocus(new TraversalRequest(FocusNavigationDirection.First));
        _restoreFocus = null;
        open.SetResult(result);
    }

    protected override void OnKeyDown(KeyEventArgs e)
    {
        base.OnKeyDown(e);
        if (_open is null || e.Handled) return;
        if (e.Key == Key.Escape)
        {
            Close(ContentDialogResult.None);
            e.Handled = true;
        }
        else if (e.Key == Key.Enter && e.OriginalSource is not ButtonBase && _default is { IsEnabled: true })
        {
            ButtonClicked(_default, e);
            e.Handled = true;
        }
    }

    // Focus is trapped in the dialog while it is open.
    protected override void OnPreviewGotKeyboardFocus(KeyboardFocusChangedEventArgs e)
    {
        base.OnPreviewGotKeyboardFocus(e);
        if (_open != null && _dialog != null && e.NewFocus is Visual target && !_dialog.IsAncestorOf(target)) e.Handled = true;
    }
}
