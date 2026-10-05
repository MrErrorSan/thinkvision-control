using System.Runtime.Versioning;

namespace ThinkVisionControl.Avalonia.Platform.MacOS;

/// <summary>
/// Nothing to publish on macOS. The Windows equivalent (jump list) maps to the Dock menu, which needs a Dock icon,
/// and this is a menu-bar-only app (<c>LSUIElement</c> / <c>ShowInDock = false</c>). The menu-bar item's menu,
/// built by the tray, is the quick-action surface; the command-line verbs cover scripting.
/// </summary>
[SupportedOSPlatform("macos")]
public sealed class MacQuickActions : IQuickActions
{
    public void Publish() { }
}
