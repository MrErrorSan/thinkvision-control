using System.IO.Pipes;
using System.Runtime.Versioning;
using Avalonia.Threading;

namespace ThinkVisionControl.Avalonia.Platform.Linux;

/// <summary>
/// One instance per user: an exclusively held lock file in <c>$XDG_RUNTIME_DIR</c> (fallback <c>/tmp</c>, per user)
/// decides the primary; later launches send their arguments, newline-separated, over a .NET named pipe (a Unix
/// domain socket, current user only). The socket lives in <c>$XDG_RUNTIME_DIR/tvc-cmd</c> when that directory exists
/// (private to the user), else .NET's default <c>/tmp/CoreFxPipe_tvc-cmd</c>.
/// </summary>
[SupportedOSPlatform("linux")]
public sealed class UnixSingleInstance : ISingleInstance
{
    const string ShortPipeName = "tvc-cmd";

    readonly string _lockPath;
    readonly string _pipeName;
    readonly Action<Action> _post;
    readonly CancellationTokenSource _cts = new();
    FileStream? _lock;

    /// <param name="runtimeDirectory">Overrides <c>$XDG_RUNTIME_DIR</c> (for tests).</param>
    /// <param name="post">Marshals <see cref="Received"/>; defaults to the Avalonia UI thread.</param>
    public UnixSingleInstance(string? runtimeDirectory = null, Action<Action>? post = null)
    {
        _post = post ?? (a => Dispatcher.UIThread.Post(a));
        var runtime = runtimeDirectory ?? Environment.GetEnvironmentVariable("XDG_RUNTIME_DIR");
        if (!string.IsNullOrEmpty(runtime) && Directory.Exists(runtime))
        {
            _lockPath = Path.Combine(runtime, "thinkvision-control.lock");
            _pipeName = Path.Combine(runtime, ShortPipeName);
        }
        else
        {
            _lockPath = Path.Combine("/tmp", $"thinkvision-control-{Environment.UserName}.lock");
            _pipeName = ShortPipeName;
        }
    }

    public event Action<string[]>? Received;

    public bool TryBecomePrimary()
    {
        if (_lock is not null) return true;
        try
        {
            // FileShare.None takes a non-blocking flock(LOCK_EX) on Unix; the kernel drops it if we crash.
            _lock = new FileStream(_lockPath, FileMode.OpenOrCreate, FileAccess.ReadWrite, FileShare.None);
        }
        catch (IOException) { return false; }
        catch (UnauthorizedAccessException) { return false; }

        _ = Task.Run(() => ListenAsync(_cts.Token));
        return true;
    }

    public bool TrySend(IReadOnlyList<string> args)
    {
        // The primary may still be starting its listener, so give it a few chances.
        for (var attempt = 0; attempt < 3; attempt++)
        {
            try
            {
                using var client = new NamedPipeClientStream(".", _pipeName, PipeDirection.Out, PipeOptions.CurrentUserOnly);
                client.Connect(1000);
                using var writer = new StreamWriter(client);
                writer.Write(string.Join('\n', args));
                return true;
            }
            catch (Exception ex) when (ex is TimeoutException or IOException or UnauthorizedAccessException)
            {
                Thread.Sleep(200);
            }
        }
        return false;
    }

    public void Dispose()
    {
        _cts.Cancel();
        _lock?.Dispose();
        _lock = null;
    }

    async Task ListenAsync(CancellationToken ct)
    {
        while (!ct.IsCancellationRequested)
        {
            try
            {
                await using var server = new NamedPipeServerStream(_pipeName, PipeDirection.In, 1, PipeTransmissionMode.Byte,
                    PipeOptions.Asynchronous | PipeOptions.CurrentUserOnly);
                await server.WaitForConnectionAsync(ct).ConfigureAwait(false);
                using var reader = new StreamReader(server);
                var text = await reader.ReadToEndAsync(ct).ConfigureAwait(false);
                var args = text.Split('\n', StringSplitOptions.RemoveEmptyEntries);
                _post(() => Received?.Invoke(args));
            }
            catch (OperationCanceledException)
            {
                return;
            }
            catch (Exception)
            {
                try { await Task.Delay(500, ct).ConfigureAwait(false); }
                catch (OperationCanceledException) { return; }
            }
        }
    }
}
