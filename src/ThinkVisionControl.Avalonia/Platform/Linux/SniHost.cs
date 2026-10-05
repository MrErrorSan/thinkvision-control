using System.Runtime.Versioning;

namespace ThinkVisionControl.Avalonia.Platform.Linux;

/// <summary>
/// Whether a StatusNotifierItem host is running (KDE, or GNOME with the AppIndicator extension), i.e. whether
/// a tray icon can be seen at all. Stock GNOME has none.
/// </summary>
[SupportedOSPlatform("linux")]
public static class SniHost
{
    public const string WatcherName = "org.kde.StatusNotifierWatcher";

    /// <summary>DBus NameHasOwner("org.kde.StatusNotifierWatcher"). Blocks for at most a second; false on timeout or no bus.</summary>
    public static bool IsAvailable()
    {
        var query = Task.Run(() => SessionBus.NameHasOwnerAsync(WatcherName));
        return query.Wait(TimeSpan.FromSeconds(1)) && query.Result;
    }
}
