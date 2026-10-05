using ThinkVisionControl.Core.Ddc;

namespace ThinkVisionControl.Avalonia.Platform.Null;

public sealed class NullSystemSetup : ISystemSetup
{
    public IReadOnlyList<SetupHint> Detect(LinkFailure reason, bool hotkeysAvailable, bool trayHostAvailable) => [];
}
