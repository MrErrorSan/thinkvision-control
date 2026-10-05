using System.Windows;
using System.Windows.Automation;
using System.Windows.Automation.Peers;
using System.Windows.Controls;
using System.Windows.Controls.Primitives;
using System.Windows.Input;

namespace ThinkVisionControl.Ui.Controls;

public enum InfoBarSeverity { Informational, Success, Warning, Error }

/// <summary>
/// Inline status message (WinUI InfoBar): severity icon and background, <see cref="Title"/> and <see cref="Message"/>,
/// an optional action button and close button, and an optional indeterminate progress bar. Collapsed while
/// <see cref="IsOpen"/> is false.
/// </summary>
[TemplatePart(Name = "PART_CloseButton", Type = typeof(ButtonBase))]
public class InfoBar : Control
{
    ButtonBase? _closeButton;

    static InfoBar()
    {
        DefaultStyleKeyProperty.OverrideMetadata(typeof(InfoBar), new FrameworkPropertyMetadata(typeof(InfoBar)));
        FocusableProperty.OverrideMetadata(typeof(InfoBar), new FrameworkPropertyMetadata(false));
    }

    public static readonly DependencyProperty SeverityProperty =
        DependencyProperty.Register(nameof(Severity), typeof(InfoBarSeverity), typeof(InfoBar), new FrameworkPropertyMetadata(InfoBarSeverity.Informational));

    public static readonly DependencyProperty TitleProperty =
        DependencyProperty.Register(nameof(Title), typeof(string), typeof(InfoBar));

    public static readonly DependencyProperty MessageProperty =
        DependencyProperty.Register(nameof(Message), typeof(string), typeof(InfoBar));

    public static readonly DependencyProperty IsOpenProperty =
        DependencyProperty.Register(nameof(IsOpen), typeof(bool), typeof(InfoBar),
            new FrameworkPropertyMetadata(false, OnIsOpenChanged));

    public static readonly DependencyProperty IsClosableProperty =
        DependencyProperty.Register(nameof(IsClosable), typeof(bool), typeof(InfoBar), new FrameworkPropertyMetadata(true));

    /// <summary>Text of the action button; the button is shown only when this is non-empty.</summary>
    public static readonly DependencyProperty ActionContentProperty =
        DependencyProperty.Register(nameof(ActionContent), typeof(string), typeof(InfoBar));

    public static readonly DependencyProperty ActionCommandProperty =
        DependencyProperty.Register(nameof(ActionCommand), typeof(ICommand), typeof(InfoBar));

    public static readonly DependencyProperty ShowProgressProperty =
        DependencyProperty.Register(nameof(ShowProgress), typeof(bool), typeof(InfoBar), new FrameworkPropertyMetadata(false));

    /// <summary>Raised after the close button sets <see cref="IsOpen"/> to false.</summary>
    public static readonly RoutedEvent ClosedEvent =
        EventManager.RegisterRoutedEvent(nameof(Closed), RoutingStrategy.Bubble, typeof(RoutedEventHandler), typeof(InfoBar));

    public InfoBarSeverity Severity { get => (InfoBarSeverity)GetValue(SeverityProperty); set => SetValue(SeverityProperty, value); }
    public string? Title { get => (string?)GetValue(TitleProperty); set => SetValue(TitleProperty, value); }
    public string? Message { get => (string?)GetValue(MessageProperty); set => SetValue(MessageProperty, value); }
    public bool IsOpen { get => (bool)GetValue(IsOpenProperty); set => SetValue(IsOpenProperty, value); }
    public bool IsClosable { get => (bool)GetValue(IsClosableProperty); set => SetValue(IsClosableProperty, value); }
    public string? ActionContent { get => (string?)GetValue(ActionContentProperty); set => SetValue(ActionContentProperty, value); }
    public ICommand? ActionCommand { get => (ICommand?)GetValue(ActionCommandProperty); set => SetValue(ActionCommandProperty, value); }
    public bool ShowProgress { get => (bool)GetValue(ShowProgressProperty); set => SetValue(ShowProgressProperty, value); }

    public event RoutedEventHandler Closed { add => AddHandler(ClosedEvent, value); remove => RemoveHandler(ClosedEvent, value); }

    public override void OnApplyTemplate()
    {
        if (_closeButton != null) _closeButton.Click -= CloseClicked;
        base.OnApplyTemplate();
        _closeButton = GetTemplateChild("PART_CloseButton") as ButtonBase;
        if (_closeButton != null) _closeButton.Click += CloseClicked;
    }

    void CloseClicked(object sender, RoutedEventArgs e)
    {
        SetCurrentValue(IsOpenProperty, false);
        RaiseEvent(new RoutedEventArgs(ClosedEvent, this));
    }

    static void OnIsOpenChanged(DependencyObject d, DependencyPropertyChangedEventArgs e)
    {
        // Screen readers announce the bar when it (re)opens.
        if ((bool)e.NewValue && UIElementAutomationPeer.FromElement((InfoBar)d) is { } peer)
            peer.RaiseAutomationEvent(AutomationEvents.LiveRegionChanged);
    }

    protected override AutomationPeer OnCreateAutomationPeer() => new InfoBarAutomationPeer(this);

    sealed class InfoBarAutomationPeer(InfoBar owner) : FrameworkElementAutomationPeer(owner)
    {
        protected override string GetClassNameCore() => nameof(InfoBar);
        protected override AutomationControlType GetAutomationControlTypeCore() => AutomationControlType.StatusBar;
        protected override AutomationLiveSetting GetLiveSettingCore() =>
            ((InfoBar)Owner).Severity == InfoBarSeverity.Error ? AutomationLiveSetting.Assertive : AutomationLiveSetting.Polite;

        protected override string GetNameCore()
        {
            var name = base.GetNameCore();
            if (!string.IsNullOrEmpty(name)) return name;
            var bar = (InfoBar)Owner;
            return $"{bar.Severity}: {bar.Title} {bar.Message}".Trim();
        }
    }
}
