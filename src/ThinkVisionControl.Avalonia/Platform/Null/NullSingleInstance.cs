namespace ThinkVisionControl.Avalonia.Platform.Null;

/// <summary>Every launch is the primary instance; nothing is forwarded.</summary>
public sealed class NullSingleInstance : ISingleInstance
{
    public bool TryBecomePrimary() => true;
    public bool TrySend(IReadOnlyList<string> args) => false;
    public event Action<string[]>? Received { add { } remove { } }
    public void Dispose() { }
}