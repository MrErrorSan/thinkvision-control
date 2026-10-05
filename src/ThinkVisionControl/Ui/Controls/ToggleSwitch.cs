using System.Windows;
using System.Windows.Controls.Primitives;
using System.Windows.Media;
using System.Windows.Media.Animation;

namespace ThinkVisionControl.Ui.Controls;

/// <summary>Windows 11 toggle switch: "On"/"Off" label, then a 40×20 track whose knob slides when toggled.</summary>
public class ToggleSwitch : ToggleButton
{
    const double KnobTravel = 20;
    static readonly Duration SlideDuration = TimeSpan.FromMilliseconds(167);
    static readonly IEasingFunction SlideEasing = new CubicEase { EasingMode = EasingMode.EaseOut };

    readonly TranslateTransform _knobOffset = new();

    static ToggleSwitch()
    {
        DefaultStyleKeyProperty.OverrideMetadata(typeof(ToggleSwitch), new FrameworkPropertyMetadata(typeof(ToggleSwitch)));
    }

    public static readonly DependencyProperty OnContentProperty =
        DependencyProperty.Register(nameof(OnContent), typeof(object), typeof(ToggleSwitch), new FrameworkPropertyMetadata("On"));

    public static readonly DependencyProperty OffContentProperty =
        DependencyProperty.Register(nameof(OffContent), typeof(object), typeof(ToggleSwitch), new FrameworkPropertyMetadata("Off"));

    public object? OnContent { get => GetValue(OnContentProperty); set => SetValue(OnContentProperty, value); }
    public object? OffContent { get => GetValue(OffContentProperty); set => SetValue(OffContentProperty, value); }

    public override void OnApplyTemplate()
    {
        base.OnApplyTemplate();
        if (GetTemplateChild("PART_Knob") is UIElement knob) knob.RenderTransform = _knobOffset;
        MoveKnob(animate: false);
    }

    protected override void OnChecked(RoutedEventArgs e)
    {
        base.OnChecked(e);
        MoveKnob(animate: true);
    }

    protected override void OnUnchecked(RoutedEventArgs e)
    {
        base.OnUnchecked(e);
        MoveKnob(animate: true);
    }

    void MoveKnob(bool animate)
    {
        var to = IsChecked == true ? KnobTravel : 0;
        if (animate && IsLoaded && SystemParameters.ClientAreaAnimation)
            _knobOffset.BeginAnimation(TranslateTransform.XProperty,
                new DoubleAnimation(to, SlideDuration) { EasingFunction = SlideEasing });
        else
        {
            _knobOffset.BeginAnimation(TranslateTransform.XProperty, null);
            _knobOffset.X = to;
        }
    }
}
