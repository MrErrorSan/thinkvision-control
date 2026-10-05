using System.Windows;
using System.Windows.Automation.Peers;
using System.Windows.Automation.Provider;
using System.Windows.Controls;
using System.Windows.Input;

namespace ThinkVisionControl.Ui.Controls;

/// <summary>
/// A Windows 11 settings row: icon, header and description on the left, the control (<see cref="ContentControl.Content"/>)
/// on the right. With <see cref="IsClickEnabled"/> the whole card is a button that raises <see cref="Click"/>.
/// </summary>
public class SettingsCard : ContentControl
{
    static SettingsCard()
    {
        DefaultStyleKeyProperty.OverrideMetadata(typeof(SettingsCard), new FrameworkPropertyMetadata(typeof(SettingsCard)));
        FocusableProperty.OverrideMetadata(typeof(SettingsCard), new FrameworkPropertyMetadata(false));
    }

    public static readonly DependencyProperty HeaderProperty =
        DependencyProperty.Register(nameof(Header), typeof(object), typeof(SettingsCard));

    public static readonly DependencyProperty DescriptionProperty =
        DependencyProperty.Register(nameof(Description), typeof(object), typeof(SettingsCard));

    /// <summary>Segoe Fluent Icons glyph, e.g. "".</summary>
    public static readonly DependencyProperty IconProperty =
        DependencyProperty.Register(nameof(Icon), typeof(string), typeof(SettingsCard));

    public static readonly DependencyProperty IsClickEnabledProperty =
        DependencyProperty.Register(nameof(IsClickEnabled), typeof(bool), typeof(SettingsCard),
            new FrameworkPropertyMetadata(false, (d, e) => ((SettingsCard)d).Focusable = (bool)e.NewValue));

    /// <summary>Glyph shown at the right edge while <see cref="IsClickEnabled"/> (default: chevron right E76C).</summary>
    public static readonly DependencyProperty ActionIconProperty =
        DependencyProperty.Register(nameof(ActionIcon), typeof(string), typeof(SettingsCard), new FrameworkPropertyMetadata(""));

    public static readonly DependencyProperty CornerRadiusProperty =
        DependencyProperty.Register(nameof(CornerRadius), typeof(CornerRadius), typeof(SettingsCard), new FrameworkPropertyMetadata(new CornerRadius(4)));

    static readonly DependencyPropertyKey IsPressedPropertyKey =
        DependencyProperty.RegisterReadOnly(nameof(IsPressed), typeof(bool), typeof(SettingsCard), new FrameworkPropertyMetadata(false));

    public static readonly DependencyProperty IsPressedProperty = IsPressedPropertyKey.DependencyProperty;

    public static readonly RoutedEvent ClickEvent =
        EventManager.RegisterRoutedEvent(nameof(Click), RoutingStrategy.Bubble, typeof(RoutedEventHandler), typeof(SettingsCard));

    public object? Header { get => GetValue(HeaderProperty); set => SetValue(HeaderProperty, value); }
    public object? Description { get => GetValue(DescriptionProperty); set => SetValue(DescriptionProperty, value); }
    public string? Icon { get => (string?)GetValue(IconProperty); set => SetValue(IconProperty, value); }
    public bool IsClickEnabled { get => (bool)GetValue(IsClickEnabledProperty); set => SetValue(IsClickEnabledProperty, value); }
    public string? ActionIcon { get => (string?)GetValue(ActionIconProperty); set => SetValue(ActionIconProperty, value); }
    public CornerRadius CornerRadius { get => (CornerRadius)GetValue(CornerRadiusProperty); set => SetValue(CornerRadiusProperty, value); }
    public bool IsPressed => (bool)GetValue(IsPressedProperty);

    public event RoutedEventHandler Click { add => AddHandler(ClickEvent, value); remove => RemoveHandler(ClickEvent, value); }

    protected override void OnMouseLeftButtonDown(MouseButtonEventArgs e)
    {
        base.OnMouseLeftButtonDown(e);
        if (e.Handled || !IsClickEnabled) return;
        Focus();
        if (CaptureMouse()) SetValue(IsPressedPropertyKey, true);
        e.Handled = true;
    }

    protected override void OnMouseLeftButtonUp(MouseButtonEventArgs e)
    {
        base.OnMouseLeftButtonUp(e);
        if (!IsPressed) return;
        var inside = new Rect(RenderSize).Contains(e.GetPosition(this));
        ReleaseMouseCapture();
        if (inside) RaiseClick();
        e.Handled = true;
    }

    protected override void OnLostMouseCapture(MouseEventArgs e)
    {
        base.OnLostMouseCapture(e);
        SetValue(IsPressedPropertyKey, false);
    }

    protected override void OnKeyDown(KeyEventArgs e)
    {
        base.OnKeyDown(e);
        if (e.Handled || !IsClickEnabled || e.OriginalSource != this || e.Key is not (Key.Enter or Key.Space)) return;
        RaiseClick();
        e.Handled = true;
    }

    void RaiseClick() => RaiseEvent(new RoutedEventArgs(ClickEvent, this));

    protected override AutomationPeer OnCreateAutomationPeer() => new SettingsCardAutomationPeer(this);

    sealed class SettingsCardAutomationPeer(SettingsCard owner) : FrameworkElementAutomationPeer(owner), IInvokeProvider
    {
        SettingsCard Card => (SettingsCard)Owner;

        protected override string GetClassNameCore() => nameof(SettingsCard);

        protected override AutomationControlType GetAutomationControlTypeCore() =>
            Card.IsClickEnabled ? AutomationControlType.Button : AutomationControlType.Group;

        protected override string GetNameCore()
        {
            var name = base.GetNameCore();
            return string.IsNullOrEmpty(name) && Card.Header is string header ? header : name;
        }

        protected override string GetHelpTextCore()
        {
            var help = base.GetHelpTextCore();
            return string.IsNullOrEmpty(help) && Card.Description is string description ? description : help;
        }

        public override object? GetPattern(PatternInterface patternInterface) =>
            patternInterface == PatternInterface.Invoke && Card.IsClickEnabled ? this : base.GetPattern(patternInterface);

        void IInvokeProvider.Invoke()
        {
            if (Card.IsEnabled && Card.IsClickEnabled) Card.RaiseClick();
        }
    }
}
