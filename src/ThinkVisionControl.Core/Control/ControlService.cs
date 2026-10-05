using ThinkVisionControl.Core.Ddc;
using ThinkVisionControl.Core.Logging;
using ThinkVisionControl.Core.Profile;
using ThinkVisionControl.Core.Settings;

namespace ThinkVisionControl.Core.Control;

public enum WriteOutcome { Ok, NeedsConfirmation, Blocked, NotResponding }

public sealed record WriteResult(WriteOutcome Outcome, string Message, uint? Value = null);

/// <summary>The only write path to the monitor. Owns the value cache and enforces <see cref="SafetyPolicy"/>.</summary>
public sealed class ControlService(DdcQueue queue, ProfileStore profiles, BackupStore backups) : IDisposable
{
    private const byte ColourMode = 0x14, Brightness = 0x10, Contrast = 0x12;
    private static readonly TimeSpan ReconnectInterval = TimeSpan.FromSeconds(5);
    private static readonly WriteResult NotRespondingResult = new(WriteOutcome.NotResponding, "Monitor not responding");

    private readonly Lock gate = new();
    private readonly Dictionary<byte, VcpReading> cache = [];
    private Timer? reconnectTimer;
    private int reconnecting;
    private bool responding;
    private bool disposed;

    public event Action<byte, uint>? ValueChanged;
    public event Action<bool>? RespondingChanged;

    public bool IsResponding { get { lock (gate) return responding; } }
    public MonitorCapabilities? Capabilities { get; private set; }
    public IReadOnlyDictionary<byte, ControlDefinition> Controls { get; private set; } = new Dictionary<byte, ControlDefinition>();

    public bool TryGetCached(byte code, out VcpReading reading)
    {
        lock (gate) return cache.TryGetValue(code, out reading);
    }

    /// <summary>Connects, reads capabilities and initial values of all writable controls.</summary>
    public Task StartAsync() => ReconnectAsync();

    public async Task ReconnectAsync()
    {
        try
        {
            if (!await queue.Run(l => l.TryConnect()))
            {
                SetResponding(false);
                return;
            }

            if (Capabilities is null)
            {
                var caps = CapabilitiesParser.Parse(await queue.Run(l => l.ReadCapabilities()));
                Controls = profiles.Resolve(caps);
                Capabilities = caps;
            }

            List<byte> cached;
            lock (gate) cached = [.. cache.Keys];
            var codes = Controls.Values
                .Where(d => d.Kind is ControlKind.Range or ControlKind.Enum && d.Section is PanelSection.Main or PanelSection.Advanced)
                .Select(d => d.Code)
                .Union(cached);
            if (await RefreshCoreAsync(codes)) SetResponding(true);
        }
        catch (Exception ex) when (ex is DdcException or FormatException)
        {
            FileLog.Write($"Connect failed: {ex.Message}");
            SetResponding(false);
        }
    }

    public Task RefreshAsync(IEnumerable<byte> codes) => RefreshCoreAsync(codes);

    /// <returns>False if every requested code failed.</returns>
    private async Task<bool> RefreshCoreAsync(IEnumerable<byte> codes)
    {
        int requested = 0, succeeded = 0;
        foreach (var code in codes.ToList())
        {
            requested++;
            try
            {
                var reading = await queue.Run(l => l.Read(code));
                succeeded++;
                UpdateCache(code, reading);
            }
            catch (DdcException ex)
            {
                FileLog.Write($"Read 0x{code:X2} failed: {ex.Message}");
            }
        }

        if (requested > 0 && succeeded == 0)
        {
            SetResponding(false);
            return false;
        }
        return true;
    }

    public async Task<WriteResult> SetAsync(byte code, uint value, bool confirmed = false)
    {
        var caps = Capabilities;
        if (caps is null) return NotRespondingResult;

        Controls.TryGetValue(code, out var def);
        var hasCached = TryGetCached(code, out var cached);
        uint? max = def?.Kind == ControlKind.Range && hasCached ? cached.Max : null;
        var decision = SafetyPolicy.Evaluate(def, caps, code, value, max, confirmed);
        if (decision.Verdict == SafetyVerdict.Block) return new(WriteOutcome.Blocked, decision.Message);
        if (decision.Verdict == SafetyVerdict.NeedsConfirmation) return new(WriteOutcome.NeedsConfirmation, decision.Message);

        var v = decision.Value;
        if (def!.Kind == ControlKind.Action)
        {
            try
            {
                backups.Save(BackupValues());
            }
            catch (Exception ex) when (ex is IOException or UnauthorizedAccessException)
            {
                FileLog.Write($"Backup failed: {ex.Message}");
                return new(WriteOutcome.Blocked, $"Backup failed — {def.Name} cancelled.");
            }
        }

        try
        {
            await queue.Run(l => { l.Write(code, v); return 0; });
        }
        catch (DdcException ex)
        {
            FileLog.Write($"Write 0x{code:X2}={v} failed: {ex.Message}");
            SetResponding(false);
            return NotRespondingResult;
        }

        if (def.Kind == ControlKind.Range)
        {
            try
            {
                var actual = await queue.Run(l => l.Read(code));
                UpdateCache(code, actual);
                if (code == Brightness)
                    FileLog.Write($"Brightness DDC write verified: requested={v}, readback={actual.Current}, max={actual.Max}.");
                if (actual.Current != v)
                {
                    var detail = code == Brightness ? " The current color mode may lock brightness." : "";
                    return new(WriteOutcome.Blocked,
                        $"{def.Name} remains {actual.Current}; the monitor did not apply {v}.{detail}");
                }
            }
            catch (DdcException ex)
            {
                FileLog.Write($"Verify read 0x{code:X2} failed: {ex.Message}");
                SetResponding(false);
                return NotRespondingResult;
            }
        }
        else if (def.Kind != ControlKind.Action)
        {
            UpdateCache(code, new VcpReading(v, hasCached ? cached.Max : 0u));
        }

        return new(WriteOutcome.Ok, Describe(def, v), v);
    }

    /// <summary>Adds <paramref name="delta"/> to the cached value, clamped to 0..Max.</summary>
    public async Task<WriteResult> StepAsync(byte code, int delta)
    {
        if (!TryGetCached(code, out var reading))
        {
            await RefreshAsync([code]);
            if (!TryGetCached(code, out reading))
                return IsResponding ? new(WriteOutcome.Blocked, $"Could not read 0x{code:X2}.") : NotRespondingResult;
        }

        return await SetAsync(code, (uint)Math.Clamp((long)reading.Current + delta, 0, reading.Max));
    }

    public async Task<WriteResult> ApplyPresetAsync(Preset preset)
    {
        if (preset.ColourMode is byte mode)
        {
            var colour = await SetAsync(ColourMode, mode);
            if (colour.Outcome != WriteOutcome.Ok) return colour;
        }

        var brightness = await SetAsync(Brightness, preset.Brightness);
        if (brightness.Outcome != WriteOutcome.Ok) return brightness;

        var contrast = await SetAsync(Contrast, preset.Contrast);
        if (contrast.Outcome != WriteOutcome.Ok) return contrast;

        return new(WriteOutcome.Ok, $"{preset.Name} preset");
    }

    public async Task<WriteResult> RestoreLatestBackupAsync()
    {
        var backup = backups.LoadLatest();
        if (backup is null) return new(WriteOutcome.Blocked, "No backup found.");

        foreach (var (code, value) in backup)
        {
            var result = await SetAsync(code, value);
            if (result.Outcome != WriteOutcome.Ok) return result;
        }
        return new(WriteOutcome.Ok, "Backup restored");
    }

    /// <summary>Re-resolves <see cref="Controls"/> after the user overlay changed (finder naming).</summary>
    public void ReloadProfile()
    {
        if (Capabilities is { } caps) Controls = profiles.Resolve(caps);
    }

    public void Dispose()
    {
        lock (gate)
        {
            if (disposed) return;
            disposed = true;
            reconnectTimer?.Dispose();
            reconnectTimer = null;
        }
        queue.Dispose();
    }

    private Dictionary<byte, uint> BackupValues()
    {
        var controls = Controls;
        lock (gate)
        {
            return cache
                .Where(kv => controls.TryGetValue(kv.Key, out var d)
                    && d.Kind is ControlKind.Range or ControlKind.Enum && d.Safety == SafetyClass.Normal)
                .ToDictionary(kv => kv.Key, kv => kv.Value.Current);
        }
    }

    private void UpdateCache(byte code, VcpReading reading)
    {
        bool changed;
        lock (gate)
        {
            changed = !cache.TryGetValue(code, out var old) || old.Current != reading.Current;
            cache[code] = reading;
        }
        if (changed) ValueChanged?.Invoke(code, reading.Current);
    }

    private void SetResponding(bool value)
    {
        bool changed;
        lock (gate)
        {
            if (disposed) return;
            changed = responding != value;
            responding = value;
            if (value)
            {
                reconnectTimer?.Dispose();
                reconnectTimer = null;
            }
            else
            {
                reconnectTimer ??= new Timer(_ => _ = ReconnectTickAsync(), null, ReconnectInterval, ReconnectInterval);
            }
        }
        if (changed) RespondingChanged?.Invoke(value);
    }

    private async Task ReconnectTickAsync()
    {
        if (Interlocked.Exchange(ref reconnecting, 1) == 1) return;
        try
        {
            await ReconnectAsync();
        }
        catch (Exception ex)
        {
            FileLog.Write($"Reconnect failed: {ex.Message}");
        }
        finally
        {
            Volatile.Write(ref reconnecting, 0);
        }
    }

    private static string Describe(ControlDefinition def, uint value) => def.Kind switch
    {
        ControlKind.Range => $"{def.Name} {value}",
        ControlKind.Enum => $"{def.Name} → {(value <= byte.MaxValue && def.ValueLabels.TryGetValue((byte)value, out var label) ? label : $"0x{value:X2}")}",
        _ => def.Name,
    };
}
