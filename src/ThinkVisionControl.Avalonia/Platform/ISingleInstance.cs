namespace ThinkVisionControl.Avalonia.Platform;

/// <summary>One running instance; later launches forward their arguments to it.</summary>
public interface ISingleInstance : IDisposable
{
    bool TryBecomePrimary();
    bool TrySend(IReadOnlyList<string> args);
    event Action<string[]>? Received;
}