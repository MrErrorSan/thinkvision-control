using ThinkVisionControl.Core.Control;
using ThinkVisionControl.Core.Settings;

namespace ThinkVisionControl.Tests;

public class SettingsStoreTests
{
    [Fact]
    public void Missing_file_gives_defaults()
    {
        using var dir = new TempDir();
        var s = new SettingsStore(dir.Path).Load();

        Assert.Equal("Ctrl+Alt+PageUp", s.Hotkeys[HotkeyAction.BrightnessUp]);
        Assert.Equal(Enum.GetValues<HotkeyAction>().Length, s.Hotkeys.Count);
        Assert.Equal(new Preset("Night", 0, 70, null), s.Night);
        Assert.Equal(0x11, s.ThisMachineInput);
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
        var saved = SettingsStore.Defaults with { Hotkeys = hotkeys, ThisMachineInput = 0x31, StartWithWindows = true, Day = new Preset("Day", 55, 65, 0x01) };

        store.Save(saved);
        var loaded = store.Load();

        Assert.Equal("Ctrl+Shift+Win+K", loaded.Hotkeys[HotkeyAction.SwitchToMe]);
        Assert.Equal((0x31, true, saved.Day), (loaded.ThisMachineInput, loaded.StartWithWindows, loaded.Day));
    }
}

public class HotkeyGestureTests
{
    [Theory]
    [InlineData("Ctrl+Alt+PageUp", HotkeyGesture.ModControl | HotkeyGesture.ModAlt, 0x21)]
    [InlineData("ctrl + alt + shift + pagedown", HotkeyGesture.ModControl | HotkeyGesture.ModAlt | HotkeyGesture.ModShift, 0x22)]
    [InlineData("Win+Ctrl+F3", HotkeyGesture.ModControl | HotkeyGesture.ModWin, 0x72)]
    [InlineData("Ctrl+Alt+M", HotkeyGesture.ModControl | HotkeyGesture.ModAlt, 0x4D)]
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
