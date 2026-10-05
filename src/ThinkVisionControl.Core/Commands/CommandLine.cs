using System.Globalization;

namespace ThinkVisionControl.Core.Commands;

public abstract record AppCommand
{
    public sealed record Show : AppCommand;
    public sealed record Preset(bool Night) : AppCommand;
    public sealed record Input(byte? Code) : AppCommand;        // null = this PC's input
    public sealed record Step(byte Code, int Delta) : AppCommand;
}

/// <summary>
/// Command-line verbs: <c>--preset day|night</c>, <c>--input hdmi|dp|usbc|this</c>, <c>--step brightness=+5</c>
/// and the <c>--startup</c> flag. <c>--x value</c> and <c>--x=value</c> are both accepted, case-insensitively.
/// The first verb wins; an unknown or malformed one gives <see cref="AppCommand.Show"/>.
/// </summary>
public static class CommandLine
{
    private const int MaxStep = 100;

    // The first name for each code is the canonical one Format writes.
    private static readonly (string Name, byte? Code)[] Inputs =
        [("hdmi", 0x11), ("dp", 0x0F), ("displayport", 0x0F), ("usbc", 0x31), ("usb-c", 0x31), ("this", null)];

    private static readonly (string Name, byte Code)[] Steps = [("brightness", 0x10), ("contrast", 0x12)];

    public static (AppCommand Command, bool Startup) Parse(IReadOnlyList<string> args)
    {
        AppCommand? command = null;
        var startup = false;
        for (var i = 0; i < args.Count; i++)
        {
            var arg = args[i];
            if (arg is null || !arg.StartsWith("--", StringComparison.Ordinal)) continue;

            var eq = arg.IndexOf('=');
            var name = (eq < 0 ? arg[2..] : arg[2..eq]).ToLowerInvariant();
            if (name == "startup") { startup = true; continue; }
            if (name is not ("preset" or "input" or "step")) continue;

            var value = eq >= 0 ? arg[(eq + 1)..]
                : i + 1 < args.Count && args[i + 1] is { } next && !next.StartsWith("--", StringComparison.Ordinal) ? args[++i]
                : null;
            command ??= ParseVerb(name, value?.Trim().ToLowerInvariant()) ?? new AppCommand.Show();
        }
        return (command ?? new AppCommand.Show(), startup);
    }

    public static string[] Format(AppCommand command) => command switch
    {
        AppCommand.Show => [],
        AppCommand.Preset p => ["--preset", p.Night ? "night" : "day"],
        AppCommand.Input i when Array.FindIndex(Inputs, x => x.Code == i.Code) is var at and >= 0 => ["--input", Inputs[at].Name],
        AppCommand.Step s when Array.FindIndex(Steps, x => x.Code == s.Code) is var at and >= 0 && s.Delta is >= -MaxStep and <= MaxStep =>
            ["--step", $"{Steps[at].Name}={s.Delta.ToString("+0;-0;+0", CultureInfo.InvariantCulture)}"],
        _ => throw new ArgumentOutOfRangeException(nameof(command), command, "Not expressible as command-line arguments."),
    };

    private static AppCommand? ParseVerb(string name, string? value)
    {
        if (value is null) return null;
        switch (name)
        {
            case "preset":
                return value switch { "day" => new AppCommand.Preset(false), "night" => new AppCommand.Preset(true), _ => null };
            case "input":
                var input = Array.FindIndex(Inputs, x => x.Name == value);
                return input >= 0 ? new AppCommand.Input(Inputs[input].Code) : null;
            default:
                var eq = value.IndexOf('=');
                if (eq < 0) return null;
                var target = Array.FindIndex(Steps, x => x.Name == value[..eq]);
                return target >= 0
                    && int.TryParse(value.AsSpan(eq + 1), NumberStyles.AllowLeadingSign, CultureInfo.InvariantCulture, out var delta)
                    && delta is >= -MaxStep and <= MaxStep
                    ? new AppCommand.Step(Steps[target].Code, delta)
                    : null;
        }
    }
}
