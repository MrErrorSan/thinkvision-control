using System.Runtime.Versioning;
using Avalonia.Threading;
using ThinkVisionControl.Core.Settings;
using Tmds.DBus.Protocol;

namespace ThinkVisionControl.Avalonia.Platform.Linux;

/// <summary>
/// Global hotkeys on Wayland through the xdg-desktop-portal GlobalShortcuts interface (KDE Plasma 5.27+, GNOME 48+).
/// <list type="number">
/// <item>Registers the host app id "thinkvision-control" (<c>org.freedesktop.host.portal.Registry.Register</c>), which
/// must precede other portal calls; ignored where the Registry doesn't exist.</item>
/// <item><c>CreateSession</c>, then <c>BindShortcuts</c> with one shortcut per <see cref="HotkeyAction"/> (id = enum name,
/// <c>preferred_trigger</c> from the gesture, e.g. "CTRL+ALT+Up"). The desktop may show a confirmation dialog and has the
/// final say on the trigger; <see cref="TriggerDescriptions"/> reports what it chose.</item>
/// <item><c>Activated</c> signals for our session → <see cref="Pressed"/> on the UI thread.</item>
/// </list>
/// No GlobalShortcuts interface (property <c>version</c> unreadable) → <see cref="IsAvailable"/> false.
/// A session can bind only once, so a changed map closes the session and opens a new one. <see cref="Suspend"/> keeps the
/// binding and ignores activations, which avoids re-prompting the user every time a shortcut is edited.
/// Not exercised against a real portal (WSL has none): only the "portal missing" path has run.
/// </summary>
[SupportedOSPlatform("linux")]
public sealed class PortalHotkeys : IHotkeys
{
    public const string AppId = "thinkvision-control";
    const string ShortcutsInterface = "org.freedesktop.portal.GlobalShortcuts";

    static int s_token;

    readonly Action<Action> _post;
    readonly SemaphoreSlim _bindGate = new(1, 1);
    DBusConnection? _bus;
    IDisposable? _activatedWatch;
    volatile string? _session;
    volatile bool _suspended;
    volatile bool _disposed;
    string? _boundKey;

    /// <param name="post">Marshals <see cref="Pressed"/>; defaults to the Avalonia UI thread.</param>
    public PortalHotkeys(Action<Action>? post = null)
    {
        _post = post ?? (a => Dispatcher.UIThread.Post(a));
        var probe = Task.Run(ProbeAsync);
        IsAvailable = probe.Wait(TimeSpan.FromSeconds(3)) && probe.Result;
    }

    public bool IsAvailable { get; }

    /// <summary>What the desktop actually bound, per action (the portal's <c>trigger_description</c>).</summary>
    public IReadOnlyDictionary<HotkeyAction, string> TriggerDescriptions { get; private set; } = new Dictionary<HotkeyAction, string>();

    public event Action<HotkeyAction>? Pressed;

    public IReadOnlyCollection<HotkeyAction> Register(IReadOnlyDictionary<HotkeyAction, string> map)
    {
        if (!IsAvailable) return [.. map.Keys];

        var failed = new List<HotkeyAction>();
        var shortcuts = new List<(string Id, string Description, string Trigger)>();
        foreach (var (action, text) in map.OrderBy(p => p.Key))
        {
            if (HotkeyGesture.TryParse(text, out var gesture) && LinuxKeys.ToPortalTrigger(gesture) is { } trigger)
                shortcuts.Add((action.ToString(), LinuxKeys.Describe(action), trigger));
            else
                failed.Add(action);
        }

        _suspended = false;
        var key = string.Join(';', shortcuts.Select(s => $"{s.Id}={s.Trigger}"));
        if (key != _boundKey)
        {
            _boundKey = key;
            _ = BindAsync(shortcuts);
        }
        // The portal answers asynchronously (possibly after a user dialog), so only unparseable gestures fail here.
        return failed;
    }

    public void Suspend() => _suspended = true;

    public void Dispose()
    {
        if (_disposed) return;
        _disposed = true;
        _activatedWatch?.Dispose();
        if (_bus is { } bus && _session is { } session) _ = CloseSessionAsync(bus, session);
    }

    async Task<bool> ProbeAsync()
    {
        if (await SessionBus.GetAsync().ConfigureAwait(false) is not { } bus) return false;
        _bus = bus;

        try
        {
            var register = SessionBus.Message(bus, SessionBus.PortalName, SessionBus.PortalPath, "org.freedesktop.host.portal.Registry",
                "Register", "sa{sv}", (ref MessageWriter w) =>
                {
                    w.WriteString(AppId);
                    w.WriteDictionaryEnd(w.WriteDictionaryStart());
                });
            await bus.CallMethodAsync(register).ConfigureAwait(false);
        }
        catch (Exception)
        {
            // No Registry (older portal) or already registered: carry on.
        }

        if (await SessionBus.GetPortalPropertyAsync(ShortcutsInterface, "version").ConfigureAwait(false) is null) return false;

        try
        {
            _activatedWatch = await bus.WatchSignalAsync(null, SessionBus.PortalPath, ShortcutsInterface, "Activated",
                (Message m, object? _) =>
                {
                    var reader = m.GetBodyReader();
                    return (Session: reader.ReadObjectPathAsString(), Id: reader.ReadString());
                },
                (Notification<(string Session, string Id)> n) =>
                {
                    if (!n.HasValue || _suspended || _disposed || n.Value.Session != _session) return;
                    if (Enum.TryParse<HotkeyAction>(n.Value.Id, out var action))
                        _post(() => Pressed?.Invoke(action));
                },
                ObserverFlags.None, emitOnCapturedContext: false).ConfigureAwait(false);
            return true;
        }
        catch (Exception)
        {
            return false;
        }
    }

    async Task BindAsync(List<(string Id, string Description, string Trigger)> shortcuts)
    {
        await _bindGate.WaitAsync().ConfigureAwait(false);
        try
        {
            if (_bus is not { } bus || _disposed) return;
            if (_session is { } old)
            {
                _session = null;
                await CloseSessionAsync(bus, old).ConfigureAwait(false);
            }
            if (shortcuts.Count == 0) return;

            var sessionToken = $"tvc_s{Interlocked.Increment(ref s_token)}";
            var created = await RequestAsync(bus, "CreateSession", "a{sv}", token => (ref MessageWriter w) =>
            {
                var options = w.WriteDictionaryStart();
                w.WriteDictionaryEntryStart(); w.WriteString("handle_token"); w.WriteVariantString(token);
                w.WriteDictionaryEntryStart(); w.WriteString("session_handle_token"); w.WriteVariantString(sessionToken);
                w.WriteDictionaryEnd(options);
            }).ConfigureAwait(false);
            if (created is null || !created.TryGetValue("session_handle", out var handle)) return;
            var session = SessionBus.Unwrap(handle) is var h && h.Type == VariantValueType.ObjectPath ? h.GetObjectPathAsString() : h.GetString();
            _session = session;

            var bound = await RequestAsync(bus, "BindShortcuts", "oa(sa{sv})sa{sv}", token => (ref MessageWriter w) =>
            {
                w.WriteObjectPath(session);
                var array = w.WriteArrayStart(DBusType.Struct);
                foreach (var (id, description, trigger) in shortcuts)
                {
                    w.WriteStructureStart();
                    w.WriteString(id);
                    var properties = w.WriteDictionaryStart();
                    w.WriteDictionaryEntryStart(); w.WriteString("description"); w.WriteVariantString(description);
                    w.WriteDictionaryEntryStart(); w.WriteString("preferred_trigger"); w.WriteVariantString(trigger);
                    w.WriteDictionaryEnd(properties);
                }
                w.WriteArrayEnd(array);
                w.WriteString(""); // no parent window
                var options = w.WriteDictionaryStart();
                w.WriteDictionaryEntryStart(); w.WriteString("handle_token"); w.WriteVariantString(token);
                w.WriteDictionaryEnd(options);
            }).ConfigureAwait(false);

            if (bound is not null && bound.TryGetValue("shortcuts", out var list))
                TriggerDescriptions = ReadTriggers(SessionBus.Unwrap(list));
        }
        catch (Exception)
        {
            // Portal refused or went away: hotkeys silently don't fire; the shortcut card still documents the CLI.
        }
        finally
        {
            _bindGate.Release();
        }
    }

    static Dictionary<HotkeyAction, string> ReadTriggers(VariantValue shortcuts)
    {
        var result = new Dictionary<HotkeyAction, string>();
        if (shortcuts.Type != VariantValueType.Array) return result;
        for (var i = 0; i < shortcuts.Count; i++)
        {
            var item = shortcuts.GetItem(i);
            if (item.Type != VariantValueType.Struct || item.Count < 2) continue;
            if (!Enum.TryParse<HotkeyAction>(item.GetItem(0).GetString(), out var action)) continue;
            var properties = item.GetItem(1);
            if (properties.Type != VariantValueType.Dictionary) continue;
            foreach (var (name, value) in properties.GetDictionary<string, VariantValue>())
                if (name == "trigger_description") result[action] = SessionBus.Unwrap(value).GetString();
        }
        return result;
    }

    /// <summary>
    /// Calls a portal method that answers through an org.freedesktop.portal.Request object. The Response signal is
    /// watched on the predicted request path before the call so it can't be missed. Null on cancel/failure/timeout.
    /// </summary>
    static async Task<Dictionary<string, VariantValue>?> RequestAsync(DBusConnection bus, string method, string signature,
        Func<string, SessionBus.BodyWriter> body)
    {
        var token = $"tvc{Interlocked.Increment(ref s_token)}";
        var sender = (bus.UniqueName ?? "").TrimStart(':').Replace('.', '_');
        var requestPath = $"{SessionBus.PortalPath}/request/{sender}/{token}";
        var response = new TaskCompletionSource<(uint Code, Dictionary<string, VariantValue> Results)>(TaskCreationOptions.RunContinuationsAsynchronously);

        using var watch = await bus.WatchSignalAsync(null, requestPath, "org.freedesktop.portal.Request", "Response",
            (Message m, object? _) =>
            {
                var reader = m.GetBodyReader();
                var code = reader.ReadUInt32();
                return (code, reader.ReadDictionaryOfStringToVariantValue());
            },
            (Notification<(uint, Dictionary<string, VariantValue>)> n) => { if (n.HasValue) response.TrySetResult(n.Value); },
            ObserverFlags.None, emitOnCapturedContext: false).ConfigureAwait(false);

        var call = SessionBus.Message(bus, SessionBus.PortalName, SessionBus.PortalPath, ShortcutsInterface, method, signature, body(token));
        await bus.CallMethodAsync(call).ConfigureAwait(false);

        // BindShortcuts may wait on a user dialog.
        var finished = await Task.WhenAny(response.Task, Task.Delay(TimeSpan.FromMinutes(5))).ConfigureAwait(false);
        if (finished != response.Task) return null;
        var (code, results) = response.Task.Result;
        return code == 0 ? results : null;
    }

    static async Task CloseSessionAsync(DBusConnection bus, string session)
    {
        try
        {
            var close = SessionBus.Message(bus, SessionBus.PortalName, session, "org.freedesktop.portal.Session", "Close");
            await bus.CallMethodAsync(close).ConfigureAwait(false);
        }
        catch (Exception) { }
    }
}
