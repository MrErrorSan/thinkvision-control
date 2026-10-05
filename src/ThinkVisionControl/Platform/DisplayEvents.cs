using Microsoft.Win32;

namespace ThinkVisionControl.Platform;

/// <summary>Calls <c>reconnect</c> after a display change or resume from sleep, debounced so a burst of events triggers one reconnect.</summary>
public sealed class DisplayEvents : IDisposable
{
    static readonly TimeSpan Debounce = TimeSpan.FromSeconds(2);

    readonly Action _reconnect;
    readonly Timer _timer;

    public DisplayEvents(Action reconnect)
    {
        _reconnect = reconnect;
        _timer = new Timer(_ => _reconnect());
        SystemEvents.DisplaySettingsChanged += OnDisplayChanged;
        SystemEvents.PowerModeChanged += OnPowerModeChanged;
    }

    public void Dispose()
    {
        SystemEvents.DisplaySettingsChanged -= OnDisplayChanged;
        SystemEvents.PowerModeChanged -= OnPowerModeChanged;
        _timer.Dispose();
    }

    void OnDisplayChanged(object? sender, EventArgs e) => Schedule();

    void OnPowerModeChanged(object? sender, PowerModeChangedEventArgs e)
    {
        if (e.Mode == PowerModes.Resume) Schedule();
    }

    void Schedule() => _timer.Change(Debounce, Timeout.InfiniteTimeSpan);
}
