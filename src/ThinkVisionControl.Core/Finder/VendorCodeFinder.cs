using ThinkVisionControl.Core.Control;

namespace ThinkVisionControl.Core.Finder;

public sealed record CodeChange(byte Code, uint Before, uint After);

/// <summary>Read-only discovery of what Lenovo vendor codes control.</summary>
public static class VendorCodeFinder
{
    /// <summary>Reads the current value of every Hidden code (fresh, not cached).</summary>
    public static Task<IReadOnlyDictionary<byte, uint>> SnapshotAsync(ControlService control) => throw new NotImplementedException();

    public static IReadOnlyList<CodeChange> Diff(IReadOnlyDictionary<byte, uint> before, IReadOnlyDictionary<byte, uint> after) =>
        throw new NotImplementedException();
}
