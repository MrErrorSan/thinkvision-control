using ThinkVisionControl.Core.Commands;
using ThinkVisionControl.Core.Control;
using ThinkVisionControl.Core.Ddc;
using ThinkVisionControl.Core.Profile;

namespace ThinkVisionControl.Tests;

public sealed class OfflineAndReconnectTests : IDisposable
{
    readonly TempDir _dir = new();
    readonly FakeMonitorLink _link = new();

    ControlService NewService(FakeMonitorLink? link = null) =>
        new(new DdcQueue(link ?? _link), new ProfileStore(_dir.Path), new BackupStore(Path.Combine(_dir.Path, "backups")),
            new CapabilitiesCache(_dir.Path));

    public void Dispose() => _dir.Dispose();

    [Fact]
    public void Without_cache_controls_come_from_the_profile_and_status_is_connecting()
    {
        using var control = NewService();

        Assert.Equal(MonitorStatus.Connecting, control.Status);
        Assert.Equal("USB-C", control.Controls[0x60].ValueLabels[0x31]);
        Assert.True(control.Capabilities.Supports(0x10));
        Assert.True(control.Capabilities.AllowsValue(0x60, 0x0F));
    }

    [Fact]
    public async Task Connecting_caches_capabilities_for_the_next_launch()
    {
        using (var control = NewService())
            await control.StartAsync();

        Assert.True(File.Exists(Path.Combine(_dir.Path, "capabilities.txt")));
        using var offline = NewService(new FakeMonitorLink { Absent = true });
        Assert.Equal("ThinkVision P32p-20", offline.Capabilities.Model);
        Assert.True(offline.Controls.ContainsKey(0xEB)); // vendor code known only from the real string
    }

    [Fact]
    public async Task Writes_are_refused_until_connected()
    {
        _link.Absent = true;
        using var control = NewService();
        await control.StartAsync();

        Assert.Equal(MonitorStatus.NotFound, control.Status);
        Assert.Equal(WriteOutcome.NotResponding, (await control.SetAsync(0x10, 30)).Outcome);
        Assert.Empty(_link.Writes);
    }

    [Fact]
    public async Task Refresh_does_no_ddc_traffic_while_not_connected()
    {
        _link.Absent = true;
        using var control = NewService();
        await control.StartAsync();

        await control.RefreshAsync([0x10, 0x12]);

        Assert.Equal(0, _link.ReadCount);
    }

    [Fact]
    public async Task Retry_burst_runs_five_attempts_then_stops()
    {
        _link.Absent = true;
        using var control = NewService();
        var delays = new List<TimeSpan>();
        control.Delay = (d, _) => { lock (delays) delays.Add(d); return Task.CompletedTask; };
        await control.StartAsync();
        var afterStart = _link.ConnectCount;

        control.RequestReconnect();
        await WaitUntil(() => _link.ConnectCount == afterStart + RetryBurst.Delays.Length);
        await Task.Delay(200);

        Assert.Equal(afterStart + RetryBurst.Delays.Length, _link.ConnectCount);
        Assert.Equal(RetryBurst.Delays, delays);
        Assert.Equal(MonitorStatus.NotFound, control.Status);
    }

    [Fact]
    public async Task Monitor_plugged_in_during_a_burst_connects_and_ends_it()
    {
        _link.Absent = true;
        using var control = NewService();
        var calls = 0;
        control.Delay = (_, _) =>
        {
            if (Interlocked.Increment(ref calls) == 2) _link.Absent = false;
            return Task.CompletedTask;
        };
        await control.StartAsync();

        control.RequestReconnect();
        await WaitUntil(() => control.Status == MonitorStatus.Connected);
        await Task.Delay(200);

        Assert.Equal(2, calls);
    }

    [Fact]
    public async Task Drag_coalesces_writes_reads_nothing_and_verifies_once_on_release()
    {
        using var control = NewService();
        await control.StartAsync();
        var readsBefore = _link.ReadCount;
        var changes = new List<uint>();
        control.ValueChanged += (code, value) => { if (code == 0x10) lock (changes) changes.Add(value); };

        control.BeginDrag(0x10);
        for (uint v = 1; v <= 20; v++)
        {
            control.DragTo(0x10, v);
            await Task.Delay(10);
        }
        Assert.Equal(readsBefore, _link.ReadCount);
        Assert.Empty(changes);

        var result = await control.EndDragAsync(0x10, 20);

        Assert.Equal(WriteOutcome.Ok, result.Outcome);
        // Writes are 50 ms apart (measured on the P32p-20: back-to-back writes apply), so 20 steps every 10 ms coalesce to well under 20.
        Assert.True(_link.Writes.Count <= 12, $"{_link.Writes.Count} writes");
        Assert.Equal(((byte)0x10, 20u), _link.Writes[^1]);
        Assert.Equal(readsBefore + 1, _link.ReadCount);
        Assert.Equal([20u], changes);
    }

    [Fact]
    public async Task Drag_release_reports_a_value_the_monitor_kept()
    {
        using var control = NewService();
        await control.StartAsync();
        _link.IgnoredWrites.Add(0x10);

        control.BeginDrag(0x10);
        control.DragTo(0x10, 60);
        var result = await control.EndDragAsync(0x10, 60);

        Assert.Equal(WriteOutcome.Blocked, result.Outcome);
        Assert.Contains("remains", result.Message);
        Assert.True(control.TryGetCached(0x10, out var reading));
        Assert.Equal(0u, reading.Current);
    }

    static async Task WaitUntil(Func<bool> condition)
    {
        for (var i = 0; i < 200 && !condition(); i++) await Task.Delay(25);
        Assert.True(condition());
    }
}

public class CommandLineTests
{
    public static TheoryData<AppCommand> Commands => new()
    {
        new AppCommand.Preset(false), new AppCommand.Preset(true),
        new AppCommand.Input(0x11), new AppCommand.Input(0x0F), new AppCommand.Input(0x31), new AppCommand.Input(null),
        new AppCommand.Step(0x10, 5), new AppCommand.Step(0x12, -5),
    };

    [Theory]
    [MemberData(nameof(Commands))]
    public void Format_round_trips_through_parse(AppCommand command) =>
        Assert.Equal(command, CommandLine.Parse(CommandLine.Format(command)).Command);

    [Theory]
    [InlineData("--preset=NIGHT")]
    [InlineData("--preset", "night")]
    public void Accepts_both_value_forms_case_insensitively(params string[] args) =>
        Assert.Equal(new AppCommand.Preset(true), CommandLine.Parse(args).Command);

    [Theory]
    [InlineData]
    [InlineData("--bogus")]
    [InlineData("--preset", "noon")]
    [InlineData("--step", "brightness=+500")]
    public void Unknown_or_malformed_input_shows_the_window(params string[] args) =>
        Assert.IsType<AppCommand.Show>(CommandLine.Parse(args).Command);

    [Fact]
    public void Startup_flag_combines_with_a_command()
    {
        var (command, startup) = CommandLine.Parse(["--startup", "--input", "usb-c"]);

        Assert.True(startup);
        Assert.Equal(new AppCommand.Input(0x31), command);
    }
}
