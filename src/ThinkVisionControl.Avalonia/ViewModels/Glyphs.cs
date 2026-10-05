using ThinkVisionControl.Avalonia.Styles;

namespace ThinkVisionControl.Avalonia.ViewModels;

/// <summary>
/// The glyph for each place in the UI, in the bundled Fluent UI System Icons font (<c>IconFont</c>). Reuses
/// <see cref="IconGlyphs"/> where it has the icon; the rest are <c>_24_regular</c> codepoints read from the same font file.
/// </summary>
public static class Glyphs
{
    public const string Display = IconGlyphs.Display;
    /// <summary>ic_fluent_options_24_regular (sliders).</summary>
    public const string Options = "";
    public const string Settings = IconGlyphs.Settings;

    /// <summary>ic_fluent_weather_sunny_24_regular.</summary>
    public const string Brightness = IconGlyphs.Brightness;
    public const string Contrast = IconGlyphs.Contrast;
    public const string ColourMode = IconGlyphs.Color;
    public const string Input = IconGlyphs.Input;
    public const string Day = IconGlyphs.Brightness;
    public const string Night = IconGlyphs.Moon;

    /// <summary>ic_fluent_top_speed_24_regular.</summary>
    public const string OverDrive = "";
    /// <summary>ic_fluent_dark_theme_24_regular.</summary>
    public const string DynamicContrast = "";
    /// <summary>ic_fluent_full_screen_maximize_24_regular.</summary>
    public const string AspectRatio = "";
    /// <summary>ic_fluent_usb_plug_24_regular.</summary>
    public const string Usb = "";
    /// <summary>ic_fluent_local_language_24_regular.</summary>
    public const string Language = "";
    public const string Power = IconGlyphs.Power;
    public const string Reset = IconGlyphs.Reset;
    /// <summary>ic_fluent_search_24_regular.</summary>
    public const string Search = "";
    /// <summary>ic_fluent_app_generic_24_regular.</summary>
    public const string Background = "";
    public const string About = IconGlyphs.Info;
    /// <summary>ic_fluent_wrench_24_regular.</summary>
    public const string Setup = "";

    /// <summary>ic_fluent_checkmark_24_regular.</summary>
    public const string Connected = "";
    /// <summary>ic_fluent_arrow_sync_24_regular.</summary>
    public const string Connecting = "";
    /// <summary>ic_fluent_plug_disconnected_24_regular.</summary>
    public const string NotFound = "";
    public const string NotResponding = IconGlyphs.Error;

    public const string Edit = IconGlyphs.Edit;
    public const string More = IconGlyphs.More;
    /// <summary>ic_fluent_dismiss_24_regular.</summary>
    public const string Clear = "";
    /// <summary>ic_fluent_copy_24_regular.</summary>
    public const string Copy = "";
    public const string ChevronRight = IconGlyphs.ChevronRight;
    /// <summary>ic_fluent_scan_24_regular.</summary>
    public const string Snapshot = "";
    /// <summary>ic_fluent_arrow_sync_24_regular.</summary>
    public const string Compare = "";
}
