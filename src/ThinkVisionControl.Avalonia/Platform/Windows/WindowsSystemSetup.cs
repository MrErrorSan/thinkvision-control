using ThinkVisionControl.Core.Ddc;

namespace ThinkVisionControl.Avalonia.Platform.Windows;

/// <summary>Windows needs no one-time setup: DDC/CI, hotkeys and the tray work out of the box.</summary>
public sealed class WindowsSystemSetup : ISystemSetup
{
    public IReadOnlyList<SetupHint> Detect(LinkFailure reason, bool hotkeysAvailable, bool trayHostAvailable) => [];
}
