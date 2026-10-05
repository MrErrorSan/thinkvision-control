using System.IO;
using System.Runtime.InteropServices;

namespace ThinkVisionControl.Platform;

public static class StartMenuRegistration
{
    public static string EnsureShortcut(string? executablePath = null)
    {
        executablePath ??= Environment.ProcessPath;
        if (string.IsNullOrWhiteSpace(executablePath))
            throw new InvalidOperationException("Could not determine the ThinkVision Control executable path.");

        executablePath = Path.GetFullPath(executablePath);
        var programs = Environment.GetFolderPath(Environment.SpecialFolder.Programs);
        var shortcutPath = Path.Combine(programs, "ThinkVision Control.lnk");
        Directory.CreateDirectory(programs);

        var shellType = Type.GetTypeFromProgID("WScript.Shell")
            ?? throw new InvalidOperationException("Windows Script Host is unavailable.");
        object? shell = null;
        object? shortcut = null;
        try
        {
            shell = Activator.CreateInstance(shellType)
                ?? throw new InvalidOperationException("Could not create a Windows Script Host instance.");
            dynamic shellAutomation = shell;
            shortcut = shellAutomation.CreateShortcut(shortcutPath);
            dynamic link = shortcut;
            link.TargetPath = executablePath;
            link.WorkingDirectory = Path.GetDirectoryName(executablePath) ?? Environment.CurrentDirectory;
            link.Description = "Control Lenovo ThinkVision monitors";
            link.IconLocation = $"{executablePath},0";
            link.Save();
            return shortcutPath;
        }
        finally
        {
            if (shortcut is not null && Marshal.IsComObject(shortcut))
                Marshal.FinalReleaseComObject(shortcut);
            if (shell is not null && Marshal.IsComObject(shell))
                Marshal.FinalReleaseComObject(shell);
        }
    }
}
