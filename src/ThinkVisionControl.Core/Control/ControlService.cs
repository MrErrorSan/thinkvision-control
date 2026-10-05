using ThinkVisionControl.Core.Ddc;
using ThinkVisionControl.Core.Logging;
using ThinkVisionControl.Core.Profile;
using ThinkVisionControl.Core.Settings;

namespace ThinkVisionControl.Core.Control;

public enum WriteOutcome { Ok, NeedsConfirmation, Blocked, NotResponding }

/// <summary>Connecting = first attempt in progress; NotFound = monitor not plugged in; NotResponding = found but DDC/CI silent.</summary>
public enum MonitorStatus { Connecting, NotFound, NotResponding, Connected }

public sealed record WriteResult(WriteOutcome Outcome, string Message, uint? Value = null);

/// <summary>The only write path to the monitor. Owns the value cache and enforces <see cref="SafetyPolicy"/>.</summary>
public sealed class ControlService : IDisposable
{
    private const byte ColourMode = 0x14, Brightness = 0x10, Contrast = 0x12;
    private static readonly WriteResult NotRespondingResult = new(WriteOutcome.NotResponding, "Monitor not responding");

    private readonly DdcQueue queue;
    private readonly ProfileStore profiles;
    private readonly BackupStore backups;
    private readonly CapabilitiesCache capabilitiesCache;
    private readonly Lock gate = new();
    private readonly Dictionary<byte, VcpReading> cache = [];
    private readonly Dictionary<byte, Drag> drags = [];
    private CancellationTokenSource? burst;
    private string? cachedCapabilities;
    private bool liveCapabilities;
    private MonitorStatus status = MonitorStatus.Connecting;
    private LinkFailure reason;
    private bool disposed;

    public ControlService(DdcQueue queue, ProfileStore profiles, BackupStore backups, CapabilitiesCache cache)
    {
        this.queue = queue;
        this.profiles = profiles;
        this.backups = backups;
        capabilitiesCache = cache;
        cachedCapabilities = cache.Load();
        Capabilities = TryParse(cachedCapabilities) ?? profiles.SynthesizeCapabilities();
        Controls = profiles.Resolve(Capabilities);
    }

    public event Action<byte, uint>? ValueChanged;
    public event Action<bool>? RespondingChanged;
    public event Action<MonitorStatus>? StatusChanged;

    public MonitorStatus Status { get { lock (gate) return status; } }

    /// <summary>Why the monitor isn't connected, when the link knows (set together with <see cref="Status"/>).</summary>
    public LinkFailure StatusReason { get { lock (gate) return reason; } }
    public bool IsResponding => Status == MonitorStatus.Connected;
    public MonitorCapabilities Capabilities { get; private set; }
    public IReadOnlyDictionary<byte, ControlDefinition> Controls { get; private set; }

    internal Func<TimeSpan, CancellationToken, Task> Delay { get; set; } = Task.Delay;

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
                SetStatus(MonitorStatus.NotFound);
                return;
            }

            if (!liveCapabilities)
            {
                var raw = await queue.Run(l => l.ReadCapabilities());
                var caps = CapabilitiesParser.Parse(raw);
                Controls = profiles.Resolve(caps);
                Capabilities = caps;
                liveCapabilities = true;
                if (raw != cachedCapabilities)
                {
                    capabilitiesCache.Save(raw);
                    cachedCapabilities = raw;
                }
            }

            List<byte> cached;
            lock (gate) cached = [.. cache.Keys];
            var codes = Controls.Values
                .Where(d => d.Kind is ControlKind.Range or ControlKind.Enum && d.Section is PanelSection.Main or PanelSection.Advanced)
                .Select(d => d.Code)
                .Union(cached);
            if (await RefreshCoreAsync(codes, recover: false)) SetStatus(MonitorStatus.Connected);
        }
        catch (Exception ex) when (ex is DdcException or FormatException)
        {
            FileLog.Write($"Connect failed: {ex.Message}");
            SetStatus(MonitorStatus.NotResponding);
        }
    }

    /// <summary>Starts (or restarts) a bounded retry burst over <see cref="RetryBurst.Delays"/>; no-op while connected.</summary>
    public void RequestReconnect()
    {
        CancellationTokenSource cts;
        lock (gate)
        {
            if (disposed || status == MonitorStatus.Connected) return;
            burst?.Cancel();
            burst = cts = new CancellationTokenSource();
        }
        _ = RunBurstAsync(cts.Token);
    }

    private async Task RunBurstAsync(CancellationToken ct)
    {
        try
        {
            foreach (var delay in RetryBurst.Delays)
            {
                await Delay(delay, ct);
                ct.ThrowIfCancellationRequested();
                await ReconnectAsync();
                if (Status == MonitorStatus.Connected) return;
            }
        }
        catch (OperationCanceledException) when (ct.IsCancellationRequested)
        {
        }
        catch (Exception ex)
        {
            FileLog.Write($"Reconnect failed: {ex.Message}");
        }
    }

    /// <summary>Re-reads the given codes. Does nothing unless connected, so an absent monitor costs no DDC traffic.</summary>
    public Task RefreshAsync(IEnumerable<byte> codes) =>
        Status == MonitorStatus.Connected ? RefreshCoreAsync(codes, recover: true) : Task.CompletedTask;

    /// <returns>False if every requested code failed.</returns>
    private async Task<bool> RefreshCoreAsync(IEnumerable<byte> codes, bool recover)
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
            if (recover) Lost(MonitorStatus.NotResponding);
            else SetStatus(MonitorStatus.NotResponding);
            return false;
        }
        return true;
    }

    public async Task<WriteResult> SetAsync(byte code, uint value, bool confirmed = false)
    {
        if (Status != MonitorStatus.Connected) return NotRespondingResult;
        var caps = Capabilities;

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
            Lost(MonitorStatus.NotResponding);
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
                Lost(MonitorStatus.NotResponding);
                return NotRespondingResult;
            }
        }
        else if (def.Kind != ControlKind.Action)
        {
            UpdateCache(code, new VcpReading(v, hasCached ? cached.Max : 0u));
        }

        return new(WriteOutcome.Ok, Describe(def, v), v);
    }

    /// <summary>Suppresses <see cref="ValueChanged"/> for <paramref name="code"/> until <see cref="EndDragAsync"/> completes.</summary>
    public void BeginDrag(byte code)
    {
        lock (gate) drags.TryAdd(code, new Drag());
    }

    /// <summary>Latest value wins; at most one write in flight per code and no read-back.</summary>
    public void DragTo(byte code, uint value)
    {
        if (!Controls.TryGetValue(code, out var def) || def.Kind != ControlKind.Range) return;
        var hasCached = TryGetCached(code, out var cached);
        var decision = SafetyPolicy.Evaluate(def, Capabilities, code, value, hasCached ? cached.Max : null, confirmed: false);
        if (decision.Verdict != SafetyVerdict.Allow) return;

        Drag drag;
        lock (gate)
        {
            if (status != MonitorStatus.Connected) return;
            if (!drags.TryGetValue(code, out drag!)) drags[code] = drag = new Drag();
            if (drag.Ending) return;
            drag.Pending = decision.Value;
            if (drag.Idle is not null) return;
            drag.Idle = new TaskCompletionSource(TaskCreationOptions.RunContinuationsAsynchronously);
        }
        _ = DragLoopAsync(code, drag);
    }

    private async Task DragLoopAsync(byte code, Drag drag)
    {
        var failed = false;
        TaskCompletionSource idle;
        while (true)
        {
            uint v;
            lock (gate)
            {
                if (failed || drag.Pending is not uint pending)
                {
                    drag.Pending = null;
                    idle = drag.Idle!;
                    drag.Idle = null;
                    break;
                }
                v = pending;
                drag.Pending = null;
            }

            try
            {
                await queue.Run(l => { l.Write(code, v); return 0; });
                UpdateCache(code, new VcpReading(v, TryGetCached(code, out var r) ? r.Max : 100u));
            }
            catch (Exception ex)
            {
                FileLog.Write($"Drag write 0x{code:X2}={v} failed: {ex.Message}");
                failed = true;
                if (ex is DdcException) Lost(MonitorStatus.NotResponding);
            }
        }
        idle.SetResult();
    }

    /// <summary>Waits for the in-flight drag write, writes the released value, verifies it once and reports it.</summary>
    public async Task<WriteResult> EndDragAsync(byte code, uint value)
    {
        Task idle = Task.CompletedTask;
        lock (gate)
        {
            if (drags.TryGetValue(code, out var drag))
            {
                drag.Ending = true;
                drag.Pending = null;
                if (drag.Idle is { } tcs) idle = tcs.Task;
            }
        }
        await idle;

        WriteResult result;
        try
        {
            result = await SetAsync(code, value);
        }
        finally
        {
            lock (gate) drags.Remove(code);
        }
        if (TryGetCached(code, out var actual)) ValueChanged?.Invoke(code, actual.Current);
        return result;
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
    public void ReloadProfile() => Controls = profiles.Resolve(Capabilities);

    public void Dispose()
    {
        lock (gate)
        {
            if (disposed) return;
            disposed = true;
            burst?.Cancel();
            burst = null;
        }
        queue.Dispose();
    }

    private static MonitorCapabilities? TryParse(string? raw)
    {
        if (raw is null) return null;
        try
        {
            return CapabilitiesParser.Parse(raw);
        }
        catch (FormatException)
        {
            return null;
        }
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
            changed = (!cache.TryGetValue(code, out var old) || old.Current != reading.Current) && !drags.ContainsKey(code);
            cache[code] = reading;
        }
        if (changed) ValueChanged?.Invoke(code, reading.Current);
    }

    /// <summary>A failure during normal operation: report it and start recovering on our own.</summary>
    private void Lost(MonitorStatus value)
    {
        SetStatus(value);
        RequestReconnect();
    }

    private void SetStatus(MonitorStatus value)
    {
        bool statusChanged, respondingChanged;
        lock (gate)
        {
            if (disposed) return;
            var newReason = value == MonitorStatus.Connected ? LinkFailure.None : queue.Link.LastFailure;
            statusChanged = status != value || reason != newReason;
            respondingChanged = (status == MonitorStatus.Connected) != (value == MonitorStatus.Connected);
            status = value;
            reason = newReason;
        }
        if (statusChanged) StatusChanged?.Invoke(value);
        if (respondingChanged) RespondingChanged?.Invoke(value == MonitorStatus.Connected);
    }

    private static string Describe(ControlDefinition def, uint value) => def.Kind switch
    {
        ControlKind.Range => $"{def.Name} {value}",
        ControlKind.Enum => $"{def.Name} → {(value <= byte.MaxValue && def.ValueLabels.TryGetValue((byte)value, out var label) ? label : $"0x{value:X2}")}",
        _ => def.Name,
    };

    private sealed class Drag
    {
        public uint? Pending;
        public TaskCompletionSource? Idle;
        public bool Ending;
    }
}
