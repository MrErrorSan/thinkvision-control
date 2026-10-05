using System.Globalization;

namespace ThinkVisionControl.Core.Settings;

/// <summary>A global hotkey as Win32 <c>RegisterHotKey</c> wants it. Text form: <c>"Ctrl+Alt+PageUp"</c>.</summary>
public readonly record struct HotkeyGesture(uint Modifiers, uint VirtualKey)
{
    public const uint ModAlt = 0x1, ModControl = 0x2, ModShift = 0x4, ModWin = 0x8;

    private static readonly Dictionary<string, uint> ModifierNames = new(StringComparer.OrdinalIgnoreCase)
    {
        ["Ctrl"] = ModControl, ["Control"] = ModControl, ["Alt"] = ModAlt, ["Shift"] = ModShift, ["Win"] = ModWin,
    };

    private static readonly Dictionary<string, uint> NamedKeys = new(StringComparer.OrdinalIgnoreCase)
    {
        ["PageUp"] = 0x21, ["PgUp"] = 0x21, ["PageDown"] = 0x22, ["PgDn"] = 0x22, ["End"] = 0x23, ["Home"] = 0x24,
        ["Left"] = 0x25, ["Up"] = 0x26, ["Right"] = 0x27, ["Down"] = 0x28, ["Insert"] = 0x2D, ["Delete"] = 0x2E,
    };

    private static readonly Dictionary<uint, string> CanonicalNames = new()
    {
        [0x21] = "PageUp", [0x22] = "PageDown", [0x23] = "End", [0x24] = "Home",
        [0x25] = "Left", [0x26] = "Up", [0x27] = "Right", [0x28] = "Down", [0x2D] = "Insert", [0x2E] = "Delete",
    };

    public static bool TryParse(string text, out HotkeyGesture gesture)
    {
        gesture = default;
        uint modifiers = 0;
        uint? key = null;
        foreach (var raw in text.Split('+'))
        {
            var part = raw.Trim();
            if (ModifierNames.TryGetValue(part, out var mod)) modifiers |= mod;
            else if (key is null && TryParseKey(part, out var vk)) key = vk;
            else return false;
        }
        if (modifiers == 0 || key is null) return false;
        gesture = new HotkeyGesture(modifiers, key.Value);
        return true;
    }

    public static bool TryCreate(uint modifiers, uint virtualKey, out HotkeyGesture gesture)
    {
        gesture = default;
        const uint allModifiers = ModAlt | ModControl | ModShift | ModWin;
        if (modifiers == 0 || (modifiers & ~allModifiers) != 0 || !IsSupportedKey(virtualKey))
            return false;

        gesture = new HotkeyGesture(modifiers, virtualKey);
        return true;
    }

    private static bool IsSupportedKey(uint key) =>
        key is >= 0x41 and <= 0x5A
            or >= 0x30 and <= 0x39
            or >= 0x70 and <= 0x87
            || CanonicalNames.ContainsKey(key);

    private static bool TryParseKey(string name, out uint vk)
    {
        if (NamedKeys.TryGetValue(name, out vk)) return true;
        if (name.Length == 1 && char.IsAsciiLetterOrDigit(name[0]))
        {
            vk = char.ToUpperInvariant(name[0]);
            return true;
        }
        if (name.Length is 2 or 3 && name[0] is 'F' or 'f' && name[1] != '0'
            && uint.TryParse(name.AsSpan(1), NumberStyles.None, CultureInfo.InvariantCulture, out var n) && n is >= 1 and <= 24)
        {
            vk = 0x6F + n;
            return true;
        }
        return false;
    }

    public override string ToString()
    {
        var parts = new List<string>(5);
        if ((Modifiers & ModControl) != 0) parts.Add("Ctrl");
        if ((Modifiers & ModAlt) != 0) parts.Add("Alt");
        if ((Modifiers & ModShift) != 0) parts.Add("Shift");
        if ((Modifiers & ModWin) != 0) parts.Add("Win");
        parts.Add(VirtualKey switch
        {
            >= 0x41 and <= 0x5A or >= 0x30 and <= 0x39 => ((char)VirtualKey).ToString(),
            >= 0x70 and <= 0x87 => $"F{VirtualKey - 0x6F}",
            _ => CanonicalNames.TryGetValue(VirtualKey, out var name) ? name : $"0x{VirtualKey:X2}",
        });
        return string.Join('+', parts);
    }
}
