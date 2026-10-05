using Avalonia;
using Avalonia.Controls;
using Avalonia.Input;
using Avalonia.Interactivity;
using Avalonia.VisualTree;
using ThinkVisionControl.Avalonia.ViewModels;

namespace ThinkVisionControl.Avalonia.Views;

/// <summary>
/// Makes a Slider whose DataContext is a <see cref="RangeItem"/> write live: a pointer press on the thumb or the track
/// starts the item's drag and the release (or lost capture) ends it; value changes in between reach the item through
/// the Value binding. Keyboard changes don't press, so they take the item's debounced path.
/// </summary>
public sealed class LiveSlider : AvaloniaObject
{
    public static readonly AttachedProperty<bool> IsLiveProperty =
        AvaloniaProperty.RegisterAttached<LiveSlider, Slider, bool>("IsLive");

    static LiveSlider() => IsLiveProperty.Changed.AddClassHandler<Slider>(OnIsLiveChanged);

    public static bool GetIsLive(Slider slider) => slider.GetValue(IsLiveProperty);
    public static void SetIsLive(Slider slider, bool value) => slider.SetValue(IsLiveProperty, value);

    static void OnIsLiveChanged(Slider slider, AvaloniaPropertyChangedEventArgs e)
    {
        slider.RemoveHandler(InputElement.PointerPressedEvent, Pressed);
        slider.RemoveHandler(InputElement.PointerReleasedEvent, Released);
        slider.RemoveHandler(InputElement.PointerCaptureLostEvent, CaptureLost);
        if (e.NewValue is not true) return;
        // Tunnel, and handled too: the slider's track and thumb handle these themselves.
        slider.AddHandler(InputElement.PointerPressedEvent, Pressed, RoutingStrategies.Tunnel, handledEventsToo: true);
        slider.AddHandler(InputElement.PointerReleasedEvent, Released, RoutingStrategies.Tunnel | RoutingStrategies.Bubble, handledEventsToo: true);
        slider.AddHandler(InputElement.PointerCaptureLostEvent, CaptureLost, RoutingStrategies.Bubble, handledEventsToo: true);
    }

    static void Pressed(object? sender, PointerPressedEventArgs e)
    {
        if (sender is Slider { IsEffectivelyEnabled: true, DataContext: RangeItem item } slider
            && e.GetCurrentPoint(slider).Properties.IsLeftButtonPressed)
            item.BeginDrag();
    }

    static void Released(object? sender, PointerReleasedEventArgs e) => End(sender);

    // Capture moving between the slider's own parts (track → thumb) is not the end of a drag.
    static void CaptureLost(object? sender, PointerCaptureLostEventArgs e)
    {
        if (sender is Visual slider && e.Pointer.Captured is Visual now && (now == slider || slider.IsVisualAncestorOf(now))) return;
        End(sender);
    }

    static void End(object? sender)
    {
        if (sender is Slider { DataContext: RangeItem item }) item.EndDrag();
    }
}
