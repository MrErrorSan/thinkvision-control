using System.Runtime.Versioning;
using Avalonia.Media;
using Avalonia.Styling;
using Avalonia.Threading;
using Tmds.DBus.Protocol;
using AvaloniaApplication = global::Avalonia.Application;

namespace ThinkVisionControl.Avalonia.Platform.Linux;

/// <summary>
/// The desktop appearance from the xdg-desktop-portal Settings interface (<c>org.freedesktop.appearance</c>):
/// <c>color-scheme</c> (1 = dark, 2 = light, 0 = no preference) and <c>accent-color</c> ((ddd) in 0–1), kept
/// current through <c>SettingChanged</c>. Without a portal, or with no preference, light/dark comes from
/// Avalonia's own detection and there is no accent.
/// </summary>
[SupportedOSPlatform("linux")]
public sealed class LinuxThemeInfo : IThemeInfo
{
    const string SettingsInterface = "org.freedesktop.portal.Settings";
    const string Namespace = "org.freedesktop.appearance";

    readonly Action<Action> _post;
    uint? _scheme;
    Color? _accent;
    IDisposable? _watch;

    /// <param name="post">Marshals <see cref="Changed"/>; defaults to the Avalonia UI thread.</param>
    public LinuxThemeInfo(Action<Action>? post = null)
    {
        _post = post ?? (a => Dispatcher.UIThread.Post(a));
        Ready = Task.Run(InitAsync);
    }

    /// <summary>Completes once the portal has been read (or found missing).</summary>
    public Task Ready { get; }

    /// <summary>True when the portal answered.</summary>
    public bool HasPortal { get; private set; }

    public bool IsDark => DarkFromScheme(_scheme) ?? AvaloniaApplication.Current?.ActualThemeVariant == ThemeVariant.Dark;

    public Color? Accent => _accent;

    public event Action? Changed;

    /// <summary>color-scheme → dark (true), light (false) or no preference (null).</summary>
    public static bool? DarkFromScheme(uint? scheme) => scheme switch { 1 => true, 2 => false, _ => null };

    /// <summary>The accent from the portal's RGB doubles; null when unset or out of range (GNOME reports unset as negative values).</summary>
    public static Color? AccentFrom(double r, double g, double b)
    {
        static bool InRange(double x) => x is >= 0 and <= 1 && !double.IsNaN(x);
        if (!InRange(r) || !InRange(g) || !InRange(b)) return null;
        static byte Channel(double x) => (byte)Math.Round(x * 255);
        return Color.FromRgb(Channel(r), Channel(g), Channel(b));
    }

    async Task InitAsync()
    {
        try
        {
            if (await SessionBus.GetAsync().ConfigureAwait(false) is not { } bus) return;

            // Sender left open: a well-known sender name in a client-side match can miss the portal's unique name.
            _watch = await bus.WatchSignalAsync(null, SessionBus.PortalPath, SettingsInterface, "SettingChanged",
                (Message m, object? _) =>
                {
                    var reader = m.GetBodyReader();
                    return (Namespace: reader.ReadString(), Key: reader.ReadString(), Value: reader.ReadVariantValue());
                },
                (Notification<(string Namespace, string Key, VariantValue Value)> n) =>
                {
                    if (n.HasValue && n.Value.Namespace == Namespace && Apply(n.Value.Key, n.Value.Value))
                        _post(() => Changed?.Invoke());
                },
                ObserverFlags.None, emitOnCapturedContext: false).ConfigureAwait(false);

            var scheme = await ReadAsync(bus, "color-scheme").ConfigureAwait(false);
            var accent = await ReadAsync(bus, "accent-color").ConfigureAwait(false);
            HasPortal = scheme is not null || accent is not null;
            var changed = false;
            if (scheme is { } s) changed |= Apply("color-scheme", s);
            if (accent is { } a) changed |= Apply("accent-color", a);
            if (changed) _post(() => Changed?.Invoke());
        }
        catch (Exception)
        {
            // No portal: Avalonia's detection, no accent.
        }
    }

    bool Apply(string key, VariantValue raw)
    {
        var value = SessionBus.Unwrap(raw);
        switch (key)
        {
            case "color-scheme" when value.Type == VariantValueType.UInt32:
                var scheme = value.GetUInt32();
                if (_scheme == scheme) return false;
                _scheme = scheme;
                return true;
            case "accent-color" when value.Type == VariantValueType.Struct && value.Count == 3:
                var accent = AccentFrom(value.GetItem(0).GetDouble(), value.GetItem(1).GetDouble(), value.GetItem(2).GetDouble());
                if (_accent == accent) return false;
                _accent = accent;
                return true;
            default:
                return false;
        }
    }

    /// <summary>Settings.ReadOne (portal v2), falling back to the older Read; null when the key or the portal is missing.</summary>
    static async Task<VariantValue?> ReadAsync(DBusConnection bus, string key)
    {
        foreach (var method in new[] { "ReadOne", "Read" })
        {
            try
            {
                var message = SessionBus.Message(bus, SessionBus.PortalName, SessionBus.PortalPath, SettingsInterface, method, "ss",
                    (ref MessageWriter w) => { w.WriteString(Namespace); w.WriteString(key); });
                return await bus.CallMethodAsync(message, (Message m, object? _) => m.GetBodyReader().ReadVariantValue()).ConfigureAwait(false);
            }
            catch (DBusErrorReplyException ex) when (ex.ErrorName == "org.freedesktop.DBus.Error.UnknownMethod")
            {
                // v1 portal: try Read
            }
            catch (Exception)
            {
                return null;
            }
        }
        return null;
    }
}
