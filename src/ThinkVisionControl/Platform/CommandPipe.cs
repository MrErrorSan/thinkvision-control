using System.Diagnostics;
using System.IO;
using System.IO.Pipes;
using ThinkVisionControl.Core.Logging;

namespace ThinkVisionControl.Platform;

/// <summary>
/// Forwards a later launch's arguments (jump list, Start menu, command line) to the running instance.
/// The pipe is per user and session, and only the current user can connect.
/// </summary>
public sealed class CommandPipe : IDisposable
{
    static readonly string PipeName = $"ThinkVisionControl.Commands.{Environment.UserName}.{Process.GetCurrentProcess().SessionId}";

    readonly CancellationTokenSource _cts = new();

    /// <param name="received">Called on a background thread with the forwarded arguments.</param>
    public CommandPipe(Action<string[]> received) => _ = Task.Run(() => ListenAsync(received, _cts.Token));

    /// <summary>Sends <paramref name="args"/> to the running instance. False if none is listening in time.</summary>
    public static bool TrySend(IReadOnlyList<string> args)
    {
        // The first instance may still be starting its listener, so give it a few chances.
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

    public void Dispose() => _cts.Cancel();

    static async Task ListenAsync(Action<string[]> received, CancellationToken ct)
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
                received(text.Split('\n', StringSplitOptions.RemoveEmptyEntries));
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
