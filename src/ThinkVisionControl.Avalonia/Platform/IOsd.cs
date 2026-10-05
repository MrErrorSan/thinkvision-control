namespace ThinkVisionControl.Avalonia.Platform;

/// <summary>On-screen display shown after hotkey actions.</summary>
public interface IOsd
{
    void ShowMessage(string text);
    void ShowLevel(string label, uint value, uint max);
    event Action? Hidden;
}