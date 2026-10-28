using ThinkVisionControl.Core.Control;

namespace ThinkVisionControl.Avalonia.Platform;

/// <summary>Tray / menu-bar icon.</summary>
public interface ITray : IDisposable
{
    /// <summary>False when nothing can show the icon (GNOME without a StatusNotifierItem host).</summary>
    bool IsHostAvailable { get; }

    void Show(Func<IReadOnlyList<TrayMenuItem>> menu, Action primaryClick);

    void SetStatus(MonitorStatus status);
}

public sealed record TrayMenuItem(string Label, Action? Run, bool IsDefault = false, bool IsSeparator = false);