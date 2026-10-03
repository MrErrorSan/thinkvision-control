namespace ThinkVisionControl.Core.Control;

/// <summary>Timestamped <c>backup-yyyyMMdd-HHmmss.json</c> files of code → value.</summary>
public sealed class BackupStore(string directory)
{
    /// <returns>The path written.</returns>
    public string Save(IReadOnlyDictionary<byte, uint> values) => throw new NotImplementedException();

    /// <returns>The newest backup, or null if none exists.</returns>
    public IReadOnlyDictionary<byte, uint>? LoadLatest() => throw new NotImplementedException();
}
