using System.Runtime.Versioning;
using System.Text;

namespace ThinkVisionControl.Avalonia.Platform.Linux;

/// <summary>
/// Start at login through XDG autostart: <c>~/.config/autostart/thinkvision-control.desktop</c>
/// (or <c>$XDG_CONFIG_HOME/autostart</c>). Works on GNOME and KDE, X11 and Wayland.
/// </summary>
[SupportedOSPlatform("linux")]
public sealed class LinuxAutostart(string? directory = null, string? executable = null) : IAutostart
{
    public const string FileName = "thinkvision-control.desktop";

    public string FilePath => Path.Combine(directory ?? DefaultDirectory(), FileName);

    public bool IsEnabled
    {
        get
        {
            try
            {
                if (!File.Exists(FilePath)) return false;
                var text = File.ReadAllText(FilePath);
                return !text.Contains("Hidden=true", StringComparison.Ordinal)
                    && !text.Contains("X-GNOME-Autostart-enabled=false", StringComparison.Ordinal);
            }
            catch (IOException) { return false; }
            catch (UnauthorizedAccessException) { return false; }
        }
    }

    public void Set(bool enabled)
    {
        if (!enabled)
        {
            File.Delete(FilePath);
            return;
        }
        var exe = executable ?? Environment.ProcessPath ?? "thinkvision-control";
        Directory.CreateDirectory(Path.GetDirectoryName(FilePath)!);
        File.WriteAllText(FilePath, Entry(exe));
    }

    static string DefaultDirectory()
    {
        var config = Environment.GetEnvironmentVariable("XDG_CONFIG_HOME");
        if (string.IsNullOrEmpty(config) || !Path.IsPathRooted(config))
            config = Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.UserProfile), ".config");
        return Path.Combine(config, "autostart");
    }

    /// <summary>The autostart entry for <paramref name="executable"/>.</summary>
    public static string Entry(string executable) => $"""
        [Desktop Entry]
        Type=Application
        Name=ThinkVision Control
        Comment=Monitor brightness, contrast and input in the tray
        Exec={QuoteExecArgument(executable)} --startup
        TryExec={EscapeString(executable)}
        Icon=thinkvision-control
        Terminal=false
        X-GNOME-Autostart-enabled=true

        """;

    /// <summary>
    /// One Exec argument per the Desktop Entry spec: double-quoted, with <c>" ` $ \</c> backslash-escaped, then the
    /// general string escaping applied on top (so a literal backslash becomes four), and <c>%</c> doubled.
    /// </summary>
    public static string QuoteExecArgument(string argument)
    {
        var quoted = new StringBuilder("\"");
        foreach (var c in argument)
        {
            if (c is '"' or '`' or '$' or '\\') quoted.Append('\\');
            quoted.Append(c);
        }
        quoted.Append('"');
        return EscapeString(quoted.ToString()).Replace("%", "%%", StringComparison.Ordinal);
    }

    /// <summary>The general escaping for desktop-file string values.</summary>
    public static string EscapeString(string value) =>
        value.Replace("\\", @"\\", StringComparison.Ordinal).Replace("\n", @"\n", StringComparison.Ordinal)
             .Replace("\t", @"\t", StringComparison.Ordinal).Replace("\r", @"\r", StringComparison.Ordinal);
}
