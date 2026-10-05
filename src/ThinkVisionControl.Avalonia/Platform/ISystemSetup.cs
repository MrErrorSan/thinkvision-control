using ThinkVisionControl.Core.Ddc;

namespace ThinkVisionControl.Avalonia.Platform;

/// <summary>A one-time setup step the user has to do on this OS, shown as a card in App settings.</summary>
/// <param name="Id">Stable key, e.g. "linux-i2c-permission".</param>
/// <param name="CopyText">Exact command(s) to copy, or null when there is nothing to run.</param>
public sealed record SetupHint(string Id, string Title, string Message, string? CopyText);

/// <summary>Detects what's missing on this machine. Called when the window opens and whenever the monitor status changes.</summary>
public interface ISystemSetup
{
    IReadOnlyList<SetupHint> Detect(LinkFailure reason, bool hotkeysAvailable, bool trayHostAvailable);
}
