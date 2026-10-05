namespace ThinkVisionControl.Core.Settings;

public enum HotkeyAction
{
    BrightnessUp, BrightnessDown, ContrastUp, ContrastDown, DayPreset, NightPreset,
    InputHdmi, InputDp, InputUsbC, SwitchToMe, OpenPanel,
}

public sealed record Preset(string Name, uint Brightness, uint Contrast, byte? ColourMode);

public sealed record AppSettings(
    IReadOnlyDictionary<HotkeyAction, string> Hotkeys,
    Preset Day,
    Preset Night,
    byte ThisMachineInput,
    bool StartWithWindows,
    bool BackgroundMode = true); // a file saved before this field existed loads as true; new installs get SettingsStore.Defaults (false)
