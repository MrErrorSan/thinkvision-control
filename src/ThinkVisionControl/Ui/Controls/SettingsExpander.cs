using System.Windows;
using System.Windows.Controls;

namespace ThinkVisionControl.Ui.Controls;

/// <summary>
/// An <see cref="Expander"/> whose header looks like a <see cref="SettingsCard"/> (icon, <see cref="HeaderedContentControl.Header"/>,
/// <see cref="Description"/>, <see cref="HeaderContent"/> on the right, then a chevron). Put the child settings in a
/// StackPanel as <see cref="ContentControl.Content"/>: SettingsCards inside are restyled as indented rows with separators.
/// </summary>
public class SettingsExpander : Expander
{
    static SettingsExpander()
    {
        DefaultStyleKeyProperty.OverrideMetadata(typeof(SettingsExpander), new FrameworkPropertyMetadata(typeof(SettingsExpander)));
    }

    public static readonly DependencyProperty DescriptionProperty =
        DependencyProperty.Register(nameof(Description), typeof(object), typeof(SettingsExpander));

    /// <summary>Segoe Fluent Icons glyph.</summary>
    public static readonly DependencyProperty IconProperty =
        DependencyProperty.Register(nameof(Icon), typeof(string), typeof(SettingsExpander));

    /// <summary>A control shown at the right of the header row, before the chevron (e.g. a ToggleSwitch).</summary>
    public static readonly DependencyProperty HeaderContentProperty =
        DependencyProperty.Register(nameof(HeaderContent), typeof(object), typeof(SettingsExpander));

    public object? Description { get => GetValue(DescriptionProperty); set => SetValue(DescriptionProperty, value); }
    public string? Icon { get => (string?)GetValue(IconProperty); set => SetValue(IconProperty, value); }
    public object? HeaderContent { get => GetValue(HeaderContentProperty); set => SetValue(HeaderContentProperty, value); }
}
