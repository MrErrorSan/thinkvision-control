using Avalonia.Controls;
using ThinkVisionControl.Avalonia.Platform;

namespace ThinkVisionControl.Avalonia.Views;

/// <summary>
/// IOsd as a small topmost, non-activating overlay window near the bottom centre of the screen.
/// Placeholder: the avalonia-ui task implements it.
/// </summary>
public sealed class OverlayOsd : IOsd
{
    /// <summary>Set by the platform layer to apply OS-specific window tweaks (e.g. click-through on macOS).</summary>
    public static Action<Window>? ConfigureWindow { get; set; }

    public event Action? Hidden;
    public void ShowMessage(string text) => throw new NotImplementedException();
    public void ShowLevel(string label, uint value, uint max) => throw new NotImplementedException();
    void RaiseHidden() => Hidden?.Invoke();
}
