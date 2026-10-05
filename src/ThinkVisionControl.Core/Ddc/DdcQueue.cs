using System.Diagnostics;
using System.Threading.Channels;

namespace ThinkVisionControl.Core.Ddc;

/// <summary>
/// Serialises all DDC traffic: one command at a time, spaced, with retries.
/// After a write the next command waits at least 400 ms (measured settle time on the P32p-20).
/// </summary>
public sealed class DdcQueue : IDisposable
{
    private const int MaxAttempts = 5;
    private static readonly TimeSpan CommandGap = TimeSpan.FromMilliseconds(50);
    private static readonly TimeSpan WriteSettle = TimeSpan.FromMilliseconds(400);
    private static readonly TimeSpan RetryGap = TimeSpan.FromMilliseconds(150);

    private readonly Channel<Func<Task>> _channel = Channel.CreateUnbounded<Func<Task>>(new UnboundedChannelOptions { SingleReader = true });
    private readonly Stopwatch _clock = Stopwatch.StartNew();
    private readonly Task _worker;
    private TimeSpan _readyAt;

    public DdcQueue(IMonitorLink link)
    {
        Link = link;
        _worker = Task.Run(WorkAsync);
    }

    public IMonitorLink Link { get; }

    public Task<T> Run<T>(Func<IMonitorLink, T> op, CancellationToken ct = default)
    {
        var tcs = new TaskCompletionSource<T>(TaskCreationOptions.RunContinuationsAsynchronously);
        if (!_channel.Writer.TryWrite(() => ExecuteAsync(op, tcs, ct)))
            tcs.SetException(new ObjectDisposedException(nameof(DdcQueue)));
        return tcs.Task;
    }

    public void Dispose()
    {
        _channel.Writer.TryComplete();
        _worker.Wait();
    }

    private async Task WorkAsync()
    {
        await foreach (var item in _channel.Reader.ReadAllAsync())
            await item();
    }

    private async Task ExecuteAsync<T>(Func<IMonitorLink, T> op, TaskCompletionSource<T> tcs, CancellationToken ct)
    {
        var link = new RecordingLink(Link);
        try
        {
            var wait = _readyAt - _clock.Elapsed;
            if (wait > TimeSpan.Zero)
                await Task.Delay(wait, ct);

            for (var attempt = 1; ; attempt++)
            {
                ct.ThrowIfCancellationRequested();
                try
                {
                    tcs.SetResult(op(link));
                    return;
                }
                catch (DdcException) when (attempt < MaxAttempts)
                {
                    await Task.Delay(RetryGap, ct);
                }
            }
        }
        catch (OperationCanceledException) when (ct.IsCancellationRequested)
        {
            tcs.TrySetCanceled(ct);
        }
        catch (Exception e)
        {
            tcs.TrySetException(e);
        }
        finally
        {
            var next = _clock.Elapsed + (link.Wrote ? WriteSettle : CommandGap);
            if (next > _readyAt)
                _readyAt = next;
        }
    }

    private sealed class RecordingLink(IMonitorLink inner) : IMonitorLink
    {
        public bool Wrote { get; private set; }

        public bool IsConnected => inner.IsConnected;
        public LinkFailure LastFailure => inner.LastFailure;
        public bool TryConnect() => inner.TryConnect();
        public string ReadCapabilities() => inner.ReadCapabilities();
        public VcpReading Read(byte code) => inner.Read(code);

        public void Write(byte code, uint value)
        {
            Wrote = true;
            inner.Write(code, value);
        }

        public void Dispose() { }
    }
}
