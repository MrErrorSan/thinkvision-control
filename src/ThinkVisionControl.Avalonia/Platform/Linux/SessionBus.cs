using System.Runtime.Versioning;
using Tmds.DBus.Protocol;

namespace ThinkVisionControl.Avalonia.Platform.Linux;

/// <summary>
/// One shared session-bus connection (Tmds.DBus.Protocol, which comes with Avalonia.FreeDesktop) and the
/// small call helpers the Linux services use. Null connection = no session bus; callers degrade.
/// </summary>
[SupportedOSPlatform("linux")]
internal static class SessionBus
{
    public const string PortalName = "org.freedesktop.portal.Desktop";
    public const string PortalPath = "/org/freedesktop/portal/desktop";

    public delegate void BodyWriter(ref MessageWriter writer);

    static readonly Lazy<Task<DBusConnection?>> Shared = new(ConnectAsync);

    public static Task<DBusConnection?> GetAsync() => Shared.Value;

    static async Task<DBusConnection?> ConnectAsync()
    {
        var address = DBusAddress.Session;
        if (string.IsNullOrEmpty(address)) return null;
        var connection = new DBusConnection(address);
        try
        {
            await connection.ConnectAsync().ConfigureAwait(false);
            return connection;
        }
        catch (Exception)
        {
            connection.Dispose();
            return null;
        }
    }

    public static MessageBuffer Message(DBusConnection connection, string destination, string path, string @interface,
        string member, string? signature = null, BodyWriter? body = null)
    {
        var writer = connection.GetMessageWriter();
        try
        {
            writer.WriteMethodCallHeader(destination, path, @interface, member, signature, MessageFlags.None);
            body?.Invoke(ref writer);
            return writer.CreateMessage();
        }
        finally
        {
            writer.Dispose();
        }
    }

    /// <summary>org.freedesktop.DBus.NameHasOwner; false when there is no bus or the call fails.</summary>
    public static async Task<bool> NameHasOwnerAsync(string name)
    {
        try
        {
            if (await GetAsync().ConfigureAwait(false) is not { } bus) return false;
            var message = Message(bus, "org.freedesktop.DBus", "/org/freedesktop/DBus", "org.freedesktop.DBus", "NameHasOwner", "s",
                (ref MessageWriter w) => w.WriteString(name));
            return await bus.CallMethodAsync(message, (Message m, object? _) => m.GetBodyReader().ReadBool()).ConfigureAwait(false);
        }
        catch (Exception)
        {
            return false;
        }
    }

    /// <summary>org.freedesktop.DBus.Properties.Get on the portal; null when the interface or property is missing.</summary>
    public static async Task<VariantValue?> GetPortalPropertyAsync(string @interface, string property)
    {
        try
        {
            if (await GetAsync().ConfigureAwait(false) is not { } bus) return null;
            var message = Message(bus, PortalName, PortalPath, "org.freedesktop.DBus.Properties", "Get", "ss",
                (ref MessageWriter w) => { w.WriteString(@interface); w.WriteString(property); });
            return await bus.CallMethodAsync(message, (Message m, object? _) => m.GetBodyReader().ReadVariantValue()).ConfigureAwait(false);
        }
        catch (Exception)
        {
            return null;
        }
    }

    /// <summary>Unwraps nested variants (the portal's legacy Settings.Read returns v inside v).</summary>
    public static VariantValue Unwrap(VariantValue value)
    {
        while (value.Type == VariantValueType.Variant) value = value.GetVariantValue();
        return value;
    }
}
