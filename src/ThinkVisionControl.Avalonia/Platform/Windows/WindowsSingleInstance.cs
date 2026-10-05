using System.Diagnostics;
using System.IO.Pipes;
using System.Runtime.Versioning;
using ThinkVisionControl.Core.Logging;

namespace ThinkVisionControl.Avalonia.Platform.Windows;

/// <summary>
/// One instance per user session: a named mutex decides the primary, and a named pipe (current user only) carries a
/// later launch's arguments to it, newline-separated. <see cref="Received"/> is raised on a background thread.
/// <para>
/// The names differ from the WPF app's so the two builds can run side by side while the Avalonia one is evaluated.
/// </para>
/// </summary>
[SupportedOSPlatform("windows")]
public sealed class WindowsSingleInstance : ISingleInstance
{
    const string MutexName = "thinkvision-control.SingleInstance";
    static readonly string PipeName = $"thinkvision-control.Commands.{Environment.UserName}.{Process.GetCurrentProcess().SessionId}";

    readonly CancellationTokenSource _cts = new();
    Mutex? _mutex;

    public event Action<string[]>? Received;

    /// <summary>True for the first instance in this session, which then starts listening for forwarded arguments.</summary>
    public bool TryBecomePrimary()
    {
        if (_mutex is not null) return true;
        var mutex = new Mutex(false, MutexName, out var createdNew);
        if (!createdNew)
        {
            mutex.Dispose();
            return false;
        }
        _mutex = mutex;
        _ = Task.Run(() => ListenAsync(_cts.Token));
        return true;
    }

    /// <summary>Sends <paramref name="args"/> to the primary instance. False if none is listening in time.</summary>
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
            catch (Exception ex) when (ex is TimeoutException or IOException)
            {
                Thread.Sleep(200);
            }
        }
        return false;
    }

    public void Dispose()
    {
        _cts.Cancel();
        _mutex?.Dispose();
        _mutex = null;
    }

    async Task ListenAsync(CancellationToken ct)
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
                FileLog.Write($"Command pipe error: {ex.Message}");
                await Task.Delay(500, ct).ContinueWith(_ => { }, TaskScheduler.Default);
            }
        }
    }
}
