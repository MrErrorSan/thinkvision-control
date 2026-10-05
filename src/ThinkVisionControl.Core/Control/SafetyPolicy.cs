using ThinkVisionControl.Core.Profile;

namespace ThinkVisionControl.Core.Control;

public enum SafetyVerdict { Allow, NeedsConfirmation, Block }

public sealed record SafetyDecision(SafetyVerdict Verdict, uint Value, string Message);

/// <summary>Pure evaluation of the write safety rules: never write OSD-enable (0xCA) or unnamed vendor codes, only values the monitor advertises, clamp ranges, and confirm guarded actions (resets, power).</summary>
public static class SafetyPolicy
{
    public static SafetyDecision Evaluate(ControlDefinition? definition, MonitorCapabilities capabilities,
        byte code, uint value, uint? max, bool confirmed)
    {
        if (definition is null || !capabilities.Supports(code))
            return Block(value, $"This monitor does not support 0x{code:X2}.");
        if (definition.Safety == SafetyClass.NeverWrite)
            return Block(value, $"{definition.Name} is never written.");
        if (definition.Safety == SafetyClass.ReadOnlyUntilNamed)
            return Block(value, "Unknown setting — identify it with the finder first.");

        switch (definition.Kind)
        {
            case ControlKind.Action:
                value = 1;
                break;
            case ControlKind.Range:
                value = Math.Min(value, max ?? 100);
                break;
            case ControlKind.Enum when !capabilities.AllowsValue(code, value):
                return Block(value, $"0x{value:X2} is not a valid value for {definition.Name}.");
        }

        if (definition.Safety == SafetyClass.ConfirmRequired && !confirmed)
            return new(SafetyVerdict.NeedsConfirmation, value, definition.ConfirmMessage ?? $"Change {definition.Name}?");

        return new(SafetyVerdict.Allow, value, string.Empty);
    }

    private static SafetyDecision Block(uint value, string message) => new(SafetyVerdict.Block, value, message);
}
