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
        _control = new ControlService(new DdcQueue(_link), new ProfileStore(_dir.Path), new BackupStore(Path.Combine(_dir.Path, "backups")));

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
        Assert.False(_control.TryGetCached(0xE0, out _)); // hidden codes are not read at start
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
        Assert.Equal(WriteOutcome.Blocked, (await _control.SetAsync(0xE0, 1, confirmed: true)).Outcome);
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
    public async Task Throttled_slider_sends_few_writes_and_latest_wins()
    {
        await _control.StartAsync();

        for (uint v = 1; v <= 20; v++) _control.SetThrottled(0x10, v);
        await WaitUntil(() => _link.Values[0x10].Current == 20);

        Assert.True(_link.Writes.Count <= 3, $"{_link.Writes.Count} writes");
        Assert.Equal(((byte)0x10, 20u), _link.Writes[^1]);
    }

    [Fact]
    public async Task Preset_sets_colour_mode_then_brightness_then_contrast()
    {
        await _control.StartAsync();

        var result = await _control.ApplyPresetAsync(new Preset("Night", 0, 60, 0x0B));

        Assert.Equal("Night preset", result.Message);
        Assert.Equal(new (byte, uint)[] { (0x14, 0x0B), (0x10, 0), (0x12, 60) }, _link.Writes);
    }

    [Fact]
    public async Task Finder_snapshot_reads_hidden_codes_and_diff_shows_change()
    {
        await _control.StartAsync();
        var before = await VendorCodeFinder.SnapshotAsync(_control);
        _link.Values[0xE0] = new(2, 2); // user changed Over Drive with the buttons
        var after = await VendorCodeFinder.SnapshotAsync(_control);

        Assert.Equal([new CodeChange(0xE0, 0, 2)], VendorCodeFinder.Diff(before, after));
        Assert.Empty(_link.Writes);
    }

    static async Task WaitUntil(Func<bool> condition)
    {
        for (var i = 0; i < 100 && !condition(); i++) await Task.Delay(50);
        Assert.True(condition());
    }
}
