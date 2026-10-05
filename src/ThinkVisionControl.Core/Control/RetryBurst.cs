namespace ThinkVisionControl.Core.Control;

/// <summary>Bounded reconnect schedule run after a failure or a device/display event; then it stops.</summary>
public static class RetryBurst
{
    public static readonly TimeSpan[] Delays =
        [TimeSpan.FromSeconds(0.5), TimeSpan.FromSeconds(1), TimeSpan.FromSeconds(2), TimeSpan.FromSeconds(4), TimeSpan.FromSeconds(8)];
}
