using System.Runtime.Versioning;
using Avalonia.Threading;
using Tmds.DBus.Protocol;

namespace ThinkVisionControl.Avalonia.Platform.Linux;

/// <summary>
/// IOsd for Wayland sessions, where the X11 overlay window can't be placed or kept on top: a transient desktop
/// notification (org.freedesktop.Notifications) that replaces the previous one instead of stacking.
/// Levels carry the <c>value</c> hint, which KDE draws as a progress bar (GNOME shows the text only).
/// </summary>
[SupportedOSPlatform("linux")]
public sealed class NotificationOsd : IOsd
{
    const string AppName = "ThinkVision Control";
    const string Icon = "video-display";
    const int TimeoutMs = 1500;

    readonly SemaphoreSlim _gate = new(1, 1); // keeps replaces_id in step when calls overlap
    readonly Timer _hideTimer;
    readonly Action<Action> _post;
    uint _lastId;

    /// <param name="post">Marshals <see cref="Hidden"/>; defaults to the Avalonia UI thread.</param>
    public NotificationOsd(Action<Action>? post = null)
    {
        _post = post ?? (a => Dispatcher.UIThread.Post(a));
        _hideTimer = new Timer(_ => _post(() => Hidden?.Invoke()));
    }

    public event Action? Hidden;

    /// <summary>The id the server returned for the last notification (0 before the first, or with no server).</summary>
    public uint LastId => _lastId;

    public void ShowMessage(string text) => Show(text, value: null);

    public void ShowLevel(string label, uint value, uint max)
    {
        var percent = max == 0 ? value : (uint)Math.Round(value * 100.0 / max);
        Show($"{label} {value}", (int)Math.Min(percent, 100));
    }

    void Show(string body, int? value)
    {
        _hideTimer.Change(TimeoutMs, Timeout.Infinite);
        _ = NotifyAsync(body, value);
    }

    /// <summary>Sends the notification; exposed for the smoke test. Returns the server's id, or 0 on failure.</summary>
    public async Task<uint> NotifyAsync(string body, int? value)
    {
        await _gate.WaitAsync().ConfigureAwait(false);
        try
        {
            if (await SessionBus.GetAsync().ConfigureAwait(false) is not { } bus) return 0;
            var replaces = _lastId;
            var message = SessionBus.Message(bus, "org.freedesktop.Notifications", "/org/freedesktop/Notifications",
                "org.freedesktop.Notifications", "Notify", "susssasa{sv}i", (ref MessageWriter w) =>
                {
                    w.WriteString(AppName);
                    w.WriteUInt32(replaces);
                    w.WriteString(Icon);
                    w.WriteString(AppName);
                    w.WriteString(body);
                    w.WriteArray(Array.Empty<string>());
                    var hints = w.WriteDictionaryStart();
                    w.WriteDictionaryEntryStart(); w.WriteString("transient"); w.WriteVariantBool(true);
                    w.WriteDictionaryEntryStart(); w.WriteString("urgency"); w.WriteVariantByte(0);
                    w.WriteDictionaryEntryStart(); w.WriteString("desktop-entry"); w.WriteVariantString("thinkvision-control");
                    if (value is { } v) { w.WriteDictionaryEntryStart(); w.WriteString("value"); w.WriteVariantInt32(v); }
                    w.WriteDictionaryEnd(hints);
                    w.WriteInt32(TimeoutMs);
                });
            _lastId = await bus.CallMethodAsync(message, (Message m, object? _) => m.GetBodyReader().ReadUInt32()).ConfigureAwait(false);
            return _lastId;
        }
        catch (Exception)
        {
            return 0; // no notification server: the OSD is silently absent
        }
        finally
        {
            _gate.Release();
        }
    }
}
