using ThinkVisionControl.Core.Settings;

namespace ThinkVisionControl.Avalonia.Platform.Null;

/// <summary>No global shortcuts: unavailable, so every registration fails.</summary>
public sealed class NullHotkeys : IHotkeys
{
    public bool IsAvailable => false;
    public IReadOnlyCollection<HotkeyAction> Register(IReadOnlyDictionary<HotkeyAction, string> map) => [.. map.Keys];
    public void Suspend() { }
    public event Action<HotkeyAction>? Pressed { add { } remove { } }
    public void Dispose() { }
}