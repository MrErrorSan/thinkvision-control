namespace ThinkVisionControl.Avalonia.Platform.Linux;

/// <summary>
/// Nothing to publish at runtime on Linux. The launcher shortcuts (Day, Night, Switch to this computer) are the
/// <c>Actions=</c> of <c>packaging/linux/thinkvision-control.desktop</c>, which the README has the user install
/// into <c>~/.local/share/applications</c>; GNOME and KDE show them in the launcher's context menu.
/// </summary>
public sealed class LinuxQuickActions : IQuickActions
{
    public void Publish() { }
}
