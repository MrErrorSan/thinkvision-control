using System.Diagnostics;
using ThinkVisionControl.Core.Ddc;

namespace ThinkVisionControl.Tests;

public class DdcQueueTests
{
    // A monitor that is slow to answer (first capabilities read, DDC/CI half off) must not keep the app from exiting:
    // a lingering process holds the single-instance lock, so the next launch from the Start menu does nothing.
    [Fact]
    public async Task Dispose_does_not_wait_for_a_stuck_monitor_command()
    {
        using var release = new ManualResetEventSlim();
        var link = new FakeMonitorLink();
        link.TryConnect();
        var queue = new DdcQueue(link);
        var started = new TaskCompletionSource();
        _ = queue.Run(_ =>
        {
            started.SetResult();
            release.Wait(TimeSpan.FromSeconds(5));
            return 0;
        });
        await started.Task;

        var clock = Stopwatch.StartNew();
        queue.Dispose();
        clock.Stop();
        release.Set();

        Assert.InRange(clock.Elapsed, DdcQueue.DisposeWait - TimeSpan.FromMilliseconds(100), DdcQueue.DisposeWait + TimeSpan.FromSeconds(1));
    }
}
