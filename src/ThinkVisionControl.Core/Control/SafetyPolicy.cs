using ThinkVisionControl.Core.Profile;

namespace ThinkVisionControl.Core.Control;

public enum SafetyVerdict { Allow, NeedsConfirmation, Block }

public sealed record SafetyDecision(SafetyVerdict Verdict, uint Value, string Message);

/// <summary>Pure evaluation of the spec §5 safety rules.</summary>
public static class SafetyPolicy
{
    public static SafetyDecision Evaluate(ControlDefinition? definition, MonitorCapabilities capabilities,
        byte code, uint value, uint? max, bool confirmed) => throw new NotImplementedException();
}
