using System.Globalization;
using System.Text.Json;
using System.Text.Json.Serialization;

namespace ThinkVisionControl.Core.Profile;

/// <summary>Embedded P32p-20 profile merged with the user overlay in <c>profile.user.json</c> (overlay wins per code).</summary>
public sealed class ProfileStore(string userDirectory)
{
    private const string ResourceName = "ThinkVisionControl.Core.p32p20.profile.json";

    private static readonly JsonSerializerOptions Json = new(JsonSerializerDefaults.Web)
    {
        WriteIndented = true,
        RespectNullableAnnotations = true,
        RespectRequiredConstructorParameters = true,
        Converters = { new JsonStringEnumConverter() },
    };

    private readonly ProfileFile _embedded = LoadEmbedded();

    private string OverlayPath => Path.Combine(userDirectory, "profile.user.json");

    public string EdidId => _embedded.EdidId;

    /// <summary>
    /// Merged definitions for every code in <paramref name="caps"/>. Codes the profile does not know become
    /// Hidden + <see cref="SafetyClass.ReadOnlyUntilNamed"/>.
    /// </summary>
    public IReadOnlyDictionary<byte, ControlDefinition> Resolve(MonitorCapabilities caps)
    {
        var known = _embedded.Controls.Select(ToDefinition).ToDictionary(d => d.Code);
        foreach (var def in LoadOverlay()) known[def.Code] = def;

        return caps.Vcp.Keys.ToDictionary(
            code => code,
            code => known.GetValueOrDefault(code) ?? new ControlDefinition(
                code, $"Unknown 0x{code:X2}", ControlKind.Enum, PanelSection.Hidden, SafetyClass.ReadOnlyUntilNamed,
                new Dictionary<byte, string>()));
    }

    /// <summary>
    /// Capabilities implied by the merged profile, for rendering controls before the monitor has ever answered:
    /// Range is continuous, Enum allows its labelled values, Action allows 1.
    /// </summary>
    public MonitorCapabilities SynthesizeCapabilities()
    {
        var known = _embedded.Controls.Select(ToDefinition).ToDictionary(d => d.Code);
        foreach (var def in LoadOverlay()) known[def.Code] = def;

        return new MonitorCapabilities("", known.Values.ToDictionary(
            d => d.Code,
            d => d.Kind switch
            {
                ControlKind.Enum => (IReadOnlyList<byte>?)[.. d.ValueLabels.Keys.Order()],
                ControlKind.Action => [1],
                _ => null,
            }));
    }

    /// <summary>Adds or replaces one definition in the user overlay and saves it.</summary>
    public void SaveOverlay(ControlDefinition definition)
    {
        var controls = LoadOverlay().Where(d => d.Code != definition.Code).Append(definition).OrderBy(d => d.Code).Select(ToEntry).ToList();
        Directory.CreateDirectory(userDirectory);
        File.WriteAllText(OverlayPath, JsonSerializer.Serialize(new ProfileFile(_embedded.EdidId, controls), Json));
    }

    private static ProfileFile LoadEmbedded()
    {
        using var stream = typeof(ProfileStore).Assembly.GetManifestResourceStream(ResourceName)
            ?? throw new InvalidOperationException($"Embedded resource {ResourceName} is missing.");
        return JsonSerializer.Deserialize<ProfileFile>(stream, Json)!;
    }

    private List<ControlDefinition> LoadOverlay()
    {
        if (!File.Exists(OverlayPath)) return [];
        try
        {
            var file = JsonSerializer.Deserialize<ProfileFile>(File.ReadAllText(OverlayPath), Json);
            return file?.Controls.Select(ToDefinition).ToList() ?? [];
        }
        catch (Exception ex) when (ex is JsonException or FormatException or OverflowException or ArgumentException)
        {
            return [];
        }
    }

    private static ControlDefinition ToDefinition(ControlEntry e) => new(
        ParseHex(e.Code), e.Name, e.Kind, e.Section, e.Safety,
        (e.Values ?? []).ToDictionary(kv => ParseHex(kv.Key), kv => kv.Value),
        e.Confirm);

    private static ControlEntry ToEntry(ControlDefinition d) => new(
        Hex(d.Code), d.Name, d.Kind, d.Section, d.Safety,
        d.ValueLabels.OrderBy(kv => kv.Key).ToDictionary(kv => Hex(kv.Key), kv => kv.Value),
        d.ConfirmMessage);

    private static byte ParseHex(string s) =>
        byte.Parse(s.StartsWith("0x", StringComparison.OrdinalIgnoreCase) ? s.AsSpan(2) : s, NumberStyles.HexNumber, CultureInfo.InvariantCulture);

    private static string Hex(byte b) => $"0x{b:X2}";

    private sealed record ProfileFile(string EdidId, List<ControlEntry> Controls);

    private sealed record ControlEntry(
        string Code,
        string Name,
        ControlKind Kind,
        PanelSection Section,
        SafetyClass Safety,
        Dictionary<string, string>? Values = null,
        string? Confirm = null);
}
