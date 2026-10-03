namespace ThinkVisionControl.Core.Ddc;

/// <summary>
/// Serialises all DDC traffic: one command at a time, spaced, with retries.
/// After a write the next command waits at least 400 ms (measured settle time on the P32p-20).
/// </summary>
public sealed class DdcQueue(IMonitorLink link) : IDisposable
{
    public IMonitorLink Link { get; } = link;

    public Task<T> Run<T>(Func<IMonitorLink, T> op, CancellationToken ct = default) => throw new NotImplementedException();

    public void Dispose() => throw new NotImplementedException();
}
