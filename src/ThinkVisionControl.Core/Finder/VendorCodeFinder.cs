using ThinkVisionControl.Core.Control;
using ThinkVisionControl.Core.Profile;

namespace ThinkVisionControl.Core.Finder;

public sealed record CodeChange(byte Code, uint Before, uint After);

/// <summary>Read-only discovery of what Lenovo vendor codes control.</summary>
public static class VendorCodeFinder
{
    /// <summary>Reads the current value of every Hidden code (fresh, not cached).</summary>
    public static async Task<IReadOnlyDictionary<byte, uint>> SnapshotAsync(ControlService control)
    {
        var codes = control.Controls.Values
            .Where(d => d.Section == PanelSection.Hidden && d.Safety != SafetyClass.NeverWrite)
            .Select(d => d.Code)
            .ToList();
        await control.RefreshAsync(codes);

        var snapshot = new Dictionary<byte, uint>();
        foreach (var code in codes)
            if (control.TryGetCached(code, out var reading))
                snapshot[code] = reading.Current;
        return snapshot;
    }

    public static IReadOnlyList<CodeChange> Diff(IReadOnlyDictionary<byte, uint> before, IReadOnlyDictionary<byte, uint> after) =>
        before.Where(kv => after.TryGetValue(kv.Key, out var value) && value != kv.Value)
            .OrderBy(kv => kv.Key)
            .Select(kv => new CodeChange(kv.Key, kv.Value, after[kv.Key]))
            .ToList();
}
