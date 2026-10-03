namespace ThinkVisionControl.Core.Logging;

/// <summary>Append-only <c>log.txt</c>; rolls to <c>log.1.txt</c> at 1 MB. Never throws.</summary>
public static class FileLog
{
    public static void Initialize(string directory) => throw new NotImplementedException();

    public static void Write(string message) => throw new NotImplementedException();
}
