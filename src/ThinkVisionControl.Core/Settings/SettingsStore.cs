namespace ThinkVisionControl.Core.Settings;

/// <summary><c>settings.json</c> in <paramref name="directory"/>. Missing → defaults; corrupt → defaults and the file is renamed <c>.bak</c>.</summary>
public sealed class SettingsStore(string directory)
{
    public static AppSettings Defaults => throw new NotImplementedException();

    public AppSettings Load() => throw new NotImplementedException();

    public void Save(AppSettings settings) => throw new NotImplementedException();
}
