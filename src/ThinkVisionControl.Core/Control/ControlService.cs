using ThinkVisionControl.Core.Ddc;
using ThinkVisionControl.Core.Profile;
using ThinkVisionControl.Core.Settings;

namespace ThinkVisionControl.Core.Control;

public enum WriteOutcome { Ok, NeedsConfirmation, Blocked, NotResponding }

public sealed record WriteResult(WriteOutcome Outcome, string Message, uint? Value = null);

/// <summary>The only write path to the monitor. Owns the value cache and enforces <see cref="SafetyPolicy"/>.</summary>
public sealed class ControlService(DdcQueue queue, ProfileStore profiles, BackupStore backups) : IDisposable
{
    public event Action<byte, uint>? ValueChanged;
    public event Action<bool>? RespondingChanged;

    public bool IsResponding => throw new NotImplementedException();
    public MonitorCapabilities? Capabilities => throw new NotImplementedException();
    public IReadOnlyDictionary<byte, ControlDefinition> Controls => throw new NotImplementedException();

    public bool TryGetCached(byte code, out VcpReading reading) => throw new NotImplementedException();

    /// <summary>Connects, reads capabilities and initial values of all writable controls.</summary>
    public Task StartAsync() => throw new NotImplementedException();
    public Task ReconnectAsync() => throw new NotImplementedException();
    public Task RefreshAsync(IEnumerable<byte> codes) => throw new NotImplementedException();

    public Task<WriteResult> SetAsync(byte code, uint value, bool confirmed = false) => throw new NotImplementedException();
    /// <summary>Adds <paramref name="delta"/> to the cached value, clamped to 0..Max.</summary>
    public Task<WriteResult> StepAsync(byte code, int delta) => throw new NotImplementedException();
    /// <summary>Slider path: latest value wins, at most ~10 writes per second.</summary>
    public void SetThrottled(byte code, uint value) => throw new NotImplementedException();
    public Task<WriteResult> ApplyPresetAsync(Preset preset) => throw new NotImplementedException();
    public Task<WriteResult> RestoreLatestBackupAsync() => throw new NotImplementedException();
    /// <summary>Re-resolves <see cref="Controls"/> after the user overlay changed (finder naming).</summary>
    public void ReloadProfile() => throw new NotImplementedException();

    public void Dispose() => throw new NotImplementedException();
}
