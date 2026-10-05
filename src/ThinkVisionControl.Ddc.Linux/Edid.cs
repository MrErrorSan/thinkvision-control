namespace ThinkVisionControl.Ddc.Linux;

/// <summary>EDID base-block identification. Pure: no I/O.</summary>
public static class Edid
{
    static ReadOnlySpan<byte> Header => [0x00, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0x00];

    /// <summary>
    /// True when <paramref name="edid"/> starts with a valid EDID header and carries the given PNP manufacturer id
    /// (bytes 8–9, big-endian, three 5-bit letters, 'A' = 1) and product code (bytes 10–11, little-endian).
    /// LEN / 0x62A2 is <c>30 AE A2 62</c> in bytes 8–11.
    /// </summary>
    public static bool Matches(ReadOnlySpan<byte> edid, string manufacturer, ushort product)
    {
        if (edid.Length < 12 || !edid[..8].SequenceEqual(Header)) return false;
        return string.Equals(Manufacturer(edid), manufacturer, StringComparison.OrdinalIgnoreCase)
            && (ushort)(edid[10] | edid[11] << 8) == product;
    }

    /// <summary>The three-letter PNP id from bytes 8–9, or "" if a letter is out of range.</summary>
    public static string Manufacturer(ReadOnlySpan<byte> edid)
    {
        if (edid.Length < 10) return "";
        var packed = edid[8] << 8 | edid[9];
        Span<char> letters = stackalloc char[3];
        for (var i = 0; i < 3; i++)
        {
            var value = packed >> (10 - 5 * i) & 0x1F;
            if (value is < 1 or > 26) return "";
            letters[i] = (char)('A' + value - 1);
        }
        return new string(letters);
    }
}
