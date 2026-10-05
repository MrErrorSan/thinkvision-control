using Microsoft.Win32;

namespace ThinkVisionControl.Platform;

/// <summary>Start-with-Windows via the per-user Run key (no admin rights needed).</summary>
public static class StartupRegistration
{
    /// <summary>Passed by the Run key so a login launch in background mode can stay in the tray.</summary>
    public const string StartupArgument = "--startup";

    const string RunKey = @"Software\Microsoft\Windows\CurrentVersion\Run";
    const string ValueName = "ThinkVisionControl";

    public static void Apply(bool enabled)
    {
        using var key = Registry.CurrentUser.CreateSubKey(RunKey);
        if (enabled)
            key.SetValue(ValueName, $"\"{Environment.ProcessPath}\" {StartupArgument}");
        else
            key.DeleteValue(ValueName, throwOnMissingValue: false);
    }
}
