using ThinkVisionControl.Core.Logging;

namespace ThinkVisionControl.Core.Control;

/// <summary>The last capability string the monitor reported, kept in <c>capabilities.txt</c> so controls render offline.</summary>
public sealed class CapabilitiesCache(string directory)
{
    private string FilePath => Path.Combine(directory, "capabilities.txt");

    public string? Load()
    {
        try
        {
            return File.Exists(FilePath) ? File.ReadAllText(FilePath) : null;
        }
        catch (Exception)
        {
            return null;
        }
    }

    public void Save(string raw)
    {
        try
        {
            Directory.CreateDirectory(directory);
            File.WriteAllText(FilePath, raw);
        }
        catch (Exception ex)
        {
            FileLog.Write($"Capabilities cache save failed: {ex.Message}");
        }
    }
}
