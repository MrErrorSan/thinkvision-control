namespace ThinkVisionControl.Core.Settings;

/// <summary>A global hotkey as Win32 <c>RegisterHotKey</c> wants it. Text form: <c>"Ctrl+Alt+PageUp"</c>.</summary>
public readonly record struct HotkeyGesture(uint Modifiers, uint VirtualKey)
{
    public const uint ModAlt = 0x1, ModControl = 0x2, ModShift = 0x4, ModWin = 0x8;

    public static bool TryParse(string text, out HotkeyGesture gesture) => throw new NotImplementedException();

    public override string ToString() => throw new NotImplementedException();
}
