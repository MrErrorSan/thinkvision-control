using System.Windows;
using System.Windows.Controls;

namespace ThinkVisionControl.Ui.Controls;

/// <summary>An entry of a <see cref="NavigationPane"/>: a 16 px glyph and a label (the label hides while the pane is compact).</summary>
public class NavigationItem : ListBoxItem
{
    static NavigationItem()
    {
        DefaultStyleKeyProperty.OverrideMetadata(typeof(NavigationItem), new FrameworkPropertyMetadata(typeof(NavigationItem)));
    }

    /// <summary>Segoe Fluent Icons glyph.</summary>
    public static readonly DependencyProperty IconProperty =
        DependencyProperty.Register(nameof(Icon), typeof(string), typeof(NavigationItem));

    public static readonly DependencyProperty LabelProperty =
        DependencyProperty.Register(nameof(Label), typeof(string), typeof(NavigationItem));

    public string? Icon { get => (string?)GetValue(IconProperty); set => SetValue(IconProperty, value); }
    public string? Label { get => (string?)GetValue(LabelProperty); set => SetValue(LabelProperty, value); }
}
