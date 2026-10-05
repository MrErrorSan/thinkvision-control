namespace ThinkVisionControl.Core.Profile;

public static class CapabilitiesParser
{
    /// <exception cref="FormatException">The string has no parsable vcp(...) section.</exception>
    public static MonitorCapabilities Parse(string capabilities)
    {
        var vcp = FindSection(capabilities, "vcp") ?? throw new FormatException("Capability string has no vcp(...) section.");
        var model = FindSection(capabilities, "model")?.Trim() ?? "";
        return new MonitorCapabilities(model, ParseVcp(vcp));
    }

    /// <summary>Contents of the first <c>name(...)</c> at the top level (depth 0 or inside the outer wrapper).</summary>
    private static string? FindSection(string s, string name)
    {
        var token = name + "(";
        var depth = 0;
        for (var i = 0; i < s.Length; i++)
        {
            if (depth <= 1 && string.CompareOrdinal(s, i, token, 0, token.Length) == 0 && (i == 0 || !char.IsAsciiLetterOrDigit(s[i - 1])))
            {
                var open = i + name.Length;
                return s[(open + 1)..MatchingClose(s, open)];
            }
            if (s[i] == '(') depth++;
            else if (s[i] == ')') depth--;
        }
        return null;
    }

    private static int MatchingClose(string s, int open)
    {
        var depth = 0;
        for (var i = open; i < s.Length; i++)
        {
            if (s[i] == '(') depth++;
            else if (s[i] == ')' && --depth == 0) return i;
        }
        throw new FormatException($"Unbalanced parentheses at position {open}.");
    }

    private static Dictionary<byte, IReadOnlyList<byte>?> ParseVcp(string vcp)
    {
        var result = new Dictionary<byte, IReadOnlyList<byte>?>();
        var i = 0;
        while (true)
        {
            SkipSpaces(vcp, ref i);
            if (i >= vcp.Length) return result;
            var code = ReadHexByte(vcp, ref i);
            SkipSpaces(vcp, ref i);
            if (i < vcp.Length && vcp[i] == '(')
            {
                var close = MatchingClose(vcp, i);
                result[code] = ParseValues(vcp[(i + 1)..close]);
                i = close + 1;
            }
            else
            {
                result[code] = null;
            }
        }
    }

    private static List<byte> ParseValues(string s)
    {
        var values = new List<byte>();
        var i = 0;
        while (true)
        {
            SkipSpaces(s, ref i);
            if (i >= s.Length) return values;
            values.Add(ReadHexByte(s, ref i));
        }
    }

    private static void SkipSpaces(string s, ref int i)
    {
        while (i < s.Length && char.IsWhiteSpace(s[i])) i++;
    }

    private static byte ReadHexByte(string s, ref int i)
    {
        if (i + 1 >= s.Length || !char.IsAsciiHexDigit(s[i]) || !char.IsAsciiHexDigit(s[i + 1]))
            throw new FormatException($"Expected a two-digit hex value at position {i} in the vcp section.");
        var value = Convert.ToByte(s.Substring(i, 2), 16);
        i += 2;
        return value;
    }
}
