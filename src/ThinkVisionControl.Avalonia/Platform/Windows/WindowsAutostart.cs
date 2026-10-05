using System.Runtime.Versioning;
using Microsoft.Win32;
using ThinkVisionControl.Core.Logging;

namespace ThinkVisionControl.Avalonia.Platform.Windows;

/// <summary>Start with Windows via the per-user Run key (no admin rights): <c>"&lt;exe&gt;" --startup</c>.</summary>
[SupportedOSPlatform("windows")]
public sealed class WindowsAutostart : IAutostart
{
    /// <summary>Passed by the Run key so a login launch in background mode can stay in the tray.</summary>
    public const string StartupArgument = "--startup";

    const string RunKey = @"Software\Microsoft\Windows\CurrentVersion\Run";
    const string ValueName = "ThinkVisionControl";

    public bool IsEnabled
    {
        get
        {
            using var key = Registry.CurrentUser.OpenSubKey(RunKey);
            return key?.GetValue(ValueName) is string;
        }
    }

    public void Set(bool enabled)
    {
        try
        {
            using var key = Registry.CurrentUser.CreateSubKey(RunKey);
            if (enabled && Environment.ProcessPath is { } exe)
                key.SetValue(ValueName, $"\"{exe}\" {StartupArgument}");
            else
                key.DeleteValue(ValueName, throwOnMissingValue: false);
        }
        catch (Exception ex) when (ex is UnauthorizedAccessException or System.Security.SecurityException or IOException)
        {
            FileLog.Write($"WindowsAutostart: {ex.GetType().Name}: {ex.Message}");
        }
    }
}
