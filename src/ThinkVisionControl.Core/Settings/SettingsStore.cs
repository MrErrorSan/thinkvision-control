using System.Globalization;
using System.Text.Json;
using System.Text.Json.Serialization;

namespace ThinkVisionControl.Core.Settings;

/// <summary><c>settings.json</c> in <paramref name="directory"/>. Missing → defaults; corrupt → defaults and the file is renamed <c>.bak</c>.</summary>
public sealed class SettingsStore(string directory)
{
    private static readonly JsonSerializerOptions Options = new()
    {
        WriteIndented = true,
        RespectNullableAnnotations = true,
        Converters = { new JsonStringEnumConverter(), new HexByteConverter() },
    };

    public static AppSettings Defaults { get; } = new(
        new Dictionary<HotkeyAction, string>
        {
            [HotkeyAction.BrightnessUp] = "Ctrl+Alt+Up",
            [HotkeyAction.BrightnessDown] = "Ctrl+Alt+Down",
            [HotkeyAction.ContrastUp] = "Ctrl+Alt+Left",
            [HotkeyAction.ContrastDown] = "Ctrl+Alt+Right",
            [HotkeyAction.DayPreset] = "Ctrl+Alt+Home",
            [HotkeyAction.NightPreset] = "Ctrl+Alt+End",
            [HotkeyAction.InputHdmi] = "Ctrl+Alt+F1",
            [HotkeyAction.InputDp] = "Ctrl+Alt+F2",
            [HotkeyAction.InputUsbC] = "Ctrl+Alt+F3",
            [HotkeyAction.SwitchToMe] = "Ctrl+Alt+M",
            [HotkeyAction.OpenPanel] = "Ctrl+Alt+O",
        },
        new Preset("Day", 60, 70, null),
        new Preset("Night", 0, 70, null),
        0x11,
        false,
        false);

    private string FilePath => Path.Combine(directory, "settings.json");

    public AppSettings Load()
    {
        var path = FilePath;
        if (!File.Exists(path)) return Defaults;

        AppSettings? loaded;
        try { loaded = JsonSerializer.Deserialize<AppSettings>(File.ReadAllText(path), Options); }
        catch (JsonException) { loaded = null; }

        if (loaded?.Day is null || loaded.Night is null)
        {
            File.Move(path, path + ".bak", overwrite: true);
            return Defaults;
        }

        var hotkeys = new Dictionary<HotkeyAction, string>();
        foreach (var (action, gesture) in Defaults.Hotkeys)
        {
            var saved = loaded.Hotkeys?.GetValueOrDefault(action);
            hotkeys[action] = saved switch
            {
                "Ctrl+Alt+PageUp" when action == HotkeyAction.BrightnessUp => gesture,
                "Ctrl+Alt+PageDown" when action == HotkeyAction.BrightnessDown => gesture,
                "Ctrl+Alt+Shift+PageUp" when action == HotkeyAction.ContrastUp => gesture,
                "Ctrl+Alt+Shift+PageDown" when action == HotkeyAction.ContrastDown => gesture,
                _ => saved ?? gesture,
            };
        }
        return loaded with { Hotkeys = hotkeys };
    }

    public void Save(AppSettings settings)
    {
        Directory.CreateDirectory(directory);
        var path = FilePath;
        var temp = path + ".tmp";
        File.WriteAllText(temp, JsonSerializer.Serialize(settings, Options));
        File.Move(temp, path, overwrite: true);
    }

    private sealed class HexByteConverter : JsonConverter<byte>
    {
        public override byte Read(ref Utf8JsonReader reader, Type typeToConvert, JsonSerializerOptions options)
        {
            if (reader.TokenType == JsonTokenType.Number) return reader.GetByte();
            var text = reader.GetString() ?? throw new JsonException("Expected a byte.");
            var ok = text.StartsWith("0x", StringComparison.OrdinalIgnoreCase)
                ? byte.TryParse(text.AsSpan(2), NumberStyles.AllowHexSpecifier, CultureInfo.InvariantCulture, out var value)
                : byte.TryParse(text, NumberStyles.None, CultureInfo.InvariantCulture, out value);
            return ok ? value : throw new JsonException($"Invalid byte '{text}'.");
        }

        public override void Write(Utf8JsonWriter writer, byte value, JsonSerializerOptions options) =>
            writer.WriteStringValue($"0x{value:X2}");
    }
}
