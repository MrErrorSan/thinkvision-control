using ThinkVisionControl.Core.Control;
using ThinkVisionControl.Core.Settings;
using System.Text.Json.Nodes;

namespace ThinkVisionControl.Tests;

public class SettingsStoreTests
{
    [Fact]
    public void Missing_file_gives_defaults()
    {
        using var dir = new TempDir();
        var s = new SettingsStore(dir.Path).Load();

        Assert.Equal("Ctrl+Alt+Up", s.Hotkeys[HotkeyAction.BrightnessUp]);
        Assert.Equal("Ctrl+Alt+Down", s.Hotkeys[HotkeyAction.BrightnessDown]);
        Assert.Equal("Ctrl+Alt+Left", s.Hotkeys[HotkeyAction.ContrastUp]);
        Assert.Equal("Ctrl+Alt+Right", s.Hotkeys[HotkeyAction.ContrastDown]);
        Assert.Equal(Enum.GetValues<HotkeyAction>().Length, s.Hotkeys.Count);
        Assert.Equal(new Preset("Night", 0, 70, null), s.Night);
        Assert.Equal(0x11, s.ThisMachineInput);
        Assert.False(s.BackgroundMode); // new installs exit on close
    }

    [Fact]
    public void Corrupt_file_gives_defaults_and_is_kept_as_bak()
    {
        using var dir = new TempDir();
        Directory.CreateDirectory(dir.Path);
        File.WriteAllText(Path.Combine(dir.Path, "settings.json"), "{ broken");

        var s = new SettingsStore(dir.Path).Load();

        Assert.Equal(SettingsStore.Defaults.Day, s.Day);
        Assert.True(File.Exists(Path.Combine(dir.Path, "settings.json.bak")));
        Assert.False(File.Exists(Path.Combine(dir.Path, "settings.json")));
    }

    [Fact]
    public void Save_then_load_round_trips()
    {
        using var dir = new TempDir();
        var store = new SettingsStore(dir.Path);
        var hotkeys = new Dictionary<HotkeyAction, string>(SettingsStore.Defaults.Hotkeys) { [HotkeyAction.SwitchToMe] = "Ctrl+Shift+Win+K" };
        var saved = SettingsStore.Defaults with
        {
            Hotkeys = hotkeys,
            ThisMachineInput = 0x31,
            StartWithWindows = true,
            BackgroundMode = false,
            Day = new Preset("Day", 55, 65, 0x01),
        };

        store.Save(saved);
        var loaded = store.Load();

        Assert.Equal("Ctrl+Shift+Win+K", loaded.Hotkeys[HotkeyAction.SwitchToMe]);
        Assert.Equal((0x31, true, saved.Day), (loaded.ThisMachineInput, loaded.StartWithWindows, loaded.Day));
        Assert.False(loaded.BackgroundMode);
    }

    [Fact]
    public void Existing_settings_without_background_mode_default_to_background()
    {
        using var dir = new TempDir();
        var settings = SettingsStore.Defaults;
        var store = new SettingsStore(dir.Path);
        store.Save(settings);
        var document = JsonNode.Parse(File.ReadAllText(Path.Combine(dir.Path, "settings.json")))
            ?? throw new InvalidOperationException("Settings JSON was empty.");
        document.AsObject().Remove(nameof(AppSettings.BackgroundMode));
        File.WriteAllText(Path.Combine(dir.Path, "settings.json"), document.ToJsonString());

        Assert.True(store.Load().BackgroundMode);
    }

    [Fact]
    public void Replaces_legacy_default_slider_shortcuts_but_preserves_custom_shortcuts()
    {
        using var dir = new TempDir();
        var store = new SettingsStore(dir.Path);
        var hotkeys = new Dictionary<HotkeyAction, string>(SettingsStore.Defaults.Hotkeys)
        {
            [HotkeyAction.BrightnessUp] = "Ctrl+Alt+PageUp",
            [HotkeyAction.BrightnessDown] = "Ctrl+Alt+PageDown",
            [HotkeyAction.ContrastUp] = "Ctrl+Alt+Shift+PageUp",
            [HotkeyAction.ContrastDown] = "Ctrl+Alt+Shift+PageDown",
            [HotkeyAction.DayPreset] = "Ctrl+Shift+D",
        };
        store.Save(SettingsStore.Defaults with { Hotkeys = hotkeys });

        var loaded = store.Load();

        Assert.Equal("Ctrl+Alt+Up", loaded.Hotkeys[HotkeyAction.BrightnessUp]);
        Assert.Equal("Ctrl+Alt+Down", loaded.Hotkeys[HotkeyAction.BrightnessDown]);
        Assert.Equal("Ctrl+Alt+Left", loaded.Hotkeys[HotkeyAction.ContrastUp]);
        Assert.Equal("Ctrl+Alt+Right", loaded.Hotkeys[HotkeyAction.ContrastDown]);
        Assert.Equal("Ctrl+Shift+D", loaded.Hotkeys[HotkeyAction.DayPreset]);
    }
}

public class HotkeyGestureTests
{
    [Theory]
    [InlineData("Ctrl+Alt+PageUp", HotkeyGesture.ModControl | HotkeyGesture.ModAlt, 0x21)]
    [InlineData("ctrl + alt + shift + pagedown", HotkeyGesture.ModControl | HotkeyGesture.ModAlt | HotkeyGesture.ModShift, 0x22)]
    [InlineData("Win+Ctrl+F3", HotkeyGesture.ModControl | HotkeyGesture.ModWin, 0x72)]
    [InlineData("Ctrl+Alt+M", HotkeyGesture.ModControl | HotkeyGesture.ModAlt, 0x4D)]
    [InlineData("Ctrl+Alt+Up", HotkeyGesture.ModControl | HotkeyGesture.ModAlt, 0x26)]
    [InlineData("Ctrl+Alt+W", HotkeyGesture.ModControl | HotkeyGesture.ModAlt, 0x57)]
    public void Parses_to_win32_codes(string text, uint mods, uint vk)
    {
        Assert.True(HotkeyGesture.TryParse(text, out var g));
        Assert.Equal(new HotkeyGesture(mods, vk), g);
    }

    [Theory]
    [InlineData("PageUp")]
    [InlineData("Ctrl+Alt")]
    [InlineData("Ctrl+A+B")]
    [InlineData("Ctrl+Banana")]
    public void Rejects_invalid(string text) => Assert.False(HotkeyGesture.TryParse(text, out _));

    [Theory]
    [InlineData(HotkeyGesture.ModControl | HotkeyGesture.ModAlt, 0x26, true)]
    [InlineData(HotkeyGesture.ModControl | HotkeyGesture.ModAlt, 0x57, true)]
    [InlineData(0, 0x26, false)]
    [InlineData(HotkeyGesture.ModControl, 0x00, false)]
    public void Creates_only_supported_modified_keys(uint modifiers, uint key, bool expected) =>
        Assert.Equal(expected, HotkeyGesture.TryCreate(modifiers, key, out _));

    [Fact]
    public void All_default_hotkeys_round_trip()
    {
        foreach (var text in SettingsStore.Defaults.Hotkeys.Values)
        {
            Assert.True(HotkeyGesture.TryParse(text, out var g), text);
            Assert.Equal(text, g.ToString());
        }
    }
}

public class BackupStoreTests
{
    [Fact]
    public void Latest_backup_wins_including_same_second_suffix()
    {
        using var dir = new TempDir();
        var store = new BackupStore(dir.Path);

        store.Save(new Dictionary<byte, uint> { [0x10] = 1 });
        store.Save(new Dictionary<byte, uint> { [0x10] = 2 });
        store.Save(new Dictionary<byte, uint> { [0x10] = 3 });

        Assert.Equal(3u, store.LoadLatest()![0x10]);
    }

    [Fact]
    public void No_backup_returns_null()
    {
        using var dir = new TempDir();
        Assert.Null(new BackupStore(dir.Path).LoadLatest());
    }
}
