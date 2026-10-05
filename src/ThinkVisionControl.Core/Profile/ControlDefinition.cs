namespace ThinkVisionControl.Core.Profile;

public enum ControlKind { Range, Enum, Action }

public enum PanelSection { Main, Advanced, Hidden }

public enum SafetyClass { Normal, ConfirmRequired, NeverWrite, ReadOnlyUntilNamed }

public sealed record ControlDefinition(
    byte Code,
    string Name,
    ControlKind Kind,
    PanelSection Section,
    SafetyClass Safety,
    IReadOnlyDictionary<byte, string> ValueLabels,
    string? ConfirmMessage = null);
