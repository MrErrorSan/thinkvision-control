using System.Windows;
using System.Windows.Controls;
using System.Windows.Controls.Primitives;

namespace ThinkVisionControl.Ui.Pages;

/// <summary>
/// Makes a Slider whose DataContext is a <see cref="RangeItem"/> write live: thumb drags start and end the item's drag;
/// value changes in between reach the item through the Value binding. Also greys the thumb and fill while disabled,
/// which the WPF Fluent slider does not do itself.
/// </summary>
public static class LiveSlider
{
    const string ThumbKey = "SliderThumbBackground", DisabledKey = "AccentFillColorDisabledBrush";

    public static readonly DependencyProperty IsLiveProperty =
        DependencyProperty.RegisterAttached("IsLive", typeof(bool), typeof(LiveSlider), new PropertyMetadata(false, OnIsLiveChanged));

    public static bool GetIsLive(DependencyObject d) => (bool)d.GetValue(IsLiveProperty);
    public static void SetIsLive(DependencyObject d, bool value) => d.SetValue(IsLiveProperty, value);

    static readonly DragStartedEventHandler Started = (s, _) => ((s as FrameworkElement)?.DataContext as RangeItem)?.BeginDrag();
    static readonly DragCompletedEventHandler Completed = (s, _) => ((s as FrameworkElement)?.DataContext as RangeItem)?.EndDrag();

    static void OnIsLiveChanged(DependencyObject d, DependencyPropertyChangedEventArgs e)
    {
        if (d is not Slider slider) return;
        slider.RemoveHandler(Thumb.DragStartedEvent, Started);
        slider.RemoveHandler(Thumb.DragCompletedEvent, Completed);
        slider.IsEnabledChanged -= EnabledChanged;
        slider.Loaded -= SliderLoaded;
        if (!(bool)e.NewValue) return;
        slider.AddHandler(Thumb.DragStartedEvent, Started);
        slider.AddHandler(Thumb.DragCompletedEvent, Completed);
        slider.IsEnabledChanged += EnabledChanged;
        slider.Loaded += SliderLoaded;
    }

    static void EnabledChanged(object sender, DependencyPropertyChangedEventArgs e) => UpdateLook((Slider)sender);
    static void SliderLoaded(object sender, RoutedEventArgs e) => UpdateLook((Slider)sender);

    static void UpdateLook(Slider slider)
    {
        if (slider.IsEnabled) slider.Resources.Remove(ThumbKey);
        else if (slider.TryFindResource(DisabledKey) is { } brush) slider.Resources[ThumbKey] = brush;
    }
}
