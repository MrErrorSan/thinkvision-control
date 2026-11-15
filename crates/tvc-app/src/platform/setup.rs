//! "System setup" cards. Windows has none.

use tvc_core::link::LinkFailure;

/// One setup card. `command` is the copyable text (lines separated by `\n`); empty when there is nothing to copy.
/// With a `fix`, `command` is [`Fix::command`]: what the button runs, to type by hand.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Hint {
    pub id: &'static str,
    pub title: String,
    pub message: String,
    pub command: String,
    pub fix: Option<Fix>,
}

/// A card's button. The root steps run first, together under one password prompt, then the user steps in order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Fix {
    /// The button's text ("Allow access").
    pub label: &'static str,
    pub steps: Vec<Step>,
    /// Shown when the check after a successful run still finds the problem.
    pub still: &'static str,
}

/// Shell lines of a fix, built from constants and the exe path only.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Step {
    /// Run as root through `pkexec`.
    Root(String),
    /// Run as the user.
    User(String),
    /// Run as the user; failing means it works after the next login (`gnome-extensions enable` just after an
    /// install).
    UserAfterLogin(String),
}

#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
impl Fix {
    /// Every root step in one script for `sh -c`; None without root steps.
    pub fn root_script(&self) -> Option<String> {
        let lines: Vec<&str> =
            self.steps.iter().filter_map(|s| if let Step::Root(l) = s { Some(l.as_str()) } else { None }).collect();
        (!lines.is_empty()).then(|| script(&lines.join("\n")))
    }

    /// The user steps in order: the script for `sh -c`, and whether a failure means "after the next login".
    pub fn user_scripts(&self) -> Vec<(String, bool)> {
        self.steps
            .iter()
            .filter_map(|s| match s {
                Step::Root(_) => None,
                Step::User(l) => Some((script(l), false)),
                Step::UserAfterLogin(l) => Some((script(l), true)),
            })
            .collect()
    }

    /// The same steps to run in a terminal: the root script under `sudo sh -c`, then the user lines.
    pub fn command(&self) -> String {
        let root = self.root_script().map(|s| format!("sudo sh -c {}", sh_quote(&s)));
        let user = self.steps.iter().filter_map(|s| match s {
            Step::Root(_) => None,
            Step::User(l) | Step::UserAfterLogin(l) => Some(l.clone()),
        });
        root.into_iter().chain(user).collect::<Vec<_>>().join("\n")
    }
}

/// `set -e`, then the lines.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn script(lines: &str) -> String {
    format!("set -e\n{lines}")
}

/// Single-quoted for sh.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn sh_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', r"'\''"))
}

/// Looks the Linux facts up again now, ignoring the cache (after a fix ran). The cards change once it's done.
pub fn recheck() {
    #[cfg(target_os = "linux")]
    live::refresh();
}

/// The cards for the current link failure and hotkey state. On Linux they also depend on the desktop, a running
/// PowerDevil, `/etc/os-release` and (on GNOME only) whether a tray host is on the session bus. Those are looked
/// up on a worker thread and cached for 30 s, so this never blocks; until the first lookup ends they count as absent.
pub fn hints(failure: Option<LinkFailure>, hotkeys_ok: bool) -> Vec<Hint> {
    #[cfg(target_os = "linux")]
    {
        linux_hints(failure, hotkeys_ok, &live::cached_facts())
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
    Hint { id, title: title.into(), message: message.into(), command: command.into(), fix: None }
}

/// A card with a button; its copy text is the fix's.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn fix_hint(id: &'static str, title: &str, message: &str, fix: Fix) -> Hint {
    Hint { command: fix.command(), fix: Some(fix), ..hint(id, title, message, "") }
}

/// The packaged udev rule file's rules, without its comments.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn udev_rules() -> String {
    include_str!("../../../../packaging/linux/60-thinkvision-i2c.rules")
        .lines()
        .filter(|l| !l.trim().is_empty() && !l.starts_with('#'))
        .collect::<Vec<_>>()
        .join("\n")
}

/// A package manager the AppIndicator card installs with, found on PATH.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PackageManager {
    Apt,
    Dnf,
    Zypper,
    Pacman,
    Xbps,
    Apk,
}

/// Each manager's executable, in the order they're looked for.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
const MANAGERS: [(&str, PackageManager); 6] = [
    ("apt-get", PackageManager::Apt),
    ("dnf", PackageManager::Dnf),
    ("zypper", PackageManager::Zypper),
    ("pacman", PackageManager::Pacman),
    ("xbps-install", PackageManager::Xbps),
    ("apk", PackageManager::Apk),
];

/// The first manager whose executable `has` reports.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn pick_manager(has: impl Fn(&str) -> bool) -> Option<PackageManager> {
    MANAGERS.iter().find(|(exe, _)| has(exe)).map(|(_, pm)| *pm)
}

/// The AppIndicator extension's package. Debian, Fedora and Arch use this name; openSUSE, Void and Alpine are
/// assumed to.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn appindicator_package(pm: PackageManager) -> &'static str {
    match pm {
        PackageManager::Apt
        | PackageManager::Dnf
        | PackageManager::Zypper
        | PackageManager::Pacman
        | PackageManager::Xbps
        | PackageManager::Apk => "gnome-shell-extension-appindicator",
    }
}

/// A non-interactive install of `package`, run as root.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn install_command(pm: PackageManager, package: &str) -> String {
    match pm {
        PackageManager::Apt => format!("apt-get update -q && DEBIAN_FRONTEND=noninteractive apt-get install -y {package}"),
        PackageManager::Dnf => format!("dnf install -y {package}"),
        PackageManager::Zypper => format!("zypper --non-interactive install {package}"),
        PackageManager::Pacman => format!("pacman -S --needed --noconfirm {package}"),
        PackageManager::Xbps => format!("xbps-install -Sy {package}"),
        PackageManager::Apk => format!("apk add {package}"),
    }
}

#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
const APPINDICATOR_UUID: &str = "appindicatorsupport@rgcjonas.gmail.com";
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
const UBUNTU_APPINDICATOR_UUID: &str = "ubuntu-appindicators@ubuntu.com";

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

/// What the Linux cards depend on besides the failure, gathered by `live::cached_facts`.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
#[derive(Clone)]
struct LinuxFacts {
    gnome_without_tray: bool,
    powerdevil_ddc: bool,
    /// `ID` and `ID_LIKE` from /etc/os-release, space-separated.
    os_release_ids: Option<String>,
    package_manager: Option<PackageManager>,
    command: String,
}

#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn linux_hints(failure: Option<LinkFailure>, hotkeys_ok: bool, facts: &LinuxFacts) -> Vec<Hint> {
    let mut hints = Vec::new();
    if failure == Some(LinkFailure::PermissionDenied) {
        hints.push(fix_hint(
            "linux-i2c-permission",
            "Allow access to monitor controls",
            "Your user can't open the monitor's I²C bus. Allow access to install a udev rule that opens it to your \
             login session (your distribution's ddcutil package ships an equivalent rule).",
            Fix {
                label: "Allow access",
                steps: vec![Step::Root(format!(
                    "cat > /etc/udev/rules.d/60-thinkvision-i2c.rules <<'TVC_RULES'\n{}\nTVC_RULES\n\
                     udevadm control --reload && udevadm trigger --action=add --subsystem-match=i2c-dev \
                     --subsystem-match=backlight",
                    udev_rules()
                ))],
                still: "Access is set up; log out and back in if the monitor still isn't found.",
            },
        ));
    }
    if failure == Some(LinkFailure::ModuleMissing) {
        hints.push(fix_hint(
            "linux-i2c-module",
            "Load the i2c-dev kernel module",
            "There are no /dev/i2c-* devices, so the i2c-dev module isn't loaded. Load it now and at every boot.",
            Fix {
                label: "Load the module",
                steps: vec![Step::Root("modprobe i2c-dev\necho i2c-dev > /etc/modules-load.d/i2c-dev.conf".into())],
                still: "The module is loaded; restart the computer if the monitor still isn't found.",
            },
        ));
    }
    if facts.powerdevil_ddc {
        hints.push(fix_hint(
            "linux-powerdevil",
            "KDE's power manager also controls monitor brightness",
            "PowerDevil talks to the monitor over DDC/CI too, so the two fight over the bus and brightness changes \
             don't stick. Turn off its DDC control (the same override `systemctl --user edit \
             plasma-powerdevil.service` makes); laptop screen brightness keeps working.",
            Fix {
                label: "Turn off PowerDevil's DDC",
                steps: vec![Step::User(
                    "mkdir -p ~/.config/systemd/user/plasma-powerdevil.service.d\n\
                     printf '[Service]\nEnvironment=POWERDEVIL_NO_DDCUTIL=1\n' > \
                     ~/.config/systemd/user/plasma-powerdevil.service.d/override.conf\n\
                     systemctl --user daemon-reload && systemctl --user restart plasma-powerdevil.service"
                        .into(),
                )],
                still: "PowerDevil's DDC control is off; log out and back in if brightness changes still don't stick.",
            },
        ));
    }
    if facts.gnome_without_tray {
        hints.push(appindicator_hint(facts));
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

/// Ubuntu ships the extension, so only turning it on; elsewhere installing it with the package manager found, or
/// (none known) saying what to install.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn appindicator_hint(facts: &LinuxFacts) -> Hint {
    const ID: &str = "linux-gnome-appindicator";
    const TITLE: &str = "Install the AppIndicator extension to see the tray icon";
    let ubuntu = facts.os_release_ids.as_deref().is_some_and(|ids| ids.to_ascii_lowercase().contains("ubuntu"));
    if ubuntu {
        return fix_hint(
            ID,
            TITLE,
            "GNOME shows tray icons only through the AppIndicator extension. Ubuntu ships it; turn it back on.",
            Fix {
                label: "Turn on",
                steps: vec![Step::UserAfterLogin(format!("gnome-extensions enable {UBUNTU_APPINDICATOR_UUID}"))],
                still: "The extension is turned on; log out and back in if the tray icon doesn't appear.",
            },
        );
    }
    let Some(pm) = facts.package_manager else {
        return hint(
            ID,
            TITLE,
            "GNOME shows tray icons only through the AppIndicator extension. Install it with your package manager \
             (usually as gnome-shell-extension-appindicator) or from extensions.gnome.org, log out and back in, then \
             turn it on.",
            &format!("gnome-extensions enable {APPINDICATOR_UUID}"),
        );
    };
    fix_hint(
        ID,
        TITLE,
        "GNOME shows tray icons only through the AppIndicator extension. Install it and turn it on.",
        Fix {
            label: "Install and turn on",
            steps: vec![
                Step::Root(install_command(pm, appindicator_package(pm))),
                Step::UserAfterLogin(format!("gnome-extensions enable {APPINDICATOR_UUID}")),
            ],
            still: "The extension is installed; log out and back in to turn it on.",
        },
    )
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
    use super::LinuxFacts;
    use std::fs;
    use std::process::{Command, Stdio};
    use std::sync::Mutex;
    use std::time::{Duration, Instant};

    struct Cache {
        facts: Option<LinuxFacts>,
        at: Option<Instant>,
        busy: bool,
        /// Bumped by `refresh`, so a lookup started before it doesn't overwrite the newer one.
        generation: u64,
    }

    static CACHE: Mutex<Cache> = Mutex::new(Cache { facts: None, at: None, busy: false, generation: 0 });

    /// The last facts looked up (none yet: all absent), starting a lookup on a worker thread when they are missing
    /// or older than 30 s.
    pub(super) fn cached_facts() -> LinuxFacts {
        let mut cache = CACHE.lock().unwrap_or_else(|e| e.into_inner());
        if !cache.busy && cache.at.is_none_or(|at| at.elapsed() > Duration::from_secs(30)) {
            start(&mut cache);
        }
        cache.facts.clone().unwrap_or_else(|| LinuxFacts {
            gnome_without_tray: false,
            powerdevil_ddc: false,
            os_release_ids: None,
            package_manager: None,
            command: command(),
        })
    }

    /// Starts a lookup now, even while one runs.
    pub(super) fn refresh() {
        let mut cache = CACHE.lock().unwrap_or_else(|e| e.into_inner());
        cache.generation += 1;
        start(&mut cache);
    }

    fn start(cache: &mut Cache) {
        let generation = cache.generation;
        let spawned = std::thread::Builder::new().name("tvc-setup-facts".into()).spawn(move || {
            let facts = facts();
            let mut cache = CACHE.lock().unwrap_or_else(|e| e.into_inner());
            if cache.generation == generation {
                *cache = Cache { facts: Some(facts), at: Some(Instant::now()), busy: false, generation };
            }
        });
        cache.busy = spawned.is_ok();
    }

    fn facts() -> LinuxFacts {
        let desktop = std::env::var("XDG_CURRENT_DESKTOP").ok();
        LinuxFacts {
            gnome_without_tray: super::is_gnome(desktop.as_deref()) && tray_host_missing(),
            powerdevil_ddc: powerdevil_ddc_active(),
            os_release_ids: os_release_ids(),
            package_manager: super::pick_manager(on_path),
            command: command(),
        }
    }

    /// An executable of this name is on PATH (or in the usual system folders, for a session with a short PATH).
    fn on_path(name: &str) -> bool {
        use std::os::unix::fs::PermissionsExt;
        let path = std::env::var_os("PATH").unwrap_or_default();
        std::env::split_paths(&path)
            .chain(["/usr/bin", "/usr/sbin", "/bin", "/sbin"].map(std::path::PathBuf::from))
            .any(|dir| fs::metadata(dir.join(name)).is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0))
    }

    fn command() -> String {
        super::command_name(std::env::current_exe().ok().map(|p| p.display().to_string()).as_deref())
    }

    /// True when a PowerDevil process of ours runs without `POWERDEVIL_NO_DDCUTIL`.
    fn powerdevil_ddc_active() -> bool {
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

    fn os_release_ids() -> Option<String> {
        let text = fs::read_to_string("/etc/os-release").ok()?;
        let ids: Vec<&str> = text
            .lines()
            .filter_map(|l| l.strip_prefix("ID=").or_else(|| l.strip_prefix("ID_LIKE=")))
            .map(|v| v.trim_matches(|c| c == '"' || c == '\''))
            .collect();
        Some(ids.join(" "))
    }

    /// No `org.kde.StatusNotifierWatcher` on the session bus (asked through `gdbus`, 1 s timeout). Unknown → false.
    fn tray_host_missing() -> bool {
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
        LinuxFacts {
            gnome_without_tray: false,
            powerdevil_ddc: false,
            os_release_ids: None,
            package_manager: None,
            command: "tvc".into(),
        }
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
        let f = LinuxFacts {
            gnome_without_tray: true,
            powerdevil_ddc: true,
            package_manager: Some(PackageManager::Dnf),
            ..facts()
        };
        let h = linux_hints(Some(LinkFailure::PermissionDenied), false, &f);
        assert_eq!(
            ids(&h),
            ["linux-i2c-permission", "linux-powerdevil", "linux-gnome-appindicator", "linux-hotkeys"]
        );
        assert!(h[0].command.starts_with("sudo sh -c 'set -e\ncat > /etc/udev/rules.d/60-thinkvision-i2c.rules"));
        assert!(h[0].command.contains("\nKERNEL==\"i2c-[0-9]*\", SUBSYSTEM==\"i2c-dev\", ATTRS{class}==\"0x03*\""));
        assert!(h[0].command.ends_with(
            "\nudevadm control --reload && udevadm trigger --action=add --subsystem-match=i2c-dev \
             --subsystem-match=backlight'"
        ));
        assert!(h[1].command.contains("printf '[Service]\\nEnvironment=POWERDEVIL_NO_DDCUTIL=1\\n' > ~/.config"));
        assert_eq!(
            h[2].command,
            "sudo sh -c 'set -e\ndnf install -y gnome-shell-extension-appindicator'\n\
             gnome-extensions enable appindicatorsupport@rgcjonas.gmail.com"
        );
        let labels: Vec<_> = h.iter().map(|h| h.fix.as_ref().map(|f| f.label)).collect();
        assert_eq!(
            labels,
            [Some("Allow access"), Some("Turn off PowerDevil's DDC"), Some("Install and turn on"), None]
        );
    }

    #[test]
    fn udev_fix_writes_every_packaged_rule() {
        let rules = udev_rules();
        assert_eq!(rules.lines().count(), 3);
        assert!(!rules.contains('#') && !rules.contains('\''));
        let h = linux_hints(Some(LinkFailure::PermissionDenied), true, &facts());
        let script = h[0].fix.as_ref().unwrap().root_script().unwrap();
        assert!(script.starts_with("set -e\ncat > /etc/udev/rules.d/60-thinkvision-i2c.rules <<'TVC_RULES'\n"));
        assert!(script.contains(&format!("\n{rules}\nTVC_RULES\nudevadm control --reload")));
    }

    #[test]
    fn module_fix_is_one_root_script() {
        let h = linux_hints(Some(LinkFailure::ModuleMissing), true, &facts());
        let fix = h[0].fix.as_ref().unwrap();
        assert_eq!(
            fix.root_script().as_deref(),
            Some("set -e\nmodprobe i2c-dev\necho i2c-dev > /etc/modules-load.d/i2c-dev.conf")
        );
        assert!(fix.user_scripts().is_empty());
        assert_eq!(
            h[0].command,
            "sudo sh -c 'set -e\nmodprobe i2c-dev\necho i2c-dev > /etc/modules-load.d/i2c-dev.conf'"
        );
    }

    #[test]
    fn powerdevil_fix_runs_as_the_user() {
        let h = linux_hints(None, true, &LinuxFacts { powerdevil_ddc: true, ..facts() });
        let fix = h[0].fix.as_ref().unwrap();
        assert_eq!(fix.root_script(), None);
        let user = fix.user_scripts();
        assert_eq!(user.len(), 1);
        assert!(!user[0].1);
        assert_eq!(user[0].0, format!("set -e\n{}", h[0].command));
    }

    #[test]
    fn appindicator_on_ubuntu_like() {
        let f = LinuxFacts {
            gnome_without_tray: true,
            os_release_ids: Some("pop ubuntu debian".into()),
            package_manager: Some(PackageManager::Apt),
            ..facts()
        };
        let h = &linux_hints(None, true, &f)[0];
        assert_eq!(h.command, "gnome-extensions enable ubuntu-appindicators@ubuntu.com");
        let fix = h.fix.as_ref().unwrap();
        assert_eq!(fix.label, "Turn on");
        assert_eq!(fix.root_script(), None);
        assert_eq!(
            fix.user_scripts(),
            [("set -e\ngnome-extensions enable ubuntu-appindicators@ubuntu.com".to_string(), true)]
        );
    }

    #[test]
    fn appindicator_without_a_package_manager_has_no_button() {
        let h = &linux_hints(None, true, &LinuxFacts { gnome_without_tray: true, ..facts() })[0];
        assert!(h.fix.is_none());
        assert!(h.message.contains("gnome-shell-extension-appindicator"));
        assert_eq!(h.command, "gnome-extensions enable appindicatorsupport@rgcjonas.gmail.com");
    }

    #[test]
    fn appindicator_enable_waits_for_login() {
        let f = LinuxFacts { gnome_without_tray: true, package_manager: Some(PackageManager::Pacman), ..facts() };
        let fix = linux_hints(None, true, &f)[0].fix.clone().unwrap();
        assert_eq!(
            fix.root_script().as_deref(),
            Some("set -e\npacman -S --needed --noconfirm gnome-shell-extension-appindicator")
        );
        assert_eq!(
            fix.user_scripts(),
            [("set -e\ngnome-extensions enable appindicatorsupport@rgcjonas.gmail.com".to_string(), true)]
        );
    }

    #[test]
    fn package_manager_pick_follows_the_order() {
        let pick = |names: &[&str]| pick_manager(|exe| names.contains(&exe));
        assert_eq!(pick(&[]), None);
        assert_eq!(pick(&["apk"]), Some(PackageManager::Apk));
        assert_eq!(pick(&["pacman", "dnf"]), Some(PackageManager::Dnf));
        assert_eq!(pick(&["zypper", "apt-get", "xbps-install"]), Some(PackageManager::Apt));
        assert_eq!(pick(&["xbps-install", "apk"]), Some(PackageManager::Xbps));
        assert_eq!(pick(&["rpm", "zypper"]), Some(PackageManager::Zypper));
    }

    #[test]
    fn install_commands_never_ask() {
        let p = "pkg";
        assert_eq!(install_command(PackageManager::Apt, p), "apt-get update -q && DEBIAN_FRONTEND=noninteractive apt-get install -y pkg");
        assert_eq!(install_command(PackageManager::Dnf, p), "dnf install -y pkg");
        assert_eq!(install_command(PackageManager::Zypper, p), "zypper --non-interactive install pkg");
        assert_eq!(install_command(PackageManager::Pacman, p), "pacman -S --needed --noconfirm pkg");
        assert_eq!(install_command(PackageManager::Xbps, p), "xbps-install -Sy pkg");
        assert_eq!(install_command(PackageManager::Apk, p), "apk add pkg");
        for (_, pm) in MANAGERS {
            assert_eq!(appindicator_package(pm), "gnome-shell-extension-appindicator");
        }
    }

    #[test]
    fn copy_text_matches_the_steps() {
        let fix = Fix {
            label: "x",
            steps: vec![
                Step::Root("a".into()),
                Step::User("b".into()),
                Step::Root("it's".into()),
                Step::UserAfterLogin("c".into()),
            ],
            still: "",
        };
        assert_eq!(fix.root_script().as_deref(), Some("set -e\na\nit's"));
        assert_eq!(fix.user_scripts(), [("set -e\nb".to_string(), false), ("set -e\nc".to_string(), true)]);
        assert_eq!(fix.command(), "sudo sh -c 'set -e\na\nit'\\''s'\nb\nc");
        assert_eq!(sh_quote("a'b"), r"'a'\''b'");
    }

    #[test]
    fn hotkey_hint_lists_eleven_commands() {
        let h = linux_hints(None, false, &facts());
        assert!(h[0].fix.is_none());
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
