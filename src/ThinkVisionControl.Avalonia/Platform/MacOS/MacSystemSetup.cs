using System.Runtime.Versioning;
using ThinkVisionControl.Core.Ddc;

namespace ThinkVisionControl.Avalonia.Platform.MacOS;

/// <summary>
/// macOS setup cards. The only fixable case is DDC failing on a built-in HDMI port (M1 and base M2),
/// which the link reports as <see cref="LinkFailure.UnsupportedPort"/>. Hotkeys (Carbon) and the menu-bar item
/// are always available here, so they never produce a card.
/// </summary>
[SupportedOSPlatform("macos")]
public sealed class MacSystemSetup : ISystemSetup
{
    public IReadOnlyList<SetupHint> Detect(LinkFailure reason, bool hotkeysAvailable, bool trayHostAvailable) =>
        reason == LinkFailure.UnsupportedPort
            ?
            [
                new SetupHint("macos-unsupported-port", "Use a USB-C or Thunderbolt port for this monitor",
                    "The built-in HDMI port on M1 and base M2 Macs doesn't pass monitor controls through. " +
                    "Connect the monitor with USB-C or Thunderbolt (or a USB-C to DisplayPort cable) instead.",
                    null),
            ]
            : [];
}
