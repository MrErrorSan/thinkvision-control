using System.Runtime.CompilerServices;
using ThinkVisionControl.Core.Settings;

[assembly: InternalsVisibleTo("ThinkVisionControl.Tests")]

namespace ThinkVisionControl.Avalonia.Platform.MacOS;

/// <summary>
/// The pure decisions behind the macOS platform layer: key and modifier mapping, the Sequoia hotkey rule,
/// appearance names, window flags and flyout geometry. No native calls, so it runs (and is testable) anywhere.
/// </summary>
internal static class MacRules
{
    // ---- Hotkeys -------------------------------------------------------------------------------------------------

    // Carbon modifier masks (HIToolbox/Events.h): cmdKey = 1 << 8, shiftKey = 1 << 9, optionKey = 1 << 11, controlKey = 1 << 12.
    public const uint CmdKey = 0x0100, ShiftKey = 0x0200, OptionKey = 0x0800, ControlKey = 0x1000;

    /// <summary>
    /// Win32 virtual-key code → Carbon virtual key code (kVK_*, HIToolbox/Events.h). kVK codes are physical
    /// positions on an ANSI (US) keyboard, so a letter maps to the key in that letter's US-layout position.
    /// macOS has no F21–F24; Insert maps to Help (the key in Insert's position on Apple extended keyboards) and
    /// Delete to Forward Delete.
    /// </summary>
    private static readonly Dictionary<uint, uint> VirtualKeys = new()
    {
        // Letters A–Z (VK 0x41–0x5A)
        ['A'] = 0x00, ['S'] = 0x01, ['D'] = 0x02, ['F'] = 0x03, ['H'] = 0x04, ['G'] = 0x05, ['Z'] = 0x06, ['X'] = 0x07,
        ['C'] = 0x08, ['V'] = 0x09, ['B'] = 0x0B, ['Q'] = 0x0C, ['W'] = 0x0D, ['E'] = 0x0E, ['R'] = 0x0F, ['Y'] = 0x10,
        ['T'] = 0x11, ['O'] = 0x1F, ['U'] = 0x20, ['I'] = 0x22, ['P'] = 0x23, ['L'] = 0x25, ['J'] = 0x26, ['K'] = 0x28,
        ['N'] = 0x2D, ['M'] = 0x2E,
        // Digits 0–9 (VK 0x30–0x39), top row
        ['1'] = 0x12, ['2'] = 0x13, ['3'] = 0x14, ['4'] = 0x15, ['6'] = 0x16, ['5'] = 0x17, ['9'] = 0x19, ['7'] = 0x1A,
        ['8'] = 0x1C, ['0'] = 0x1D,
        // F1–F20 (VK_F1 = 0x70 … VK_F20 = 0x83)
        [0x70] = 0x7A, [0x71] = 0x78, [0x72] = 0x63, [0x73] = 0x76, [0x74] = 0x60, [0x75] = 0x61, [0x76] = 0x62,
        [0x77] = 0x64, [0x78] = 0x65, [0x79] = 0x6D, [0x7A] = 0x67, [0x7B] = 0x6F, [0x7C] = 0x69, [0x7D] = 0x6B,
        [0x7E] = 0x71, [0x7F] = 0x6A, [0x80] = 0x40, [0x81] = 0x4F, [0x82] = 0x50, [0x83] = 0x5A,
        // Navigation: VK_PRIOR, VK_NEXT, VK_END, VK_HOME, VK_LEFT, VK_UP, VK_RIGHT, VK_DOWN, VK_INSERT, VK_DELETE
        [0x21] = 0x74, // kVK_PageUp
        [0x22] = 0x79, // kVK_PageDown
        [0x23] = 0x77, // kVK_End
        [0x24] = 0x73, // kVK_Home
        [0x25] = 0x7B, // kVK_LeftArrow
        [0x26] = 0x7E, // kVK_UpArrow
        [0x27] = 0x7C, // kVK_RightArrow
        [0x28] = 0x7D, // kVK_DownArrow
        [0x2D] = 0x72, // kVK_Help
        [0x2E] = 0x75, // kVK_ForwardDelete
    };

    /// <summary>The Carbon key code for a Win32 virtual key; false for keys macOS doesn't have (F21–F24) or the app doesn't use.</summary>
    public static bool TryMapKey(uint virtualKey, out uint keyCode) => VirtualKeys.TryGetValue(virtualKey, out keyCode);

    /// <summary>Win32 MOD_* → Carbon modifier mask. Ctrl → control, Alt → option, Shift → shift, Win → command.</summary>
    public static uint MapModifiers(uint modifiers)
    {
        uint carbon = 0;
        if ((modifiers & HotkeyGesture.ModControl) != 0) carbon |= ControlKey;
        if ((modifiers & HotkeyGesture.ModAlt) != 0) carbon |= OptionKey;
        if ((modifiers & HotkeyGesture.ModShift) != 0) carbon |= ShiftKey;
        if ((modifiers & HotkeyGesture.ModWin) != 0) carbon |= CmdKey;
        return carbon;
    }

    /// <summary>
    /// macOS 15 (Sequoia) refuses hot keys whose only modifiers are Shift and/or Option, so at least Command or
    /// Control is required. Applied on every version so a shortcut that works today keeps working after an upgrade.
    /// </summary>
    public static bool IsAllowedModifierSet(uint carbonModifiers) => (carbonModifiers & (CmdKey | ControlKey)) != 0;

    /// <summary>Parses gesture text ("Ctrl+Alt+Up") into a Carbon key code and modifier mask macOS will accept.</summary>
    public static bool TryMapGesture(string text, out uint keyCode, out uint carbonModifiers)
    {
        keyCode = carbonModifiers = 0;
        if (!HotkeyGesture.TryParse(text, out var gesture) || !TryMapKey(gesture.VirtualKey, out keyCode)) return false;
        carbonModifiers = MapModifiers(gesture.Modifiers);
        return IsAllowedModifierSet(carbonModifiers);
    }

    // ---- Appearance ----------------------------------------------------------------------------------------------

    /// <summary>
    /// True for the dark NSAppearance names: NSAppearanceNameDarkAqua, NSAppearanceNameVibrantDark and the
    /// accessibility high-contrast dark variants (e.g. NSAppearanceNameAccessibilityHighContrastDarkAqua).
    /// </summary>
    public static bool IsDarkAppearanceName(string? name) =>
        name is not null && name.Contains("Dark", StringComparison.OrdinalIgnoreCase);

    // ---- Windows and bundles -------------------------------------------------------------------------------------

    /// <summary>NSStatusWindowLevel (= kCGStatusWindowLevel): above normal and floating windows, below menus.</summary>
    public const long StatusWindowLevel = 25;

    // NSWindowCollectionBehavior bits (AppKit/NSWindow.h).
    public const ulong CanJoinAllSpaces = 1 << 0, Stationary = 1 << 4, IgnoresCycle = 1 << 6, FullScreenAuxiliary = 1 << 8;

    /// <summary>On every Space, unaffected by Exposé, out of the window cycle, allowed over full-screen apps (= 337).</summary>
    public const ulong OverlayCollectionBehavior = CanJoinAllSpaces | Stationary | IgnoresCycle | FullScreenAuxiliary;

    /// <summary>True when the process runs from an application bundle (<c>NSBundle.mainBundle.bundlePath</c> ends in .app).</summary>
    public static bool IsAppBundlePath(string? bundlePath) =>
        bundlePath is not null && bundlePath.TrimEnd('/').EndsWith(".app", StringComparison.OrdinalIgnoreCase);

    // ---- Flyout geometry -----------------------------------------------------------------------------------------

    /// <summary>Gap (points) between the menu bar / screen edges and the flyout.</summary>
    public const double FlyoutGap = 6;

    /// <summary>True when the Cocoa point lies on the screen frame (edges inclusive: the cursor can sit on the top row).</summary>
    public static bool Contains(double x, double y, double frameX, double frameY, double frameWidth, double frameHeight) =>
        x >= frameX && x <= frameX + frameWidth && y >= frameY && y <= frameY + frameHeight;

    /// <summary>
    /// Top-left of a flyout centred under the cursor's x and just below the menu bar, kept inside the visible frame.
    /// Inputs are Cocoa global coordinates in points (origin bottom-left of the primary screen, y up); the visible
    /// frame excludes the menu bar and the Dock. The result is top-left-origin (y down from the primary screen's
    /// top) and multiplied by <paramref name="scale"/>, which is how Avalonia expresses macOS screen pixels.
    /// </summary>
    /// <param name="flyoutWidth">Flyout width in points (Avalonia DIPs).</param>
    public static (double X, double Y) FlyoutOrigin(double cursorX,
        double visibleX, double visibleY, double visibleWidth, double visibleHeight,
        double primaryScreenHeight, double flyoutWidth, double scale)
    {
        var minX = visibleX + FlyoutGap;
        var maxX = visibleX + visibleWidth - FlyoutGap - flyoutWidth;
        var left = cursorX - flyoutWidth / 2;
        left = maxX < minX ? minX : Math.Clamp(left, minX, maxX);

        // Cocoa y of the visible area's top edge → distance from the primary screen's top edge.
        // A flyout taller than the visible area still starts under the menu bar (its top stays reachable).
        var top = primaryScreenHeight - (visibleY + visibleHeight) + FlyoutGap;

        if (scale <= 0) scale = 1;
        return (left * scale, top * scale);
    }
}
