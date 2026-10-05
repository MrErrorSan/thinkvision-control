namespace ThinkVisionControl.Avalonia.Platform.Null;

/// <summary>No on-screen display.</summary>
public sealed class NullOsd : IOsd
{
    public void ShowMessage(string text) { }
    public void ShowLevel(string label, uint value, uint max) { }
    public event Action? Hidden { add { } remove { } }
}