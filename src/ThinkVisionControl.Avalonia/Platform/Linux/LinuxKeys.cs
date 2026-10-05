using ThinkVisionControl.Core.Settings;

namespace ThinkVisionControl.Avalonia.Platform.Linux;

/// <summary>Pure translations from the Win32-shaped <see cref="HotkeyGesture"/> to X11 and XDG shortcut terms.</summary>
public static class LinuxKeys
{
    // X11 modifier masks (X.h).
    public const uint ShiftMask = 1, ControlMask = 4, Mod1Mask = 8, Mod4Mask = 64;

    /// <summary>The X keysym for a Win32 virtual key that <see cref="HotkeyGesture"/> accepts; null if there is none.</summary>
    public static uint? ToKeysym(uint virtualKey) => virtualKey switch
    {
        >= 0x41 and <= 0x5A => virtualKey + 0x20,          // A–Z → XK_a–XK_z
        >= 0x30 and <= 0x39 => virtualKey,                 // 0–9 → XK_0–XK_9
        >= 0x70 and <= 0x87 => 0xFFBE + (virtualKey - 0x70), // F1–F24 → XK_F1…
        0x21 => 0xFF55, // PageUp → XK_Prior
        0x22 => 0xFF56, // PageDown → XK_Next
        0x23 => 0xFF57, // End
        0x24 => 0xFF50, // Home
        0x25 => 0xFF51, // Left
        0x26 => 0xFF52, // Up
        0x27 => 0xFF53, // Right
        0x28 => 0xFF54, // Down
        0x2D => 0xFF63, // Insert
        0x2E => 0xFFFF, // Delete
        _ => null,
    };

    /// <summary>The xkb keysym name (as the XDG shortcuts format wants it) for a Win32 virtual key; null if there is none.</summary>
    public static string? ToKeysymName(uint virtualKey) => virtualKey switch
    {
        >= 0x41 and <= 0x5A => ((char)(virtualKey + 0x20)).ToString(),
        >= 0x30 and <= 0x39 => ((char)virtualKey).ToString(),
        >= 0x70 and <= 0x87 => $"F{virtualKey - 0x6F}",
        0x21 => "Page_Up",
        0x22 => "Page_Down",
        0x23 => "End",
        0x24 => "Home",
        0x25 => "Left",
        0x26 => "Up",
        0x27 => "Right",
        0x28 => "Down",
        0x2D => "Insert",
        0x2E => "Delete",
        _ => null,
    };

    /// <summary>Win32 MOD_* bits → X11 modifier mask (Ctrl→Control, Alt→Mod1, Shift→Shift, Win→Mod4).</summary>
    public static uint ToX11Modifiers(uint win32Modifiers)
    {
        uint mask = 0;
        if ((win32Modifiers & HotkeyGesture.ModControl) != 0) mask |= ControlMask;
        if ((win32Modifiers & HotkeyGesture.ModAlt) != 0) mask |= Mod1Mask;
        if ((win32Modifiers & HotkeyGesture.ModShift) != 0) mask |= ShiftMask;
        if ((win32Modifiers & HotkeyGesture.ModWin) != 0) mask |= Mod4Mask;
        return mask;
    }

    /// <summary>The gesture in the XDG shortcuts format used by the GlobalShortcuts portal, e.g. "CTRL+ALT+Up"; null if not expressible.</summary>
    public static string? ToPortalTrigger(HotkeyGesture gesture)
    {
        if (ToKeysymName(gesture.VirtualKey) is not { } key) return null;
        var parts = new List<string>(5);
        if ((gesture.Modifiers & HotkeyGesture.ModControl) != 0) parts.Add("CTRL");
        if ((gesture.Modifiers & HotkeyGesture.ModAlt) != 0) parts.Add("ALT");
        if ((gesture.Modifiers & HotkeyGesture.ModShift) != 0) parts.Add("SHIFT");
        if ((gesture.Modifiers & HotkeyGesture.ModWin) != 0) parts.Add("LOGO");
        parts.Add(key);
        return string.Join('+', parts);
    }

    /// <summary>Human-readable description of an action for the portal's shortcut list.</summary>
    public static string Describe(HotkeyAction action) => action switch
    {
        HotkeyAction.BrightnessUp => "Monitor brightness up",
        HotkeyAction.BrightnessDown => "Monitor brightness down",
        HotkeyAction.ContrastUp => "Monitor contrast up",
        HotkeyAction.ContrastDown => "Monitor contrast down",
        HotkeyAction.DayPreset => "Day preset",
        HotkeyAction.NightPreset => "Night preset",
        HotkeyAction.InputHdmi => "Switch monitor to HDMI",
        HotkeyAction.InputDp => "Switch monitor to DisplayPort",
        HotkeyAction.InputUsbC => "Switch monitor to USB-C",
        HotkeyAction.SwitchToMe => "Switch monitor to this computer",
        HotkeyAction.OpenPanel => "Open ThinkVision Control",
        _ => action.ToString(),
    };
}
