using System.IO.Pipes;
using System.Runtime.Versioning;
using ThinkVisionControl.Core.Logging;

namespace ThinkVisionControl.Avalonia.Platform.MacOS;

/// <summary>
/// One instance per user: an exclusive lock on <c>$TMPDIR/thinkvision-control.lock</c> decides the primary
/// (.NET takes <c>flock(LOCK_EX | LOCK_NB)</c> for <see cref="FileShare.None"/> on Unix, released when the process
/// dies). Later launches forward their arguments, newline-separated, over a named pipe — a Unix domain socket at
/// <c>$TMPDIR/CoreFxPipe_tvc-cmd</c>, short enough for macOS's 104-byte socket path limit even under the long
/// per-user <c>/var/folders/…/T/</c>. <see cref="PipeOptions.CurrentUserOnly"/> makes it mode 0600 with a peer UID check.
/// <see cref="Received"/> is raised on a background thread.
/// </summary>
[SupportedOSPlatform("macos")]
public sealed class UnixSingleInstance : ISingleInstance
{
    private const string PipeName = "tvc-cmd";

    private static readonly string LockPath = Path.Combine(Path.GetTempPath(), "thinkvision-control.lock");

    private readonly CancellationTokenSource _cts = new();
    private FileStream? _lock;

    public event Action<string[]>? Received;

    public bool TryBecomePrimary()
    {
        if (_lock is not null) return true;
        try
        {
            _lock = new FileStream(LockPath, FileMode.OpenOrCreate, FileAccess.ReadWrite, FileShare.None);
        }
        catch (IOException)
        {
            return false; // another instance holds the lock
        }
        catch (UnauthorizedAccessException ex)
        {
            // Lock file owned by someone else: don't block startup, run without forwarding.
            FileLog.Write($"Single-instance lock unavailable: {ex.Message}");
            return true;
        }

        // A socket file left by a crashed primary would make bind fail; we hold the lock, so it is stale.
        try { File.Delete(Path.Combine(Path.GetTempPath(), "CoreFxPipe_" + PipeName)); }
        catch (Exception ex) when (ex is IOException or UnauthorizedAccessException) { }

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
                using var client = new NamedPipeClientStream(".", PipeName, PipeDirection.Out, PipeOptions.CurrentUserOnly);
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

    private async Task ListenAsync(CancellationToken ct)
    {
        while (!ct.IsCancellationRequested)
        {
            try
            {
                await using var server = new NamedPipeServerStream(PipeName, PipeDirection.In, 1, PipeTransmissionMode.Byte,
                    PipeOptions.Asynchronous | PipeOptions.CurrentUserOnly);
                await server.WaitForConnectionAsync(ct);
                using var reader = new StreamReader(server);
                var text = await reader.ReadToEndAsync(ct);
                Received?.Invoke(text.Split('\n', StringSplitOptions.RemoveEmptyEntries));
            }
            catch (OperationCanceledException)
            {
                return;
            }
            catch (Exception ex)
            {
                FileLog.Write($"Command socket error: {ex.Message}");
                await Task.Delay(500, ct).ContinueWith(_ => { }, TaskScheduler.Default);
            }
        }
    }
}
