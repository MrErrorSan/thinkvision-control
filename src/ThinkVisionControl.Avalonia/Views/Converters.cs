using Avalonia.Controls;
using Avalonia.Data.Converters;
using Avalonia.Media;
using FluentAvalonia.UI.Controls;

namespace ThinkVisionControl.Avalonia.Views;

public static class Converters
{
    /// <summary>A glyph codepoint (Glyphs / IconGlyphs) → an icon in the bundled icon font; null → no icon.</summary>
    public static readonly IValueConverter Glyph = new FuncValueConverter<string?, FAIconSource?>(glyph =>
        string.IsNullOrEmpty(glyph) ? null : new FAFontIconSource { Glyph = glyph, FontFamily = IconFont });

    /// <summary>A string → a one-item list (an expander's single child row); null → no rows.</summary>
    public static readonly IValueConverter SingleItem = new FuncValueConverter<string?, IReadOnlyList<string>>(text =>
        string.IsNullOrEmpty(text) ? [] : [text]);

    /// <summary>A toggle's state as the label shown left of it.</summary>
    public static readonly IValueConverter OnOff = new FuncValueConverter<bool?, string>(on => on == true ? "On" : "Off");

    /// <summary>The <c>IconFont</c> resource from Styles/Theme.axaml.</summary>
    public static FontFamily IconFont =>
        global::Avalonia.Application.Current?.TryFindResource("IconFont", out var font) == true && font is FontFamily family
            ? family
            : FontFamily.Default;
}
