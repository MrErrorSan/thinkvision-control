using System.Diagnostics;
using System.Threading.Channels;

namespace ThinkVisionControl.Core.Ddc;

/// <summary>
/// Serialises all DDC traffic: one command at a time, 50 ms apart, with retries.
/// A read waits until 400 ms after the last write: measured on the P32p-20, reads fail while a change settles,
/// but back-to-back writes 50 ms apart all apply, so writes (e.g. a slider drag) are not held back.
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
    private TimeSpan _lastWriteAt = TimeSpan.MinValue;

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

    /// <summary>True if a write finished less than <paramref name="window"/> ago.</summary>
    public bool WroteWithin(TimeSpan window) => _lastWriteAt != TimeSpan.MinValue && _clock.Elapsed - _lastWriteAt < window;

    /// <summary>Lets the command in flight finish, but waits at most <see cref="DisposeWait"/> so exit never hangs on the monitor.</summary>
    public void Dispose()
    {
        _channel.Writer.TryComplete();
        _worker.Wait(DisposeWait);
    }

    internal static readonly TimeSpan DisposeWait = TimeSpan.FromSeconds(1);

    private async Task WorkAsync()
    {
        await foreach (var item in _channel.Reader.ReadAllAsync())
            await item();
    }

    private async Task ExecuteAsync<T>(Func<IMonitorLink, T> op, TaskCompletionSource<T> tcs, CancellationToken ct)
    {
        var link = new RecordingLink(this);
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
            if (link.Wrote) _lastWriteAt = _clock.Elapsed;
            var next = _clock.Elapsed + CommandGap;
            if (next > _readyAt)
                _readyAt = next;
        }
    }

    private sealed class RecordingLink(DdcQueue queue) : IMonitorLink
    {
        readonly IMonitorLink inner = queue.Link;

        public bool Wrote { get; private set; }

        public bool IsConnected => inner.IsConnected;
        public LinkFailure LastFailure => inner.LastFailure;
        public bool TryConnect() => inner.TryConnect();

        public string ReadCapabilities()
        {
            SettleAfterWrite();
            return inner.ReadCapabilities();
        }

        public VcpReading Read(byte code)
        {
            SettleAfterWrite();
            return inner.Read(code);
        }

        public void Write(byte code, uint value)
        {
            Wrote = true;
            queue._lastWriteAt = queue._clock.Elapsed;
            inner.Write(code, value);
        }

        // Runs on the queue's worker, inside the op, so blocking here just delays this one command.
        void SettleAfterWrite()
        {
            if (queue._lastWriteAt == TimeSpan.MinValue) return;
            var wait = queue._lastWriteAt + WriteSettle - queue._clock.Elapsed;
            if (wait > TimeSpan.Zero) Thread.Sleep(wait);
        }

        public void Dispose() { }
    }
}
