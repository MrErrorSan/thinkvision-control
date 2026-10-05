using System.Globalization;
using System.Text.Json;
using System.Text.RegularExpressions;

namespace ThinkVisionControl.Core.Control;

/// <summary>Timestamped <c>backup-yyyyMMdd-HHmmss.json</c> files of code → value.</summary>
public sealed partial class BackupStore(string directory)
{
    private static readonly JsonSerializerOptions JsonOptions = new() { WriteIndented = true };

    /// <returns>The path written.</returns>
    public string Save(IReadOnlyDictionary<byte, uint> values)
    {
        Directory.CreateDirectory(directory);
        var stem = $"backup-{DateTime.Now.ToString("yyyyMMdd-HHmmss", CultureInfo.InvariantCulture)}";
        var path = Path.Combine(directory, stem + ".json");
        for (var n = 1; File.Exists(path); n++)
            path = Path.Combine(directory, $"{stem}-{n}.json");

        var json = values.OrderBy(kv => kv.Key).ToDictionary(kv => $"0x{kv.Key:X2}", kv => kv.Value);
        File.WriteAllText(path, JsonSerializer.Serialize(json, JsonOptions));
        return path;
    }

    /// <returns>The newest backup, or null if none exists.</returns>
    public IReadOnlyDictionary<byte, uint>? LoadLatest()
    {
        if (!Directory.Exists(directory)) return null;

        var latest = Directory.EnumerateFiles(directory, "backup-*.json")
            .Select(path => (path, match: FileNamePattern().Match(Path.GetFileName(path))))
            .Where(f => f.match.Success)
            .OrderByDescending(f => f.match.Groups[1].Value, StringComparer.Ordinal)
            .ThenByDescending(f => f.match.Groups[2].Success ? int.Parse(f.match.Groups[2].Value, CultureInfo.InvariantCulture) : 0)
            .Select(f => f.path)
            .FirstOrDefault();
        if (latest is null) return null;

        var json = JsonSerializer.Deserialize<Dictionary<string, uint>>(File.ReadAllText(latest)) ?? [];
        return json.ToDictionary(kv => Convert.ToByte(kv.Key, 16), kv => kv.Value);
    }

    [GeneratedRegex(@"^backup-(\d{8}-\d{6})(?:-(\d+))?\.json$")]
    private static partial Regex FileNamePattern();
}
