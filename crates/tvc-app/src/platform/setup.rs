//! "System setup" cards. Windows has none.

use tvc_core::link::LinkFailure;

/// One setup card. `command` is the copyable text (lines separated by `\n`); empty when there is nothing to copy.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Hint {
    pub id: &'static str,
    pub title: String,
    pub message: String,
    pub command: String,
}

/// The cards for the current link failure and hotkey state. On Linux this also looks at the desktop, a running
/// PowerDevil, `/etc/os-release` and (on GNOME only) whether a tray host is on the session bus, so call it off
/// the UI thread or rarely.
pub fn hints(failure: Option<LinkFailure>, hotkeys_ok: bool) -> Vec<Hint> {
    #[cfg(target_os = "linux")]
    {
        let desktop = std::env::var("XDG_CURRENT_DESKTOP").ok();
        let gnome = is_gnome(desktop.as_deref());
        let facts = LinuxFacts {
            gnome_without_tray: gnome && live::tray_host_missing(),
            powerdevil_ddc: live::powerdevil_ddc_active(),
            os_release_ids: live::os_release_ids(),
            command: command_name(std::env::current_exe().ok().map(|p| p.display().to_string()).as_deref()),
        };
        linux_hints(failure, hotkeys_ok, &facts)
    }
    #[cfg(target_os = "macos")]
    {
        let _ = hotkeys_ok; // Carbon hotkeys are always available.
        mac_hints(failure)
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    {
        let _ = (failure, hotkeys_ok);
        Vec::new()
    }
}

#[cfg_attr(not(any(target_os = "linux", target_os = "macos")), allow(dead_code))]
fn hint(id: &'static str, title: &str, message: &str, command: &str) -> Hint {
    Hint { id, title: title.into(), message: message.into(), command: command.into() }
}

#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn mac_hints(failure: Option<LinkFailure>) -> Vec<Hint> {
    if failure != Some(LinkFailure::UnsupportedPort) {
        return Vec::new();
    }
    vec![hint(
        "macos-unsupported-port",
        "Use a USB-C or Thunderbolt port for this monitor",
        "The built-in HDMI port on M1 and base M2 Macs doesn't pass monitor controls through. \
         Connect the monitor with USB-C or Thunderbolt (or a USB-C to DisplayPort cable) instead.",
        "",
    )]
}

/// What the Linux cards depend on besides the failure, gathered live by `hints`.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
struct LinuxFacts {
    gnome_without_tray: bool,
    powerdevil_ddc: bool,
    /// `ID` and `ID_LIKE` from /etc/os-release, space-separated.
    os_release_ids: Option<String>,
    command: String,
}

#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn linux_hints(failure: Option<LinkFailure>, hotkeys_ok: bool, facts: &LinuxFacts) -> Vec<Hint> {
    let mut hints = Vec::new();
    if failure == Some(LinkFailure::PermissionDenied) {
        hints.push(hint(
            "linux-i2c-permission",
            "Allow access to monitor controls",
            "Your user can't open the monitor's I²C bus. Install the udev rule from the README (or your distribution's \
             ddcutil package, which ships an equivalent rule), then log out and back in.",
            "echo 'KERNEL==\"i2c-[0-9]*\", SUBSYSTEM==\"i2c-dev\", ATTRS{class}==\"0x03*\", TAG+=\"uaccess\"' \
             | sudo tee /etc/udev/rules.d/60-thinkvision-i2c.rules\n\
             sudo udevadm control --reload && sudo udevadm trigger",
        ));
    }
    if failure == Some(LinkFailure::ModuleMissing) {
        hints.push(hint(
            "linux-i2c-module",
            "Load the i2c-dev kernel module",
            "There are no /dev/i2c-* devices, so the i2c-dev module isn't loaded. Load it now and at every boot.",
            "sudo modprobe i2c-dev\necho i2c-dev | sudo tee /etc/modules-load.d/i2c-dev.conf",
        ));
    }
    if facts.powerdevil_ddc {
        hints.push(hint(
            "linux-powerdevil",
            "KDE's power manager also controls monitor brightness",
            "PowerDevil talks to the monitor over DDC/CI too, so the two fight over the bus and brightness changes \
             don't stick. Turn off its DDC control (the same override `systemctl --user edit \
             plasma-powerdevil.service` makes); laptop screen brightness keeps working.",
            "mkdir -p ~/.config/systemd/user/plasma-powerdevil.service.d\n\
             printf '[Service]\\nEnvironment=POWERDEVIL_NO_DDCUTIL=1\\n' > \
             ~/.config/systemd/user/plasma-powerdevil.service.d/override.conf\n\
             systemctl --user daemon-reload && systemctl --user restart plasma-powerdevil.service",
        ));
    }
    if facts.gnome_without_tray {
        let ubuntu = facts.os_release_ids.as_deref().is_some_and(|ids| ids.to_ascii_lowercase().contains("ubuntu"));
        hints.push(if ubuntu {
            hint(
                "linux-gnome-appindicator",
                "Install the AppIndicator extension to see the tray icon",
                "GNOME shows tray icons only through the AppIndicator extension. Ubuntu ships it; turn it back on, \
                 then log out and back in if the icon doesn't appear.",
                "gnome-extensions enable ubuntu-appindicators@ubuntu.com",
            )
        } else {
            hint(
                "linux-gnome-appindicator",
                "Install the AppIndicator extension to see the tray icon",
                "GNOME shows tray icons only through the AppIndicator extension. Install and enable it, logging out \
                 and back in between the two commands.",
                "sudo dnf install gnome-shell-extension-appindicator\n\
                 gnome-extensions enable appindicatorsupport@rgcjonas.gmail.com",
            )
        });
    }
    if !hotkeys_ok {
        hints.push(hint(
            "linux-hotkeys",
            "Set desktop shortcuts",
            "This desktop doesn't let apps register global shortcuts. Add these commands as custom shortcuts instead: \
             GNOME: Settings → Keyboard → Custom Shortcuts; KDE: System Settings → Shortcuts → Add Command.",
            &shortcut_commands(&facts.command),
        ));
    }
    hints
}

#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn shortcut_commands(cmd: &str) -> String {
    [
        "--step brightness=+5    # Brightness up",
        "--step brightness=-5    # Brightness down",
        "--step contrast=+5    # Contrast up",
        "--step contrast=-5    # Contrast down",
        "--preset day    # Day preset",
        "--preset night    # Night preset",
        "--input hdmi    # Switch to HDMI",
        "--input dp    # Switch to DisplayPort",
        "--input usbc    # Switch to USB-C",
        "--input this    # Switch to this computer",
    ]
    .iter()
    .map(|rest| format!("{cmd} {rest}"))
    .chain(std::iter::once(format!("{cmd}    # Open the panel")))
    .collect::<Vec<_>>()
    .join("\n")
}

/// `XDG_CURRENT_DESKTOP` is a colon-separated list; GNOME matches case-insensitively.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn is_gnome(desktop: Option<&str>) -> bool {
    desktop.is_some_and(|d| d.split(':').any(|p| p.eq_ignore_ascii_case("GNOME")))
}

/// The command to bind: the running binary's full path, single-quoted if it contains a space.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn command_name(exe: Option<&str>) -> String {
    match exe {
        None | Some("") => "thinkvision-control".into(),
        Some(p) if p.contains(' ') => format!("'{p}'"),
        Some(p) => p.into(),
    }
}

/// PowerDevil's comm is cut to 15 characters by the kernel ("org_kde_powerde"), so the cmdline confirms it.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn is_powerdevil_process(comm: &str, cmdline: &str) -> bool {
    let comm = comm.trim();
    comm == "org_kde_powerdevil" || (comm == "org_kde_powerde" && cmdline.contains("org_kde_powerdevil"))
}

/// Whether a NUL-separated environ sets `POWERDEVIL_NO_DDCUTIL` to a non-zero value.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn has_ddc_disabled(environ: &str) -> bool {
    environ
        .split('\0')
        .find_map(|e| e.strip_prefix("POWERDEVIL_NO_DDCUTIL="))
        .is_some_and(|v| !v.is_empty() && v != "0")
}

#[cfg(target_os = "linux")]
mod live {
    use std::fs;
    use std::process::{Command, Stdio};

    /// True when a PowerDevil process of ours runs without `POWERDEVIL_NO_DDCUTIL`.
    pub fn powerdevil_ddc_active() -> bool {
        let Ok(dirs) = fs::read_dir("/proc") else { return false };
        for dir in dirs.flatten() {
            let name = dir.file_name();
            if !name.to_string_lossy().bytes().all(|b| b.is_ascii_digit()) {
                continue;
            }
            let path = dir.path();
            let Ok(comm) = fs::read_to_string(path.join("comm")) else { continue };
            if !comm.starts_with("org_kde_powerde") {
                continue;
            }
            let cmdline = fs::read(path.join("cmdline")).unwrap_or_default();
            if !super::is_powerdevil_process(&comm, &String::from_utf8_lossy(&cmdline)) {
                continue;
            }
            // Another user's process (unreadable environ) is skipped.
            if let Ok(environ) = fs::read(path.join("environ")) {
                return !super::has_ddc_disabled(&String::from_utf8_lossy(&environ));
            }
        }
        false
    }

    pub fn os_release_ids() -> Option<String> {
        let text = fs::read_to_string("/etc/os-release").ok()?;
        let ids: Vec<&str> = text
            .lines()
            .filter_map(|l| l.strip_prefix("ID=").or_else(|| l.strip_prefix("ID_LIKE=")))
            .map(|v| v.trim_matches(|c| c == '"' || c == '\''))
            .collect();
        Some(ids.join(" "))
    }

    /// No `org.kde.StatusNotifierWatcher` on the session bus (asked through `gdbus`, 1 s timeout). Unknown → false.
    pub fn tray_host_missing() -> bool {
        Command::new("gdbus")
            .args([
                "call",
                "--session",
                "--timeout=1",
                "--dest=org.freedesktop.DBus",
                "--object-path=/org/freedesktop/DBus",
                "--method=org.freedesktop.DBus.NameHasOwner",
                "org.kde.StatusNotifierWatcher",
            ])
            .stderr(Stdio::null())
            .output()
            .is_ok_and(|o| o.status.success() && String::from_utf8_lossy(&o.stdout).contains("false"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn facts() -> LinuxFacts {
        LinuxFacts { gnome_without_tray: false, powerdevil_ddc: false, os_release_ids: None, command: "tvc".into() }
    }

    fn ids(h: &[Hint]) -> Vec<&str> {
        h.iter().map(|h| h.id).collect()
    }

    #[test]
    fn linux_hints_follow_the_failure() {
        assert!(linux_hints(None, true, &facts()).is_empty());
        assert_eq!(ids(&linux_hints(Some(LinkFailure::PermissionDenied), true, &facts())), ["linux-i2c-permission"]);
        assert_eq!(ids(&linux_hints(Some(LinkFailure::ModuleMissing), true, &facts())), ["linux-i2c-module"]);
        assert!(linux_hints(Some(LinkFailure::NotFound), true, &facts()).is_empty());
    }

    #[test]
    fn linux_hints_in_order() {
        let f = LinuxFacts { gnome_without_tray: true, powerdevil_ddc: true, ..facts() };
        let h = linux_hints(Some(LinkFailure::PermissionDenied), false, &f);
        assert_eq!(
            ids(&h),
            ["linux-i2c-permission", "linux-powerdevil", "linux-gnome-appindicator", "linux-hotkeys"]
        );
        assert!(h[0].command.starts_with("echo 'KERNEL==\"i2c-[0-9]*\", SUBSYSTEM==\"i2c-dev\""));
        assert!(h[0].command.ends_with("\nsudo udevadm control --reload && sudo udevadm trigger"));
        assert!(h[1].command.contains("printf '[Service]\\nEnvironment=POWERDEVIL_NO_DDCUTIL=1\\n' > ~/.config"));
        assert!(h[2].command.starts_with("sudo dnf install"));
    }

    #[test]
    fn appindicator_on_ubuntu_like() {
        let f = LinuxFacts { gnome_without_tray: true, os_release_ids: Some("pop ubuntu debian".into()), ..facts() };
        assert_eq!(linux_hints(None, true, &f)[0].command, "gnome-extensions enable ubuntu-appindicators@ubuntu.com");
    }

    #[test]
    fn hotkey_hint_lists_eleven_commands() {
        let h = linux_hints(None, false, &facts());
        let lines: Vec<&str> = h[0].command.lines().collect();
        assert_eq!(lines.len(), 11);
        assert_eq!(lines[0], "tvc --step brightness=+5    # Brightness up");
        assert_eq!(lines[10], "tvc    # Open the panel");
    }

    #[test]
    fn mac_hint_only_for_unsupported_port() {
        assert!(mac_hints(None).is_empty());
        assert!(mac_hints(Some(LinkFailure::NotFound)).is_empty());
        let h = mac_hints(Some(LinkFailure::UnsupportedPort));
        assert_eq!(h[0].id, "macos-unsupported-port");
        assert!(h[0].command.is_empty());
    }

    #[test]
    fn helpers() {
        assert!(is_gnome(Some("ubuntu:GNOME")));
        assert!(!is_gnome(Some("KDE")));
        assert!(!is_gnome(None));
        assert_eq!(command_name(Some("/opt/a b/tvc")), "'/opt/a b/tvc'");
        assert_eq!(command_name(Some("/usr/bin/tvc")), "/usr/bin/tvc");
        assert_eq!(command_name(None), "thinkvision-control");
        assert!(is_powerdevil_process("org_kde_powerdevil\n", ""));
        assert!(is_powerdevil_process("org_kde_powerde", "/usr/lib/org_kde_powerdevil\0"));
        assert!(!is_powerdevil_process("org_kde_powerde", "other"));
        assert!(has_ddc_disabled("A=1\0POWERDEVIL_NO_DDCUTIL=1\0"));
        assert!(!has_ddc_disabled("POWERDEVIL_NO_DDCUTIL=0\0"));
        assert!(!has_ddc_disabled("POWERDEVIL_NO_DDCUTIL=\0"));
        assert!(!has_ddc_disabled("X=1"));
    }
}
