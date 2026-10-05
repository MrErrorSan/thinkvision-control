using System.Globalization;

namespace ThinkVisionControl.Core.Logging;

/// <summary>Append-only <c>log.txt</c>; rolls to <c>log.1.txt</c> at 1 MB. Never throws.</summary>
public static class FileLog
{
    private const long MaxBytes = 1024 * 1024;
    private static readonly Lock Gate = new();
    private static string? _directory;

    public static void Initialize(string directory)
    {
        lock (Gate)
        {
            _directory = directory;
            try { Directory.CreateDirectory(directory); }
            catch (Exception) { }
        }
    }

    public static void Write(string message)
    {
        lock (Gate)
        {
            if (_directory is null) return;
            try
            {
                var path = Path.Combine(_directory, "log.txt");
                var info = new FileInfo(path);
                if (info.Exists && info.Length > MaxBytes)
                    File.Move(path, Path.Combine(_directory, "log.1.txt"), overwrite: true);
                var stamp = DateTime.Now.ToString("yyyy-MM-dd HH:mm:ss.fff", CultureInfo.InvariantCulture);
                File.AppendAllText(path, $"{stamp} {message}{Environment.NewLine}");
            }
            catch (Exception) { }
        }
    }
}
