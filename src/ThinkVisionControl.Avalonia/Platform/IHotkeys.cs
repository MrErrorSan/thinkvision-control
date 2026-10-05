using ThinkVisionControl.Core.Settings;

namespace ThinkVisionControl.Avalonia.Platform;

/// <summary>System-wide shortcuts.</summary>
public interface IHotkeys : IDisposable
{
    /// <summary>False when the OS offers no way to register them (Wayland without the portal) → setup card.</summary>
    bool IsAvailable { get; }

    /// <summary>Replaces all registrations with <paramref name="map"/> (gesture text per action); returns the actions that failed.</summary>
    IReadOnlyCollection<HotkeyAction> Register(IReadOnlyDictionary<HotkeyAction, string> map);

    /// <summary>Releases all registrations until the next <see cref="Register"/> (e.g. while editing a shortcut).</summary>
    void Suspend();

    event Action<HotkeyAction>? Pressed;
}