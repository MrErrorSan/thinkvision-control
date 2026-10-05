using Avalonia.Markup.Xaml;
using FluentAvalonia.UI.Controls;

namespace ThinkVisionControl.Avalonia.Views;

/// <summary>
/// <c>IconSource="{v:Glyph {x:Static vm:Glyphs.Power}}"</c>: an icon in the bundled icon font for a fixed glyph.
/// (A bound glyph uses <see cref="Converters.Glyph"/> instead.)
/// </summary>
public sealed class GlyphExtension(string glyph) : MarkupExtension
{
    public string Glyph { get; } = glyph;

    public override object ProvideValue(IServiceProvider serviceProvider) =>
        new FAFontIconSource { Glyph = Glyph, FontFamily = Converters.IconFont };
}
