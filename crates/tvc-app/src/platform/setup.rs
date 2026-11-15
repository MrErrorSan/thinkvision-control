//! "System setup" cards. Windows has none.

// Most of this module runs on Linux only.
#![cfg_attr(not(target_os = "linux"), allow(dead_code))]

use tvc_core::i18n::{tr, trf};
use tvc_core::link::LinkFailure;

use super::is_gnome;

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
    /// The button's text ("Allow access"), translated.
    pub label: String,
    pub steps: Vec<Step>,
    /// Shown when the check after a successful run still finds the problem, translated.
    pub still: String,
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
fn script(lines: &str) -> String {
    format!("set -e\n{lines}")
}

/// Single-quoted for sh.
fn sh_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', r"'\''"))
}

/// Looks the Linux facts up again now, ignoring the cache (after a fix ran). The cards change once it's done.
pub fn recheck() {
    #[cfg(target_os = "linux")]
    live::refresh();
}

/// The cards for the current link failure (`backlight`: of a built-in panel) and hotkey state. On Linux they also
/// depend on the desktop, running processes (PowerDevil, other DDC tools, a polkit agent), loaded kernel modules,
/// `/etc/os-release` and whether a tray host is on the session bus. Those are looked up on a worker thread and
/// cached for 30 s, so this never blocks; until the first lookup ends they count as absent.
pub fn hints(failure: Option<LinkFailure>, backlight: bool, hotkeys_ok: bool) -> Vec<Hint> {
    #[cfg(target_os = "linux")]
    {
        linux_hints(failure, backlight, hotkeys_ok, &live::cached_facts())
    }
    #[cfg(target_os = "macos")]
    {
        let _ = (backlight, hotkeys_ok); // Carbon hotkeys are always available; no backlight backend.
        mac_hints(failure)
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    {
        let _ = (failure, backlight, hotkeys_ok);
        Vec::new()
    }
}

fn hint(id: &'static str, title: &str, message: &str, command: &str) -> Hint {
    Hint { id, title: title.into(), message: message.into(), command: command.into(), fix: None }
}

/// A card with a button; its copy text is the fix's.
fn fix_hint(id: &'static str, title: &str, message: &str, fix: Fix) -> Hint {
    Hint { command: fix.command(), fix: Some(fix), ..hint(id, title, message, "") }
}

/// The packaged udev rule file's rules, without its comments.
fn udev_rules() -> String {
    include_str!("../../../../packaging/linux/60-thinkvision-i2c.rules")
        .lines()
        .filter(|l| !l.trim().is_empty() && !l.starts_with('#'))
        .collect::<Vec<_>>()
        .join("\n")
}

/// The root lines that install the packaged udev rule and apply it to the present devices.
fn install_rules() -> String {
    format!(
        "cat > /etc/udev/rules.d/60-thinkvision-i2c.rules <<'TVC_RULES'\n{}\nTVC_RULES\n\
         udevadm control --reload && udevadm trigger --action=add --subsystem-match=i2c-dev \
         --subsystem-match=backlight",
        udev_rules()
    )
}

/// A package manager the AppIndicator card installs with, found on PATH.
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
const MANAGERS: [(&str, PackageManager); 6] = [
    ("apt-get", PackageManager::Apt),
    ("dnf", PackageManager::Dnf),
    ("zypper", PackageManager::Zypper),
    ("pacman", PackageManager::Pacman),
    ("xbps-install", PackageManager::Xbps),
    ("apk", PackageManager::Apk),
];

/// The first manager whose executable `has` reports.
fn pick_manager(has: impl Fn(&str) -> bool) -> Option<PackageManager> {
    MANAGERS.iter().find(|(exe, _)| has(exe)).map(|(_, pm)| *pm)
}

/// The distribution's own manager by `/etc/os-release` `ID` and `ID_LIKE` (space-separated) when `has` finds its
/// executable, else the first one on PATH.
fn pick_manager_for(ids: Option<&str>, has: impl Fn(&str) -> bool) -> Option<PackageManager> {
    let by_os = ids.unwrap_or_default().split_whitespace().find_map(|id| match id.to_ascii_lowercase().as_str() {
        "debian" | "ubuntu" => Some(PackageManager::Apt),
        "fedora" | "rhel" | "centos" => Some(PackageManager::Dnf),
        "suse" | "opensuse" => Some(PackageManager::Zypper),
        id if id.starts_with("opensuse") || id.starts_with("sles") => Some(PackageManager::Zypper),
        "arch" => Some(PackageManager::Pacman),
        "void" => Some(PackageManager::Xbps),
        "alpine" => Some(PackageManager::Apk),
        _ => None,
    });
    let exe = |pm: PackageManager| MANAGERS.iter().find(|(_, p)| *p == pm).map_or("", |(exe, _)| exe);
    by_os.filter(|pm| has(exe(*pm))).or_else(|| pick_manager(has))
}

/// The AppIndicator extension's package. Debian, Fedora and Arch use this name; openSUSE, Void and Alpine are
/// assumed to.
const APPINDICATOR_PACKAGE: &str = "gnome-shell-extension-appindicator";

/// A non-interactive install of `package`, run as root.
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

/// Turns the extension on. Just after an install GNOME Shell doesn't know it until the next login, so
/// `gnome-extensions enable` fails; then the uuid goes into `enabled-extensions` directly, which the next login
/// reads, and the step still fails so the card says to log in again.
fn enable_extension(uuid: &str) -> String {
    format!(
        "if ! gnome-extensions enable {uuid} 2>/dev/null; then\n\
         gsettings set org.gnome.shell disable-user-extensions false\n\
         list=$(gsettings get org.gnome.shell enabled-extensions)\n\
         case \"$list\" in *\"'{uuid}'\"*) ;; \"@as []\"|\"[]\") list=\"['{uuid}']\" ;; \
         *) list=\"${{list%]}}, '{uuid}']\" ;; esac\n\
         gsettings set org.gnome.shell enabled-extensions \"$list\"\n\
         false\n\
         fi"
    )
}

const APPINDICATOR_UUID: &str = "appindicatorsupport@rgcjonas.gmail.com";
const UBUNTU_APPINDICATOR_UUID: &str = "ubuntu-appindicators@ubuntu.com";

#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn mac_hints(failure: Option<LinkFailure>) -> Vec<Hint> {
    if failure != Some(LinkFailure::UnsupportedPort) {
        return Vec::new();
    }
    vec![hint(
        "macos-unsupported-port",
        &tr("Use a USB-C or Thunderbolt port for this monitor"),
        &tr("The built-in HDMI port on M1 and base M2 Macs doesn't pass monitor controls through. \
         Connect the monitor with USB-C or Thunderbolt (or a USB-C to DisplayPort cable) instead."),
        "",
    )]
}

/// What the Linux cards depend on besides the failure, gathered by `live::cached_facts`.
#[derive(Clone)]
struct LinuxFacts {
    /// `XDG_CURRENT_DESKTOP`.
    desktop: Option<String>,
    /// No StatusNotifier host on the session bus (false when unknown).
    tray_host_missing: bool,
    powerdevil_ddc: bool,
    /// `ID` and `ID_LIKE` from /etc/os-release, space-separated.
    os_release_ids: Option<String>,
    package_manager: Option<PackageManager>,
    /// Ubuntu's AppIndicator extension is installed.
    ubuntu_appindicator: bool,
    /// NVIDIA's driver is loaded and a connected monitor has no DDC bus.
    nvidia_without_ddc: bool,
    /// The NVIDIA adapters couldn't be opened to find the monitor's bus (no udev rule yet).
    nvidia_denied: bool,
    /// The ddcci kernel driver is loaded.
    ddcci_driver: bool,
    /// Another program that drives the monitor over DDC/CI.
    other_ddc: Option<&'static str>,
    /// A known polkit agent runs (false: none found, though an unknown one may run).
    polkit_agent: bool,
    command: String,
}

fn linux_hints(failure: Option<LinkFailure>, backlight: bool, hotkeys_ok: bool, facts: &LinuxFacts) -> Vec<Hint> {
    let mut hints = Vec::new();
    // On NVIDIA's driver a bus the user may not open is never bound, so no link reports the denial itself.
    let nvidia_denied = facts.nvidia_without_ddc && facts.nvidia_denied;
    let denied = failure == Some(LinkFailure::PermissionDenied) || nvidia_denied;
    if denied && backlight && !nvidia_denied {
        hints.push(fix_hint(
            "linux-backlight-permission",
            &tr("Allow brightness control for the built-in display"),
            &tr("Your user can't change the built-in display's brightness, and the login manager didn't do it for the \
             app either. Allow access to add your user to the video group, which the app's udev rule lets write \
             the brightness."),
            Fix {
                label: tr("Allow access"),
                steps: vec![Step::Root(format!(
                    "{}\nusermod -aG video \"$(id -nu \"${{PKEXEC_UID:-$SUDO_UID}}\")\"",
                    install_rules()
                ))],
                still: tr("Your user is in the video group now; log out and back in to use it."),
            },
        ));
    } else if denied {
        hints.push(fix_hint(
            "linux-i2c-permission",
            &tr("Allow access to monitor controls"),
            &tr("Your user can't open the monitor's I²C bus. Allow access to install a udev rule that opens it to your \
             login session (your distribution's ddcutil package ships an equivalent rule)."),
            Fix {
                label: tr("Allow access"),
                steps: vec![Step::Root(install_rules())],
                still: tr("Access is set up; log out and back in if the monitor still isn't found."),
            },
        ));
    }
    if failure == Some(LinkFailure::ModuleMissing) {
        hints.push(fix_hint(
            "linux-i2c-module",
            &tr("Load the i2c-dev kernel module"),
            &tr("There are no /dev/i2c-* devices, so the i2c-dev module isn't loaded. Load it now and at every boot."),
            Fix {
                label: tr("Load the module"),
                steps: vec![Step::Root("modprobe i2c-dev\necho i2c-dev > /etc/modules-load.d/i2c-dev.conf".into())],
                still: tr("The module is loaded; restart the computer if the monitor still isn't found."),
            },
        ));
    }
    if facts.nvidia_without_ddc && !nvidia_denied {
        hints.push(hint(
            "linux-nvidia",
            &tr("Turn on monitor controls in the NVIDIA driver"),
            &tr("NVIDIA's driver isn't offering this monitor's control channel. Check that kernel modesetting is on (the \
             nvidia-drm.modeset=1 kernel option, the default on recent drivers). If the monitor still can't be \
             reached, add the driver option below and restart the computer (on some systems, rebuild the \
             initramfs first)."),
            "echo 'options nvidia NVreg_RegistryDwords=RMUseSwI2c=0x01;RMI2cSpeed=100' | \
             sudo tee /etc/modprobe.d/nvidia-ddc.conf",
        ));
    }
    if facts.ddcci_driver {
        hints.push(fix_hint(
            "linux-ddcci",
            &tr("The ddcci kernel driver also controls the monitor"),
            &tr("The ddcci driver (ddcci-dkms) drives the monitor's brightness itself, so its changes and this app's \
             fight each other. Unload it while you use this app; to keep it from loading at boot, uninstall it or \
             blacklist it in /etc/modprobe.d."),
            Fix {
                label: tr("Unload the driver"),
                steps: vec![Step::Root("modprobe -r ddcci_backlight ddcci".into())],
                still: tr("The driver is unloaded; restart the computer if it comes back."),
            },
        ));
    }
    if facts.powerdevil_ddc {
        hints.push(fix_hint(
            "linux-powerdevil",
            &tr("KDE's power manager also controls monitor brightness"),
            &tr("PowerDevil talks to the monitor over DDC/CI too, so the two fight over the bus and brightness changes \
             don't stick. Turn off its DDC control (the same override `systemctl --user edit \
             plasma-powerdevil.service` makes); laptop screen brightness keeps working."),
            Fix {
                label: tr("Turn off PowerDevil's DDC"),
                steps: vec![Step::User(
                    "mkdir -p ~/.config/systemd/user/plasma-powerdevil.service.d\n\
                     printf '[Service]\\nEnvironment=POWERDEVIL_NO_DDCUTIL=1\\n' > \
                     ~/.config/systemd/user/plasma-powerdevil.service.d/override.conf\n\
                     mkdir -p ~/.config/plasma-workspace/env\n\
                     printf 'export POWERDEVIL_NO_DDCUTIL=1\\n' > \
                     ~/.config/plasma-workspace/env/thinkvision-no-ddcutil.sh\n\
                     systemctl --user daemon-reload && systemctl --user restart plasma-powerdevil.service"
                        .into(),
                )],
                still: tr("PowerDevil's DDC control is off; log out and back in if brightness changes still don't stick."),
            },
        ));
    }
    if let Some(other) = facts.other_ddc {
        hints.push(hint(
            "linux-other-ddc",
            &tr("Another app also controls the monitor"),
            &trf(
                "{0} talks to the monitor over DDC/CI too, so the two can fight over the bus and changes may not \
                 stick. Quit it, or turn off its monitor control, while you use this app.",
                &[&other],
            ),
            "",
        ));
    }
    if facts.tray_host_missing {
        if is_gnome(facts.desktop.as_deref()) {
            hints.push(appindicator_hint(facts));
        } else {
            hints.push(tray_host_hint(facts.desktop.as_deref()));
        }
    }
    if !hotkeys_ok {
        hints.push(hint(
            "linux-hotkeys",
            &tr("Set desktop shortcuts"),
            &tr("This desktop doesn't let apps register global shortcuts. Add these commands as custom shortcuts instead: \
             GNOME: Settings → Keyboard → Custom Shortcuts; KDE: System Settings → Keyboard → Shortcuts → Add New → \
             Command or Script; COSMIC: Settings → Keyboard → Custom shortcuts."),
            &shortcut_commands(&facts.command),
        ));
    }
    if !facts.polkit_agent {
        for h in hints.iter_mut().filter(|h| h.fix.as_ref().is_some_and(|f| f.root_script().is_some())) {
            h.message = trf(
                "{0} No password prompt seems to be running on this desktop; if none appears, run the command in a \
                 terminal instead.",
                &[&h.message],
            );
        }
    }
    hints
}

/// Ubuntu ships the extension, so only turning it on (installing it first when it was removed); elsewhere installing
/// it with the package manager found, or (none known) saying what to install.
fn appindicator_hint(facts: &LinuxFacts) -> Hint {
    const ID: &str = "linux-gnome-appindicator";
    let title = tr("Install the AppIndicator extension to see the tray icon");
    let ubuntu = facts.os_release_ids.as_deref().is_some_and(|ids| ids.to_ascii_lowercase().contains("ubuntu"));
    if ubuntu && facts.ubuntu_appindicator {
        return fix_hint(
            ID,
            &title,
            &tr("GNOME shows tray icons only through the AppIndicator extension. Ubuntu ships it; turn it back on."),
            Fix {
                label: tr("Turn on"),
                steps: vec![Step::UserAfterLogin(enable_extension(UBUNTU_APPINDICATOR_UUID))],
                still: tr("The extension is turned on; log out and back in if the tray icon doesn't appear."),
            },
        );
    }
    let uuid = if ubuntu { UBUNTU_APPINDICATOR_UUID } else { APPINDICATOR_UUID };
    let Some(pm) = facts.package_manager else {
        return hint(
            ID,
            &title,
            &tr("GNOME shows tray icons only through the AppIndicator extension. Install it with your package manager \
             (usually as gnome-shell-extension-appindicator) or from extensions.gnome.org, log out and back in, then \
             turn it on."),
            &format!("gnome-extensions enable {uuid}"),
        );
    };
    fix_hint(
        ID,
        &title,
        &tr("GNOME shows tray icons only through the AppIndicator extension. Install it and turn it on."),
        Fix {
            label: tr("Install and turn on"),
            steps: vec![Step::Root(install_command(pm, APPINDICATOR_PACKAGE)), Step::UserAfterLogin(enable_extension(uuid))],
            still: tr("The extension is installed and turned on; log out and back in to see the tray icon."),
        },
    )
}

/// The card for a desktop other than GNOME with no tray host on the session bus.
fn tray_host_hint(desktop: Option<&str>) -> Hint {
    let has = |name: &str| desktop.is_some_and(|d| d.split(':').any(|p| p.to_ascii_lowercase().contains(name)));
    let how = if has("cinnamon") {
        tr("Add the XApp Status applet to a panel, and check that xapp-sn-watcher is running.")
    } else if has("xfce") {
        tr("Add the Status Tray plugin to a panel (Panel → Panel Preferences → Items).")
    } else if has("kde") {
        tr("Add the System Tray widget to a panel.")
    } else if has("sway") || has("hyprland") || has("wlroots") || has("river") {
        tr("Use a bar with a tray, such as Waybar with its tray module.")
    } else {
        tr("Add a system tray (status notifier) to your panel.")
    };
    hint(
        "linux-tray-host",
        &tr("Add a system tray to see the tray icon"),
        &trf(
            "This desktop has no tray for app icons right now, so the app keeps its window open instead of hiding in \
             the tray. {0}",
            &[&how],
        ),
        "",
    )
}

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

/// The command to bind: the app's full path, single-quoted if it contains a space.
fn command_name(exe: Option<&str>) -> String {
    match exe {
        None | Some("") => "thinkvision-control".into(),
        Some(p) if p.contains(' ') => format!("'{p}'"),
        Some(p) => p.into(),
    }
}

/// PowerDevil's comm is cut to 15 characters by the kernel ("org_kde_powerde"), so the cmdline confirms it.
fn is_powerdevil_process(comm: &str, cmdline: &str) -> bool {
    let comm = comm.trim();
    comm == "org_kde_powerdevil" || (comm == "org_kde_powerde" && cmdline.contains("org_kde_powerdevil"))
}

/// Whether a NUL-separated environ sets `POWERDEVIL_NO_DDCUTIL` to a non-zero value.
fn has_ddc_disabled(environ: &str) -> bool {
    environ
        .split('\0')
        .find_map(|e| e.strip_prefix("POWERDEVIL_NO_DDCUTIL="))
        .is_some_and(|v| !v.is_empty() && v != "0")
}

/// Whether a process's `/proc/<pid>/maps` has libddcutil loaded (PowerDevil built with DDC support).
fn maps_use_ddcutil(maps: &str) -> bool {
    maps.lines().any(|l| l.rsplit('/').next().is_some_and(|f| f.starts_with("libddcutil.so")))
}

/// A polkit authentication agent, by the file name of its executable (`/proc/<pid>/cmdline`'s first word).
fn is_polkit_agent(exe_name: &str) -> bool {
    const AGENTS: [&str; 15] = [
        "gnome-shell",
        "cinnamon",
        "polkit-kde-authentication-agent-1",
        "polkit-gnome-authentication-agent-1",
        "polkit-mate-authentication-agent-1",
        "polkit-efl-authentication-agent-1",
        "lxqt-policykit-agent",
        "lxpolkit",
        "xfce-polkit",
        "hyprpolkitagent",
        "budgie-polkit-dialog",
        "cosmic-osd",
        "soteria",
        "ukui-polkit",
        "mate-polkit",
    ];
    AGENTS.contains(&exe_name)
}

/// Another program that drives DDC/CI, by its process name (comm, at most 15 characters).
fn other_ddc_program(comm: &str) -> Option<&'static str> {
    match comm.trim() {
        "vdu_controls" => Some("VDU Controls"),
        "ddcui" => Some("ddcui"),
        _ => None,
    }
}

#[cfg(target_os = "linux")]
mod live {
    use super::LinuxFacts;
    use crate::platform::dbus;
    use std::fs;
    use std::path::Path;
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
            desktop: None,
            tray_host_missing: false,
            powerdevil_ddc: false,
            os_release_ids: None,
            package_manager: None,
            ubuntu_appindicator: false,
            nvidia_without_ddc: false,
            nvidia_denied: false,
            ddcci_driver: false,
            other_ddc: None,
            polkit_agent: true,
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
        let os_release_ids = os_release_ids();
        let procs = scan_processes();
        let gnome = super::is_gnome(desktop.as_deref());
        let other_ddc = procs
            .other_ddc
            .or_else(|| ddcutil_service().then_some("ddcutil-service"))
            .or_else(|| (gnome && gnome_ddc_extension()).then_some("The \"Brightness control using ddcutil\" extension"));
        LinuxFacts {
            tray_host_missing: tray_host_missing(),
            desktop,
            powerdevil_ddc: procs.powerdevil_ddc,
            package_manager: super::pick_manager_for(os_release_ids.as_deref(), on_path),
            os_release_ids,
            ubuntu_appindicator: Path::new("/usr/share/gnome-shell/extensions")
                .join(super::UBUNTU_APPINDICATOR_UUID)
                .is_dir(),
            nvidia_without_ddc: tvc_ddc::linux::nvidia_without_ddc(),
            nvidia_denied: tvc_ddc::linux::nvidia_access_denied(),
            ddcci_driver: ["ddcci", "ddcci_backlight"].iter().any(|m| Path::new("/sys/module").join(m).exists()),
            other_ddc,
            polkit_agent: procs.polkit_agent,
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
        let exe = crate::platform::autostart::exe_path().map(|p| p.display().to_string());
        super::command_name(exe.as_deref())
    }

    /// What one pass over /proc found. Another user's processes (unreadable environ or maps) count only by name.
    struct Processes {
        /// A PowerDevil of ours runs without `POWERDEVIL_NO_DDCUTIL`, with libddcutil loaded (or maps unreadable).
        powerdevil_ddc: bool,
        other_ddc: Option<&'static str>,
        polkit_agent: bool,
    }

    fn scan_processes() -> Processes {
        let mut found = Processes { powerdevil_ddc: false, other_ddc: None, polkit_agent: false };
        let Ok(dirs) = fs::read_dir("/proc") else { return Processes { polkit_agent: true, ..found } };
        for dir in dirs.flatten() {
            let name = dir.file_name();
            if !name.to_string_lossy().bytes().all(|b| b.is_ascii_digit()) {
                continue;
            }
            let path = dir.path();
            let Ok(comm) = fs::read_to_string(path.join("comm")) else { continue };
            let cmdline = String::from_utf8_lossy(&fs::read(path.join("cmdline")).unwrap_or_default()).into_owned();
            let exe_name = cmdline.split('\0').next().unwrap_or_default().rsplit('/').next().unwrap_or_default();
            found.polkit_agent |= super::is_polkit_agent(exe_name) || super::is_polkit_agent(comm.trim());
            found.other_ddc = found.other_ddc.or_else(|| super::other_ddc_program(&comm));
            if !found.powerdevil_ddc && super::is_powerdevil_process(&comm, &cmdline) {
                if let Ok(environ) = fs::read(path.join("environ")) {
                    let maps = fs::read_to_string(path.join("maps")).ok();
                    found.powerdevil_ddc = !super::has_ddc_disabled(&String::from_utf8_lossy(&environ))
                        && maps.is_none_or(|m| super::maps_use_ddcutil(&m));
                }
            }
        }
        found
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

    /// No `org.kde.StatusNotifierWatcher` on the session bus: asked through zbus, else through `gdbus` (1 s
    /// timeout). Unknown → false.
    fn tray_host_missing() -> bool {
        if let Some(present) = dbus::tray_host_present() {
            return !present;
        }
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

    /// ddcutil's D-Bus service (used by vdu_controls and the ddcutil GNOME extension) is running.
    fn ddcutil_service() -> bool {
        dbus::session().and_then(|bus| dbus::name_has_owner(&bus, "com.ddcutil.DdcutilService")) == Some(true)
    }

    /// GNOME's "Brightness control using ddcutil" extension is turned on.
    fn gnome_ddc_extension() -> bool {
        Command::new("gsettings")
            .args(["get", "org.gnome.shell", "enabled-extensions"])
            .stderr(Stdio::null())
            .output()
            .is_ok_and(|o| String::from_utf8_lossy(&o.stdout).contains("display-brightness-ddcutil@themightydeity.github.com"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn facts() -> LinuxFacts {
        LinuxFacts {
            desktop: None,
            tray_host_missing: false,
            powerdevil_ddc: false,
            os_release_ids: None,
            package_manager: None,
            ubuntu_appindicator: false,
            nvidia_without_ddc: false,
            nvidia_denied: false,
            ddcci_driver: false,
            other_ddc: None,
            polkit_agent: true,
            command: "tvc".into(),
        }
    }

    /// A GNOME session without a tray host.
    fn gnome_without_tray() -> LinuxFacts {
        LinuxFacts { desktop: Some("ubuntu:GNOME".into()), tray_host_missing: true, ..facts() }
    }

    fn ids(h: &[Hint]) -> Vec<&str> {
        h.iter().map(|h| h.id).collect()
    }

    #[test]
    fn linux_hints_follow_the_failure() {
        assert!(linux_hints(None, false, true, &facts()).is_empty());
        assert_eq!(ids(&linux_hints(Some(LinkFailure::PermissionDenied), false, true, &facts())), ["linux-i2c-permission"]);
        assert_eq!(
            ids(&linux_hints(Some(LinkFailure::PermissionDenied), true, true, &facts())),
            ["linux-backlight-permission"]
        );
        assert_eq!(ids(&linux_hints(Some(LinkFailure::ModuleMissing), false, true, &facts())), ["linux-i2c-module"]);
        assert!(linux_hints(Some(LinkFailure::NotFound), false, true, &facts()).is_empty());
    }

    #[test]
    fn linux_hints_in_order() {
        let f = LinuxFacts {
            powerdevil_ddc: true,
            package_manager: Some(PackageManager::Dnf),
            os_release_ids: Some("fedora".into()),
            ..gnome_without_tray()
        };
        let h = linux_hints(Some(LinkFailure::PermissionDenied), false, false, &f);
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
        assert!(h[1].command.contains("printf 'export POWERDEVIL_NO_DDCUTIL=1\\n' > ~/.config/plasma-workspace/env/"));
        assert_eq!(
            h[2].command,
            format!(
                "sudo sh -c 'set -e\ndnf install -y gnome-shell-extension-appindicator'\n{}",
                enable_extension(APPINDICATOR_UUID)
            )
        );
        let labels: Vec<_> = h.iter().map(|h| h.fix.as_ref().map(|f| f.label.as_str())).collect();
        assert_eq!(
            labels,
            [Some("Allow access"), Some("Turn off PowerDevil's DDC"), Some("Install and turn on"), None]
        );
    }

    #[test]
    fn driver_and_other_app_cards() {
        let f = LinuxFacts { nvidia_without_ddc: true, ddcci_driver: true, other_ddc: Some("ddcui"), ..facts() };
        let h = linux_hints(None, false, true, &f);
        assert_eq!(ids(&h), ["linux-nvidia", "linux-ddcci", "linux-other-ddc"]);
        assert!(h[0].fix.is_none());
        assert!(h[0].command.contains("NVreg_RegistryDwords=RMUseSwI2c=0x01;RMI2cSpeed=100"));
        assert_eq!(h[1].fix.as_ref().unwrap().root_script().as_deref(), Some("set -e\nmodprobe -r ddcci_backlight ddcci"));
        assert!(h[2].message.starts_with("ddcui talks to the monitor"));
        // NVIDIA adapters refused for permission: the access card, not the driver one.
        let f = LinuxFacts { nvidia_without_ddc: true, nvidia_denied: true, ..facts() };
        assert_eq!(ids(&linux_hints(None, true, true, &f)), ["linux-i2c-permission"]);
    }

    #[test]
    fn backlight_fix_adds_the_user_to_video() {
        let h = linux_hints(Some(LinkFailure::PermissionDenied), true, true, &facts());
        let script = h[0].fix.as_ref().unwrap().root_script().unwrap();
        assert!(script.contains("\nTVC_RULES\nudevadm control --reload"));
        assert!(script.ends_with("\nusermod -aG video \"$(id -nu \"${PKEXEC_UID:-$SUDO_UID}\")\""));
    }

    #[test]
    fn no_polkit_agent_points_to_the_terminal() {
        let f = LinuxFacts { polkit_agent: false, powerdevil_ddc: true, ..facts() };
        let h = linux_hints(Some(LinkFailure::ModuleMissing), false, true, &f);
        assert!(h[0].message.ends_with("run the command in a terminal instead."));
        // A user-only fix needs no password.
        assert!(!h[1].message.contains("password prompt"));
    }

    #[test]
    fn tray_host_card_per_desktop() {
        let h = |d: &str| linux_hints(None, false, true, &LinuxFacts { desktop: Some(d.into()), tray_host_missing: true, ..facts() });
        assert_eq!(ids(&h("X-Cinnamon")), ["linux-tray-host"]);
        assert!(h("X-Cinnamon")[0].message.contains("XApp Status applet"));
        assert!(h("XFCE")[0].message.contains("Status Tray plugin"));
        assert!(h("sway")[0].message.contains("Waybar"));
        assert!(h("Unknown")[0].message.contains("system tray"));
        assert_eq!(ids(&h("GNOME")), ["linux-gnome-appindicator"]);
    }

    #[test]
    fn udev_fix_writes_every_packaged_rule() {
        let rules = udev_rules();
        assert_eq!(rules.lines().count(), 3);
        assert!(!rules.contains('#') && !rules.contains('\''));
        let h = linux_hints(Some(LinkFailure::PermissionDenied), false, true, &facts());
        let script = h[0].fix.as_ref().unwrap().root_script().unwrap();
        assert!(script.starts_with("set -e\ncat > /etc/udev/rules.d/60-thinkvision-i2c.rules <<'TVC_RULES'\n"));
        assert!(script.contains(&format!("\n{rules}\nTVC_RULES\nudevadm control --reload")));
    }

    #[test]
    fn module_fix_is_one_root_script() {
        let h = linux_hints(Some(LinkFailure::ModuleMissing), false, true, &facts());
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
        let h = linux_hints(None, false, true, &LinuxFacts { powerdevil_ddc: true, ..facts() });
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
            os_release_ids: Some("pop ubuntu debian".into()),
            package_manager: Some(PackageManager::Apt),
            ubuntu_appindicator: true,
            ..gnome_without_tray()
        };
        let h = &linux_hints(None, false, true, &f)[0];
        assert_eq!(h.command, enable_extension(UBUNTU_APPINDICATOR_UUID));
        let fix = h.fix.as_ref().unwrap();
        assert_eq!(fix.label, "Turn on");
        assert_eq!(fix.root_script(), None);
        assert_eq!(fix.user_scripts(), [(format!("set -e\n{}", enable_extension(UBUNTU_APPINDICATOR_UUID)), true)]);
    }

    #[test]
    fn appindicator_on_ubuntu_without_the_extension_installs_it() {
        let f = LinuxFacts {
            os_release_ids: Some("ubuntu debian".into()),
            package_manager: Some(PackageManager::Apt),
            ..gnome_without_tray()
        };
        let fix = linux_hints(None, false, true, &f)[0].fix.clone().unwrap();
        assert_eq!(fix.label, "Install and turn on");
        assert!(fix.root_script().unwrap().contains("apt-get install -y gnome-shell-extension-appindicator"));
        assert!(fix.user_scripts()[0].0.contains("gnome-extensions enable ubuntu-appindicators@ubuntu.com"));
    }

    #[test]
    fn appindicator_without_a_package_manager_has_no_button() {
        let h = &linux_hints(None, false, true, &gnome_without_tray())[0];
        assert!(h.fix.is_none());
        assert!(h.message.contains("gnome-shell-extension-appindicator"));
        assert_eq!(h.command, "gnome-extensions enable appindicatorsupport@rgcjonas.gmail.com");
    }

    #[test]
    fn appindicator_enable_waits_for_login() {
        let f = LinuxFacts { package_manager: Some(PackageManager::Pacman), ..gnome_without_tray() };
        let fix = linux_hints(None, false, true, &f)[0].fix.clone().unwrap();
        assert_eq!(
            fix.root_script().as_deref(),
            Some("set -e\npacman -S --needed --noconfirm gnome-shell-extension-appindicator")
        );
        assert_eq!(fix.user_scripts(), [(format!("set -e\n{}", enable_extension(APPINDICATOR_UUID)), true)]);
    }

    #[test]
    fn enabling_falls_back_to_gsettings() {
        let s = enable_extension("a@b");
        assert!(s.starts_with("if ! gnome-extensions enable a@b 2>/dev/null; then\n"));
        assert!(s.contains("\"@as []\"|\"[]\") list=\"['a@b']\""));
        assert!(s.contains("*) list=\"${list%]}, 'a@b']\""));
        assert!(s.contains("\ngsettings set org.gnome.shell enabled-extensions \"$list\"\nfalse\nfi"));
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
    fn package_manager_prefers_the_distribution() {
        let pick = |ids: Option<&str>, names: &[&str]| pick_manager_for(ids, |exe| names.contains(&exe));
        // An Arch system that also has apt-get on PATH.
        assert_eq!(pick(Some("arch"), &["apt-get", "pacman"]), Some(PackageManager::Pacman));
        assert_eq!(pick(Some("opensuse-tumbleweed suse opensuse"), &["apt-get", "zypper"]), Some(PackageManager::Zypper));
        assert_eq!(pick(Some("pop ubuntu debian"), &["dnf", "apt-get"]), Some(PackageManager::Apt));
        // The distribution's own manager missing (Fedora Atomic has no dnf install): the PATH order.
        assert_eq!(pick(Some("fedora"), &["pacman"]), Some(PackageManager::Pacman));
        assert_eq!(pick(None, &["zypper"]), Some(PackageManager::Zypper));
        assert_eq!(pick(Some("fedora"), &[]), None);
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
    }

    #[test]
    fn copy_text_matches_the_steps() {
        let fix = Fix {
            label: "x".into(),
            steps: vec![
                Step::Root("a".into()),
                Step::User("b".into()),
                Step::Root("it's".into()),
                Step::UserAfterLogin("c".into()),
            ],
            still: String::new(),
        };
        assert_eq!(fix.root_script().as_deref(), Some("set -e\na\nit's"));
        assert_eq!(fix.user_scripts(), [("set -e\nb".to_string(), false), ("set -e\nc".to_string(), true)]);
        assert_eq!(fix.command(), "sudo sh -c 'set -e\na\nit'\\''s'\nb\nc");
        assert_eq!(sh_quote("a'b"), r"'a'\''b'");
    }

    #[test]
    fn hotkey_hint_lists_eleven_commands() {
        let h = linux_hints(None, false, false, &facts());
        assert!(h[0].fix.is_none());
        assert!(h[0].message.contains("Add New → Command or Script") && h[0].message.contains("COSMIC"));
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
        let maps = "7f00-7f01 r-xp 00000000 08:01 123 /usr/lib/x86_64-linux-gnu/libddcutil.so.5.1.2\n\
                    7f02-7f03 r-xp 00000000 08:01 124 /usr/lib/libc.so.6\n";
        assert!(maps_use_ddcutil(maps));
        assert!(!maps_use_ddcutil("7f02-7f03 r-xp 00000000 08:01 124 /usr/lib/libc.so.6\n"));
        assert!(is_polkit_agent("polkit-kde-authentication-agent-1"));
        assert!(is_polkit_agent("gnome-shell"));
        assert!(!is_polkit_agent("bash"));
        assert_eq!(other_ddc_program("vdu_controls\n"), Some("VDU Controls"));
        assert_eq!(other_ddc_program("ddcutil"), None);
    }
}
