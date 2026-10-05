using System.Runtime.Versioning;
using ThinkVisionControl.Core.Ddc;

namespace ThinkVisionControl.Avalonia.Platform.Linux;

/// <summary>
/// The Linux "System setup" cards. <see cref="Detect"/> gathers the live facts (desktop, PowerDevil process,
/// distribution); <see cref="Hints"/> turns facts into cards and is pure, for tests.
/// </summary>
[SupportedOSPlatform("linux")]
public sealed class LinuxSystemSetup(Func<bool>? powerDevilDdcActive = null, string? executable = null) : ISystemSetup
{
    public IReadOnlyList<SetupHint> Detect(LinkFailure reason, bool hotkeysAvailable, bool trayHostAvailable) =>
        Hints(reason, hotkeysAvailable, trayHostAvailable,
            Environment.GetEnvironmentVariable("XDG_CURRENT_DESKTOP"),
            (powerDevilDdcActive ?? IsPowerDevilDdcActive)(),
            ReadOsReleaseId(),
            CommandName(executable ?? Environment.ProcessPath));

    // ---- pure ----

    public static IReadOnlyList<SetupHint> Hints(LinkFailure reason, bool hotkeysAvailable, bool trayHostAvailable,
        string? currentDesktop, bool powerDevilDdcActive, string? osReleaseId, string command)
    {
        var hints = new List<SetupHint>();

        if (reason == LinkFailure.PermissionDenied)
            hints.Add(new SetupHint("linux-i2c-permission", "Allow access to monitor controls",
                "Your user can't open the monitor's I²C bus. Install the udev rule from the README (or your distribution's ddcutil package, " +
                "which ships an equivalent rule), then log out and back in.",
                """
                echo 'KERNEL=="i2c-[0-9]*", SUBSYSTEM=="i2c-dev", ATTRS{class}=="0x03*", TAG+="uaccess"' | sudo tee /etc/udev/rules.d/60-thinkvision-i2c.rules
                sudo udevadm control --reload && sudo udevadm trigger
                """));

        if (reason == LinkFailure.ModuleMissing)
            hints.Add(new SetupHint("linux-i2c-module", "Load the i2c-dev kernel module",
                "There are no /dev/i2c-* devices, so the i2c-dev module isn't loaded. Load it now and at every boot.",
                """
                sudo modprobe i2c-dev
                echo i2c-dev | sudo tee /etc/modules-load.d/i2c-dev.conf
                """));

        if (powerDevilDdcActive)
            hints.Add(new SetupHint("linux-powerdevil", "KDE's power manager also controls monitor brightness",
                "PowerDevil talks to the monitor over DDC/CI too, so the two fight over the bus and brightness changes don't stick. " +
                "Turn off its DDC control (the same override `systemctl --user edit plasma-powerdevil.service` makes); laptop screen brightness keeps working.",
                """
                mkdir -p ~/.config/systemd/user/plasma-powerdevil.service.d
                printf '[Service]\nEnvironment=POWERDEVIL_NO_DDCUTIL=1\n' > ~/.config/systemd/user/plasma-powerdevil.service.d/override.conf
                systemctl --user daemon-reload && systemctl --user restart plasma-powerdevil.service
                """));

        if (IsGnome(currentDesktop) && !trayHostAvailable)
        {
            var ubuntu = osReleaseId is not null && osReleaseId.Contains("ubuntu", StringComparison.OrdinalIgnoreCase);
            hints.Add(new SetupHint("linux-gnome-appindicator", "Install the AppIndicator extension to see the tray icon",
                ubuntu
                    ? "GNOME shows tray icons only through the AppIndicator extension. Ubuntu ships it; turn it back on, then log out and back in if the icon doesn't appear."
                    : "GNOME shows tray icons only through the AppIndicator extension. Install and enable it, logging out and back in between the two commands.",
                ubuntu
                    ? "gnome-extensions enable ubuntu-appindicators@ubuntu.com"
                    : """
                      sudo dnf install gnome-shell-extension-appindicator
                      gnome-extensions enable appindicatorsupport@rgcjonas.gmail.com
                      """));
        }

        if (!hotkeysAvailable)
            hints.Add(new SetupHint("linux-hotkeys", "Set desktop shortcuts",
                "This desktop doesn't let apps register global shortcuts. Add these commands as custom shortcuts instead: " +
                "GNOME: Settings → Keyboard → Custom Shortcuts; KDE: System Settings → Shortcuts → Add Command.",
                string.Join('\n', ShortcutCommands(command))));

        return hints;
    }

    public static IEnumerable<string> ShortcutCommands(string command) =>
    [
        $"{command} --step brightness=+5    # Brightness up",
        $"{command} --step brightness=-5    # Brightness down",
        $"{command} --step contrast=+5    # Contrast up",
        $"{command} --step contrast=-5    # Contrast down",
        $"{command} --preset day    # Day preset",
        $"{command} --preset night    # Night preset",
        $"{command} --input hdmi    # Switch to HDMI",
        $"{command} --input dp    # Switch to DisplayPort",
        $"{command} --input usbc    # Switch to USB-C",
        $"{command} --input this    # Switch to this computer",
        $"{command}    # Open the panel",
    ];

    public static bool IsGnome(string? currentDesktop) =>
        currentDesktop is not null && currentDesktop.Split(':').Any(d => d.Equals("GNOME", StringComparison.OrdinalIgnoreCase));

    /// <summary>The command to put in shortcuts: the running binary's full path, quoted if needed; the bare name under <c>dotnet run</c>.</summary>
    public static string CommandName(string? processPath)
    {
        if (string.IsNullOrEmpty(processPath) || Path.GetFileNameWithoutExtension(processPath) == "dotnet") return "thinkvision-control";
        return processPath.Contains(' ') ? $"'{processPath}'" : processPath;
    }

    /// <summary>PowerDevil's comm is truncated by the kernel to 15 characters ("org_kde_powerde"), so the cmdline confirms it.</summary>
    public static bool IsPowerDevilProcess(string comm, string? cmdline)
    {
        comm = comm.Trim();
        if (comm == "org_kde_powerdevil") return true;
        return comm == "org_kde_powerde" && cmdline is not null && cmdline.Contains("org_kde_powerdevil", StringComparison.Ordinal);
    }

    /// <summary>Whether a NUL-separated /proc/&lt;pid&gt;/environ turns PowerDevil's DDC off (POWERDEVIL_NO_DDCUTIL set to a non-zero value).</summary>
    public static bool HasDdcDisabled(string environ)
    {
        foreach (var entry in environ.Split('\0'))
            if (entry.StartsWith("POWERDEVIL_NO_DDCUTIL=", StringComparison.Ordinal))
            {
                var value = entry["POWERDEVIL_NO_DDCUTIL=".Length..];
                return value.Length > 0 && value != "0";
            }
        return false;
    }

    // ---- live ----

    /// <summary>True when a PowerDevil process of ours runs without POWERDEVIL_NO_DDCUTIL.</summary>
    public static bool IsPowerDevilDdcActive()
    {
        try
        {
            foreach (var dir in Directory.EnumerateDirectories("/proc"))
            {
                var pid = Path.GetFileName(dir);
                if (pid.Length == 0 || !pid.All(char.IsAsciiDigit)) continue;
                try
                {
                    var comm = File.ReadAllText(Path.Combine(dir, "comm"));
                    if (!comm.StartsWith("org_kde_powerde", StringComparison.Ordinal)) continue;
                    if (!IsPowerDevilProcess(comm, File.ReadAllText(Path.Combine(dir, "cmdline")))) continue;
                    return !HasDdcDisabled(File.ReadAllText(Path.Combine(dir, "environ")));
                }
                catch (Exception ex) when (ex is IOException or UnauthorizedAccessException)
                {
                    // Process exited, or another user's: skip.
                }
            }
        }
        catch (Exception ex) when (ex is IOException or UnauthorizedAccessException) { }
        return false;
    }

    /// <summary>ID and ID_LIKE from /etc/os-release, space-separated (e.g. "pop ubuntu debian").</summary>
    static string? ReadOsReleaseId()
    {
        try
        {
            var ids = File.ReadLines("/etc/os-release")
                .Where(l => l.StartsWith("ID=", StringComparison.Ordinal) || l.StartsWith("ID_LIKE=", StringComparison.Ordinal))
                .Select(l => l[(l.IndexOf('=') + 1)..].Trim('"', '\''));
            return string.Join(' ', ids);
        }
        catch (Exception ex) when (ex is IOException or UnauthorizedAccessException) { return null; }
    }
}
