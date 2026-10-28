using ThinkVisionControl.Core.Control;
using ThinkVisionControl.Core.Ddc;
using ThinkVisionControl.Core.Finder;
using ThinkVisionControl.Core.Profile;
using ThinkVisionControl.Core.Settings;

namespace ThinkVisionControl.Tests;

public sealed class ControlServiceTests : IDisposable
{
    readonly TempDir _dir = new();
    readonly FakeMonitorLink _link = new();
    readonly ControlService _control;

    public ControlServiceTests() =>
        _control = new ControlService(new DdcQueue(_link), new ProfileStore(_dir.Path), new BackupStore(Path.Combine(_dir.Path, "backups")),
            new CapabilitiesCache(_dir.Path));

    public void Dispose()
    {
        _control.Dispose();
        _dir.Dispose();
    }

    [Fact]
    public async Task Start_reads_capabilities_and_values()
    {
        await _control.StartAsync();

        Assert.True(_control.IsResponding);
        Assert.True(_control.TryGetCached(0x12, out var contrast));
        Assert.Equal(70u, contrast.Current);
        Assert.True(_control.TryGetCached(0xE0, out var overDrive));
        Assert.Equal(1u, overDrive.Current);
        Assert.False(_control.TryGetCached(0xEB, out _)); // hidden codes are not read at start
    }

    [Fact]
    public async Task Step_clamps_at_max()
    {
        _link.Values[0x10] = new(95, 100);
        await _control.StartAsync();

        var result = await _control.StepAsync(0x10, 10);

        Assert.Equal(WriteOutcome.Ok, result.Outcome);
        Assert.Equal(((byte)0x10, 100u), _link.Writes.Single());
        Assert.Equal("Brightness 100", result.Message);
    }

    [Fact]
    public async Task Step_reports_when_monitor_does_not_apply_brightness()
    {
        await _control.StartAsync();
        _link.IgnoredWrites.Add(0x10);

        var result = await _control.StepAsync(0x10, 5);

        Assert.Equal(WriteOutcome.Blocked, result.Outcome);
        Assert.Contains("Brightness remains 0", result.Message);
        Assert.Contains("color mode may lock brightness", result.Message);
        Assert.Equal(((byte)0x10, 5u), _link.Writes.Single());
    }

    [Fact]
    public async Task Direct_range_write_verifies_the_monitor_applied_the_requested_value()
    {
        await _control.StartAsync();
        _link.IgnoredWrites.Add(0x10);

        var result = await _control.SetAsync(0x10, 25);

        Assert.Equal(WriteOutcome.Blocked, result.Outcome);
        Assert.Contains("Brightness remains 0", result.Message);
        Assert.Contains("color mode may lock brightness", result.Message);
        Assert.True(_control.TryGetCached(0x10, out var reading));
        Assert.Equal(0u, reading.Current);
    }

    [Fact]
    public async Task Enum_write_reports_label()
    {
        await _control.StartAsync();

        var result = await _control.SetAsync(0x60, 0x0F);

        Assert.Equal("Input → DisplayPort", result.Message);
    }

    [Fact]
    public async Task Dead_monitor_reports_not_responding_and_raises_event()
    {
        await _control.StartAsync();
        var events = new List<bool>();
        _control.RespondingChanged += events.Add;
        _link.Dead = true;

        var result = await _control.SetAsync(0x10, 30);

        Assert.Equal(WriteOutcome.NotResponding, result.Outcome);
        Assert.False(_control.IsResponding);
        Assert.Equal([false], events);
    }

    [Fact]
    public async Task Osd_enable_and_unnamed_codes_never_reach_the_monitor()
    {
        await _control.StartAsync();

        Assert.Equal(WriteOutcome.Blocked, (await _control.SetAsync(0xCA, 2, confirmed: true)).Outcome);
        Assert.Equal(WriteOutcome.Blocked, (await _control.SetAsync(0xEB, 1, confirmed: true)).Outcome);
        Assert.Empty(_link.Writes);
    }

    [Fact]
    public async Task Factory_reset_needs_confirmation_and_backs_up_first()
    {
        await _control.StartAsync();

        Assert.Equal(WriteOutcome.NeedsConfirmation, (await _control.SetAsync(0x04, 1)).Outcome);
        Assert.Empty(_link.Writes);

        Assert.Equal(WriteOutcome.Ok, (await _control.SetAsync(0x04, 1, confirmed: true)).Outcome);
        Assert.Equal(((byte)0x04, 1u), _link.Writes.Single());
        var backup = new BackupStore(Path.Combine(_dir.Path, "backups")).LoadLatest();
        Assert.NotNull(backup);
        Assert.Equal(70u, backup[0x12]);
        Assert.False(backup.ContainsKey(0xD6)); // only Normal-safety controls are backed up
    }

    [Fact]
    public async Task Restore_reapplies_latest_backup()
    {
        await _control.StartAsync();
        await _control.SetAsync(0x05, 1, confirmed: true); // backup taken with contrast 70
        await _control.SetAsync(0x12, 40);
        _link.Writes.Clear();

        Assert.Equal(WriteOutcome.Ok, (await _control.RestoreLatestBackupAsync()).Outcome);
        Assert.Contains(((byte)0x12, 70u), _link.Writes);
    }

    [Fact]
    public async Task Range_write_detects_monitor_rejecting_the_value()
    {
        await _control.StartAsync();
        _link.IgnoredWrites.Add(0x10); // e.g. brightness locked by a calibrated colour mode

        var result = await _control.SetAsync(0x10, 20);

        Assert.Equal(WriteOutcome.Blocked, result.Outcome);
        Assert.Contains("Brightness remains 0", result.Message);
        Assert.Contains("color mode may lock brightness", result.Message);
        Assert.True(_control.TryGetCached(0x10, out var reading));
        Assert.Equal(0u, reading.Current);
    }

    [Fact]
    public async Task Preset_sets_colour_mode_then_brightness_then_contrast()
    {
        await _control.StartAsync();

        var result = await _control.ApplyPresetAsync(new Preset("Night", 0, 60, 0x12));

        Assert.Equal("Night preset", result.Message);
        Assert.Equal(new (byte, uint)[] { (0x14, 0x12), (0x10, 0), (0x12, 60) }, _link.Writes);
    }

    [Fact]
    public async Task Finder_snapshot_reads_hidden_codes_and_diff_shows_change()
    {
        await _control.StartAsync();
        var before = await VendorCodeFinder.SnapshotAsync(_control);
        _link.Values[0xEB] = new(1, 1); // user changed an unidentified OSD setting with the buttons
        var after = await VendorCodeFinder.SnapshotAsync(_control);

        Assert.Equal([new CodeChange(0xEB, 0, 1)], VendorCodeFinder.Diff(before, after));
        Assert.Empty(_link.Writes);
    }

    [Fact]
    public void Status_is_connecting_before_start() => Assert.Equal(MonitorStatus.Connecting, _control.Status);

    [Fact]
    public async Task Unplugged_monitor_is_reported_as_not_found()
    {
        _link.Absent = true;
        var statuses = new List<MonitorStatus>();
        _control.StatusChanged += statuses.Add;

        await _control.StartAsync();

        Assert.Equal(MonitorStatus.NotFound, _control.Status);
        Assert.False(_control.IsResponding);
        Assert.Equal([MonitorStatus.NotFound], statuses);
    }

    [Fact]
    public async Task Found_monitor_without_ddc_is_reported_as_not_responding()
    {
        _link.Dead = true;

        await _control.StartAsync();

        Assert.Equal(MonitorStatus.NotResponding, _control.Status);
    }

    [Fact]
    public async Task Monitor_plugged_in_later_connects_on_reconnect()
    {
        _link.Absent = true;
        await _control.StartAsync();
        _link.Absent = false;

        await _control.ReconnectAsync();

        Assert.Equal(MonitorStatus.Connected, _control.Status);
        Assert.NotNull(_control.Capabilities);
    }

    [Fact]
    public async Task Unplugging_after_connect_is_reported_as_not_found()
    {
        await _control.StartAsync();
        _link.Absent = true;
        _link.Dead = true;

        await _control.ReconnectAsync();

        Assert.Equal(MonitorStatus.NotFound, _control.Status);
    }

    [Fact]
    public async Task Poll_reports_a_change_made_elsewhere()
    {
        await _control.StartAsync();
        var changes = new List<(byte, uint)>();
        _control.ValueChanged += (code, value) => changes.Add((code, value));
        _link.Values[0x60] = new(0x0F, 49); // the other PC switched to DisplayPort

        await _control.PollAsync([0x10, 0x60]);

        Assert.Equal([((byte)0x60, 0x0Fu)], changes);
    }

    [Fact]
    public async Task Poll_reads_nothing_during_a_drag()
    {
        await _control.StartAsync();
        _control.BeginDrag(0x10);
        var reads = _link.ReadCount;

        await _control.PollAsync([0x12, 0x60]);

        Assert.Equal(reads, _link.ReadCount);
    }

    [Fact]
    public async Task Poll_reads_nothing_right_after_a_write()
    {
        await _control.StartAsync();
        await _control.SetAsync(0x12, 50);
        var reads = _link.ReadCount;

        await _control.PollAsync([0x10, 0x60]);

        Assert.Equal(reads, _link.ReadCount);
    }

    [Fact]
    public async Task Failed_poll_keeps_the_monitor_connected()
    {
        await _control.StartAsync();
        var connects = _link.ConnectCount;
        _link.Dead = true; // standby, or its own menu is open

        await _control.PollAsync([0x10, 0x12]);
        await Task.Delay(300);

        Assert.Equal(MonitorStatus.Connected, _control.Status);
        Assert.Equal(connects, _link.ConnectCount);
    }

    [Fact]
    public async Task WhenQuiet_waits_for_a_recent_write_and_returns_at_once_otherwise()
    {
        await _control.StartAsync();
        var idle = System.Diagnostics.Stopwatch.StartNew();
        await _control.WhenQuietAsync(TimeSpan.FromSeconds(1), TimeSpan.FromSeconds(5));
        idle.Stop();

        await _control.SetAsync(0x12, 50);
        var busy = System.Diagnostics.Stopwatch.StartNew();
        await _control.WhenQuietAsync(TimeSpan.FromSeconds(1), TimeSpan.FromSeconds(5));
        busy.Stop();

        Assert.True(idle.ElapsedMilliseconds < 100, $"idle wait {idle.ElapsedMilliseconds} ms");
        Assert.InRange(busy.ElapsedMilliseconds, 200, 1500);
    }
}
