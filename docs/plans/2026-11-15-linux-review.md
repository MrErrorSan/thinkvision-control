# Linux compliance review: ThinkVision Control 0.1.0

Branch `release/0.1.0` at a942335. I read the code only; nothing ran on Linux. Line numbers are from HEAD unless marked *(wt)*, which means the working tree. Another agent is editing `setup.rs`, `controller.rs`, `view.rs` and `ui/*` right now, so *(wt)* lines will move. Function names are given to anchor them.

Vendored sources checked: `~/.cargo/registry/src/*/` for slint 1.18.1 (`i-slint-core`, `i-slint-backend-winit`), ksni 0.3.7 and winit 0.30.13. `cargo tree --target x86_64-unknown-linux-gnu` was run offline for the dependency facts.

**Severity:** H = a core feature is broken or invisible on a default install of a target distro. M = it works badly, misleads the user, or breaks on a common setup. L = polish, or a narrow case.

**Assumption** marks a claim I couldn't confirm from the code or the vendored source. Each one becomes a test step in (c).

---

## (a) Findings

### Status after the code fixes

Implemented on `release/0.1.0` after b93dedc; checked with clippy for Linux, Windows and macOS only, so every item still needs its test in (c). Every fallback only adds to what 0.0.8 did: a connector that already has a bus is never probed or changed, and a missing D-Bus, logind or portal falls back to the 0.0.8 behaviour (sysfs write, `gdbus`, the OSD window, the tray made at once).

| ID | Status | Note |
|---|---|---|
| LNX-01 | Done | `linux/discovery.rs`: with `/sys/module/nvidia` loaded, a connected external connector without a bus is matched to an unclaimed `NVIDIA i2c adapter` on its GPU by reading 128 bytes at 0x50 under the flock and comparing them with the sysfs EDID; cached per connector and EDID (misses too, unless a read was refused for permission). `linux-nvidia` card (copyable modprobe option, no button), and the "can't be reached"/"No supported display" text points to it. When the adapters can't be opened for permission, the I²C access card shows instead. |
| LNX-02 | Done | `controller.rs` `tray_host_ready`: the tray is created once `org.kde.StatusNotifierWatcher` has an owner (zbus `NameHasOwner`, polled every 2 s); after 60 s the window opens. Closing the window with no tray and no host exits instead of hiding (not minimise: GNOME's "Quit" would only minimise it again). The host check runs on every desktop, with a `linux-tray-host` card per desktop outside GNOME. |
| LNX-03 | Done | `backlight/sysfs.rs`: the sysfs write first; on EACCES, logind `Session.SetBrightness` on `/org/freedesktop/login1/session/auto`, checked at open by writing the current value. New `linux-backlight-permission` card (rule plus `usermod -aG video`, says to log in again) instead of the I²C one. |
| LNX-04 | Done | Backlights named `ddcci*` or under an `/i2c-` device are skipped; a `raw` one needs a built-in connector. `linux-ddcci` card when the module is loaded, with an "Unload the driver" button. |
| LNX-05 | Done | X11: the OSD is override-redirect with `_NET_WM_WINDOW_TYPE_NOTIFICATION`, placed again once its native window exists. Wayland: Plasma's `org.kde.osdService.showText`, else a transient `org.freedesktop.Notifications` notification that replaces the last one; the window only when neither is on the bus or a call fails. A layer-shell OSD is still a later wave. |
| LNX-06 | Done | `slint::set_xdg_app_id("thinkvision-control")` right after `select_backend` (Slint needs the platform first); `StartupNotify`, `StartupWMClass` and `SingleMainWindow` in the .desktop. |
| LNX-07 | Done | `taskbar.rs`: the portal's `color-scheme` (read, then followed through `SettingChanged`) picks the glyph; GNOME keeps the white one. |
| LNX-08 | Done | `clipboard.rs`: arboard (the clipboard Slint's winit backend uses), kept for the process; the tools remain the fallback, and "Couldn't copy" shows on failure. |
| LNX-09 | Partial | Card text: Plasma 6 path and COSMIC; the command uses the installed path without " (deleted)". The GlobalShortcuts portal is still a recommendation. |
| LNX-10 | Done | PowerDevil counts only with libddcutil in `/proc/<pid>/maps` (unreadable maps count as before); the fix also writes `~/.config/plasma-workspace/env/thinkvision-no-ddcutil.sh`. `linux-other-ddc` card for `com.ddcutil.DdcutilService`, vdu_controls, ddcui and the ddcutil GNOME extension. |
| LNX-11 | Done | zbus is a direct dependency (already in the build through ksni). logind signals, the session path, `NameHasOwner`, `SetBrightness` and the portal go through it; the `gdbus` code stays as the fallback and logs once when it's missing too. |
| LNX-12 | Done | `i2c.rs` `open_fresh`: EACCES on a node changed in the last 10 s is retried for up to 2 s before `PermissionDenied`. |
| LNX-13 | Partial | A later launch leaves its `XDG_ACTIVATION_TOKEN` in `$XDG_RUNTIME_DIR/thinkvision-control.activation` (the wire format is unchanged, so 0.0.8 copies still parse requests); the next main window is created with it. An already open window can't take a token in winit, so it gets `request_user_attention` instead. |
| LNX-14 | Partial | Done: `/usr/bin/pkexec`; a `/proc` scan for known polkit agents adds "run the command in a terminal" to root fixes when none is found; `enabled-extensions` through `gsettings` when `gnome-extensions enable` must wait for a login; Ubuntu without its extension installs it; the package manager by `/etc/os-release` first. Not done: (3) polkit outside a logind session, (4) root-password wording in the app's cards. |
| LNX-15 | Done | README lists the libraries; `libudev-dev` dropped from `ci.yml` and the README. Still in `release.yml:58` (outside this change). `RUST_FONTCONFIG_DLOPEN` not set. |
| LNX-16 | Done | README: glibc 2.35+. |
| LNX-17 | Done | `install.sh` (in `scripts/make-linux-tarball.sh`) runs one root script with sudo, else pkexec, else prints a `su -c` line; it no longer says to log out. `uninstall.sh` likewise. README names root's password for openSUSE and Debian. |
| LNX-18 | Done | Without `xdg-mime`, `install.sh` adds the handler under `[Default Applications]` in `mimeapps.list`; it runs `kbuildsycoca6` when present. |
| LNX-19 | Not done | Packaging (.deb, .rpm, AUR, OBS) is a separate decision; the recommendation stands. |
| LNX-20 to 25 | Not done | Need a Linux machine (A4). |

### A1. Fix now in code (no Linux machine needed)

| ID | Sev | Scope | Finding and evidence | Fix |
|---|---|---|---|---|
| LNX-01 | **H** | Every distro on NVIDIA proprietary. This includes hybrid ThinkPad P/X laptops, where the dGPU drives the external ports | **No DDC on NVIDIA, and the message blames the cable.** `ddc_bus` finds a bus only through the connector's `ddc` link or an `i2c-N` child folder (`linux/discovery.rs:61-69`). nvidia-drm creates neither; its adapters (`NVIDIA i2c adapter N at X:00.0`) hang off the PCI device. So `backend = None` (`linux/mod.rs:77`). With `nvidia-drm.modeset=1` (the default on recent drivers and needed for Wayland; assumption: 560+), the monitor is listed but uncontrollable. The UI then says "*P32p-20 can't be reached … Try another cable or port*" (`view.rs:264-271` *(wt)*). Without modeset there are no connectors, so `connectors()` returns None and the result is `ListError::Unsupported` (`discovery.rs:36-37, 93`). The UI then says "*No supported display found … Connect a display that supports DDC/CI*" (`view.rs:255-262`). Neither case gets a setup card. | 1) Fallback mapping: when a connected external connector has no bus and `/sys/module/nvidia` exists, list the `/dev/i2c-*` adapters whose `/sys/bus/i2c/devices/i2c-N/name` starts with `NVIDIA i2c adapter` and whose parent is the connector's card's PCI device. Read 128 bytes at 0x50 (offset 0, under the flock), compare them with the sysfs EDID, then bind. This is what ddcutil does. Cache it per EDID hash so it doesn't rescan on every enumerate (DDC-12). 2) Add a `linux-nvidia` card when nvidia is loaded and no connector has a bus. It names `nvidia-drm.modeset=1` and ddcutil's `options nvidia NVreg_RegistryDwords=RMUseSwI2c=0x01;RMI2cSpeed=100` (assumption: still needed on 5xx drivers). 3) When the result is `Unsupported` and nvidia is loaded, show that card instead of "Connect a display". |
| LNX-02 | **H** | All DEs. Hits hardest at autostart (GNOME, KDE) and on DEs with no SNI host | **The tray icon is lost for good if the StatusNotifierWatcher isn't up when the tray is created.** ksni defaults to `assume_sni_available: false` (`ksni/src/blocking.rs:106`), so a missing watcher makes `spawn()` fail (`ksni/src/service.rs:88-104`). Slint only logs `debug_log!("Failed to create system tray icon")` and returns. It never retries (`i-slint-core/items/system_tray.rs:303-312`, `system_tray/ksni.rs:263`). `Tray::new()` succeeds anyway, so the app's own "could not create" log never fires (`controller.rs` `ensure_tray`, 1224 *(wt)*). At login, autostart can beat gnome-shell's AppIndicator extension or plasmashell. The result is a `--startup` copy running with no window and no icon. Only GNOME is checked for a missing host (`setup.rs:225`, `is_gnome`). | On Linux, create `Tray` only once `org.kde.StatusNotifierWatcher` has an owner. Poll `NameHasOwner` every 2 s for 60 s, or watch `NameOwnerChanged`. If no host appears, open the window rather than stay hidden. When the window closes and there is no host, minimise it instead of hiding it. Run the "no tray host" check on every DE (not just GNOME), with text per DE: Cinnamon: *XApp Status* applet / `xapp-sn-watcher`; XFCE: *Status Tray* plugin; Sway/Hyprland: a bar with a tray (waybar). Upstream: ask Slint to retry or to expose `assume_sni_available`. |
| LNX-03 | **H** | Laptops on every distro: Ubuntu, Fedora and Arch don't put users in `video` | **Built-in panel brightness fails for a default user, and the card shown can't fix it.** The rule only does `chgrp video` + `chmod g+w` (`60-thinkvision-i2c.rules:23-24`), and new users aren't in `video` on Ubuntu, Fedora, Debian or Arch (assumption). `sysfs.rs:80-83` returns `PermissionDenied`, which raises the **I²C** card (`setup.rs` `linux_hints`, the first `if` in both versions). Its fix reinstalls the same rule (*(wt)* `Step::Root(cat > …rules)`) and its "still" text says to log out, which doesn't help either. | Write through logind: `org.freedesktop.login1.Session.SetBrightness("backlight", <name>, <raw>)` on the app's own session path. That path is already worked out in `monitor_events.rs:482-500` (`own_session_path`). It needs systemd ≥ 243 (every target) and no group membership. Keep the sysfs read, and try sysfs writes first. Give the backlight failure its own card id. If a group fallback is kept, the root step must be `usermod -aG video "$(id -nu "$PKEXEC_UID")"` and the card must say a re-login is needed. |
| LNX-04 | M | Desktops with `ddcci-driver-linux` (AUR `ddcci-dkms`, PPAs) | **A kernel ddcci backlight is treated as a laptop panel.** `pick_backlight` accepts any `raw` device (`backlight/mod.rs:107-114`). With no eDP connector, `displays()` falls back to `display_info(name, &[], "internal")` (`backlight/sysfs.rs:40-43`). The result is a phantom "built-in display" on a desktop that drives the same monitor over the driver. The driver also claims 0x37, so the app takes it with `I2C_SLAVE_FORCE` (`linux/i2c.rs:283-288`) and two masters share the bus. | Skip backlight devices whose name starts with `ddcci` or whose canonical `device` path contains `/i2c-`. Require a built-in connector before accepting a `raw` device. Add a card: "The ddcci kernel driver controls this monitor; unload it (`sudo modprobe -r ddcci_backlight ddcci`) or this app's changes will fight it". |
| LNX-05 | **H** | Every Linux session (X11 and Wayland) | **The OSD takes focus, shows in the taskbar and, on Wayland, can't be placed.** `with_active(false)` is *"X11 / Wayland: Unsupported"* (`winit-0.30.13/src/window.rs:436-438`), and the non-Windows path sets nothing else (`osd.rs:39-40`). Each show after a hide re-maps the window (`osd.rs:277-280`), so on X11 the WM focuses it and the user's typing goes to the OSD. On Wayland, `set_outer_position` is ignored, `primary_monitor()` is `None` (`winit .../wayland/output.rs:18-21`), and `always-on-top` (`background.slint:81`) is a no-op. GNOME and KWin place it as an ordinary toplevel: centred or smart-placed, focused, listed in Alt+Tab and the dash. GNOME has no layer-shell. | **X11:** in `osd::attributes`, add `WindowAttributesExtX11::with_override_redirect(true)` and `.with_x11_window_type(vec![WindowType::Notification])` (`winit .../platform/x11.rs:162-165`). The OSD then gets no focus and no taskbar entry, and lands exactly where `place` puts it. **Wayland:** don't open a toplevel. KDE: call plasmashell `org.kde.osdService` (`/org/kde/osdService`, `showText(icon, text)`; assumption: callable by any session client). Elsewhere: an `org.freedesktop.Notifications` notification with `replaces_id`, `transient` and `x-canonical-private-synchronous`, or OSD off with a setting. Don't rely on `org.gnome.Shell.ShowOSD` (assumption: restricted to allow-listed callers since GNOME 41). A layer-shell OSD for KWin, COSMIC and wlroots is possible but needs its own SCTK surface: a later wave. |
| LNX-06 | M | Every Wayland session; X11 is partly covered by WM_CLASS | **No Wayland app_id.** `slint::set_xdg_app_id` is never called (grep finds nothing). Slint passes it to winit only when it's set (`i-slint-backend-winit/winitwindowadapter.rs:582-591`). GNOME and KDE then can't match windows to `thinkvision-control.desktop`: generic icon, no grouping with the pinned launcher, a wrong name in "is ready" notifications. Portals (LNX-09) also identify host apps by this id. | Call `slint::set_xdg_app_id("thinkvision-control")` in `main` before `select_backend`. Add `StartupWMClass=thinkvision-control`, `StartupNotify=true` and `SingleMainWindow=true` to `packaging/linux/thinkvision-control.desktop`. |
| LNX-07 | M | KDE (Breeze light panel is the default), XFCE, light Cinnamon themes | **The tray glyph is always the white "on dark" one.** `taskbar::is_light()` returns `false` off Windows (`taskbar.rs:28`), and `update_tray` uses it (`controller.rs:1281` *(wt)*). On a light panel the icon is nearly invisible. `ThemeChanged` exists only on Windows (`controller.rs` `start_resident`), so nothing would update it anyway. | Slint already follows the portal's `org.freedesktop.appearance color-scheme` (`xdg_desktop_settings.rs:69-75`). In `Tray` (`background.slint`), add `property <bool> dark: Palette.color-scheme == ColorScheme.dark;` and `changed dark => { root.theme-changed(); }`, and pass `!dark` to `is_light` on Linux. GNOME's top bar is dark in both modes, so keep the white glyph when `is_gnome`. Better still, upstream: give ksni an `icon_name` with a `-symbolic` icon so KDE and GNOME recolour it. |
| LNX-08 | M | Default installs of Ubuntu, Fedora, Kubuntu and Debian (assumption: none ships wl-copy, xclip or xsel by default) | **The setup cards' Copy button silently does nothing.** `clipboard::set` shells out to `wl-copy`, `xclip` or `xsel` (`clipboard.rs:54-62`). The cards are the whole Linux recovery path (README "Troubleshooting"). | Use Slint's own clipboard, which the winit backend already provides (arboard/smithay; `arboard` is in the tree with `wayland-data-control`): put the text in a hidden `TextInput`, call `select-all()`, then `copy()`. Keep the tools as a fallback. Show "Copied" or "Couldn't copy, select the text" so a failure is visible. |
| LNX-09 | M | Wayland: GNOME and KDE | **The hotkey detection is right; a portal path is recommended.** `hotkeys.rs:49-58` returns None on `XDG_SESSION_TYPE=wayland`, on `WAYLAND_DISPLAY`, or with no `DISPLAY`. That is correct: XWayland grabs fire only while an X11 window has focus, as the code says. **Recommendation:** add the `org.freedesktop.portal.GlobalShortcuts` portal. Support: KDE Plasma 5.27+/6 yes; GNOME 48+ yes (xdg-desktop-portal-gnome 48; so Fedora 42+, Ubuntu 25.04+, **not Ubuntu 24.04**); Hyprland yes; COSMIC unknown (assumption). Use `ashpd`, or zbus directly, which is already compiled in (LNX-11). It needs LNX-06's app id, and for unsandboxed apps xdg-desktop-portal ≥ 1.19's `org.freedesktop.host.portal.Registry.Register` (assumption). Keep the card as the fallback. **Smaller fixes in the card:** the KDE path in Plasma 6 is *System Settings → Keyboard → Shortcuts → Add New → Command or Script*. Add COSMIC (*Settings → Keyboard → Custom shortcuts*). The command comes from `current_exe()` without stripping `" (deleted)"` (`setup.rs:233` HEAD, `:455` *(wt)*). After an in-place update the shown path is `'…/thinkvision-control (deleted)'`. Reuse `autostart::imp::exe_path`. | See the finding text. |
| LNX-10 | M | KDE (Plasma 5.27 and 6) | **The PowerDevil check is too broad.** `powerdevil_ddc_active` (`setup.rs:237-259` HEAD) flags any PowerDevil without the variable, even one built without libddcutil (assumption: Debian 12's 5.27 build has no ddcutil). The variable name `POWERDEVIL_NO_DDCUTIL` is right for both versions. 5.27 checks only that it is set; Plasma 6 checks for a value > 0 (assumption). `has_ddc_disabled` treats `=0` as "not disabled" (`setup.rs:179-184`), which matches 6. The systemd override applies only when Plasma starts through systemd (the default since 5.25). **Other programs that drive DDC:** `ddcutil-service` (session bus name `com.ddcutil.DdcutilService`), vdu_controls, ddcui, the GNOME extension `display-brightness-ddcutil@themightydeity.github.com`, and the ddcci kernel driver (LNX-04). GNOME itself: assumption that GNOME 49+ mutter has no DDC brightness of its own; check on Fedora 43. | Confirm libddcutil is in use: read `/proc/<pid>/maps` and look for `libddcutil.so`. Also write `~/.config/plasma-workspace/env/thinkvision-no-ddcutil.sh` (`export POWERDEVIL_NO_DDCUTIL=1`) so the setting holds without systemd boot (assumption: startplasma sources it in both boot modes). Add a "Another app controls the monitor" card when `NameHasOwner(com.ddcutil.DdcutilService)`, a `vdu_controls`/`ddcui` process, or that extension in `org.gnome.shell enabled-extensions` is found. |
| LNX-11 | M | openSUSE KDE (`gdbus` is in `glib2-tools`), minimal Debian KDE (`libglib2.0-bin`); assumption that these can be missing | **`gdbus` is a hidden runtime dependency.** Without it, the logind watch returns silently (`monitor_events.rs:462`): no suspend, resume, lock or unlock handling, so writes go to a sleeping monitor. The session lookup and the tray-host check (`setup.rs:272-286`) report "unknown". | zbus 5.19 is already in the binary (through ksni, the Slint winit backend and accesskit: `cargo tree -i zbus`). Make it a direct dependency and replace every `gdbus` call: the logind signals, `NameHasOwner`, `SetBrightness` (LNX-03) and the portal reads. At minimum, log `gdbus not found` once. |
| LNX-12 | M | Docks and hotplug, every distro | **The kernel's hotplug event arrives before udev sets the ACL.** The netlink socket joins group 1, raw kernel events (`monitor_events.rs:516`). A new `/dev/i2c-N` after a dock plug-in may not have its `uaccess` ACL yet, so the reconnect gets `EACCES` and the permission card flashes. The 1.5 s debounce probably hides this (assumption). | Join group 2 (udev events, after the rules have run; the messages have a `libudev\0` header to skip). Or retry a `PermissionDenied` once after 2 s when it follows a hotplug, before showing the card. |
| LNX-13 | M | Every Wayland session | **An open window can't be brought to the front.** `focus_window()` is a no-op on Wayland (`winit .../wayland/window/mod.rs:629`). `show_window` relies on it (`controller.rs:2006` *(wt)*). A second launch from the app menu or tray "Open" leaves the window behind others. | The second instance reads `XDG_ACTIVATION_TOKEN` (set by GNOME Shell and KWin when launching a `.desktop`) and forwards it in the wire request. The primary recreates the window with `WindowAttributesExtStartupNotify::with_activation_token` (`winit .../platform/startup_notify.rs:55`), or calls `set_activation_token_env` before showing. |
| LNX-14 | M | The new in-app fixes: every DE | **The pkexec flow** (`setup.rs` *(wt)*, `Fix`/`Step::Root`). 1) **Agent detection:** polkit has no "is an agent registered" API. Run pkexec and map the result: exit 127 with `No authentication agent found` on stderr means no agent, so show the `sudo` command (already in `Fix::command`); exit 126 means the user cancelled, so show nothing; spawn `ENOENT` means pkexec isn't installed (Debian 12 ships it as its own `pkexec` package; assumption: not pulled in by the KDE task). A pre-check can scan `/proc` for `gnome-shell`, `cinnamon` (built-in agent; assumption), `polkit-kde-authentication-agent-1`, `polkit-gnome-authentication-agent-1`, `lxqt-policykit-agent`, `polkit-mate-authentication-agent-1`, `xfce-polkit` and `hyprpolkitagent`. If none is found, lead with the terminal command. XFCE on Arch, Sway and Hyprland usually have none. 2) Use absolute `/usr/bin/pkexec /bin/sh -c`. 3) An autostarted copy runs under the systemd user manager, outside a logind session. Check that polkit still finds the session's agent (assumption). 4) openSUSE asks for the **root** password (pkexec `auth_admin` and sudo `targetpw`); say "your password or the root password". 5) **AppIndicator after install:** `gnome-extensions enable` fails before the next login (*(wt)* `Step::UserAfterLogin`), and nothing runs it again after login, so the user has to click a second time. Instead, append the uuid to `org.gnome.shell enabled-extensions` with `gsettings`: it takes effect at the next login even though the shell doesn't know the extension yet. Also check `disable-user-extensions`. 6) The Ubuntu branch assumes `ubuntu-appindicators@ubuntu.com` is installed (`setup.rs:108` HEAD). Check with `gnome-extensions info`, and fall back to `apt-get install gnome-shell-extension-appindicator` when it's missing. 7) The package manager is picked by probing PATH with apt-get first (*(wt)* `MANAGERS`). Prefer `/etc/os-release` `ID`/`ID_LIKE`. On Fedora Atomic (Silverblue) there's no dnf install; the hint text already covers having no button. | See the finding text. |

Package and uuid facts, confirmed for the card:

| Distro | Package | uuid | Default state |
|---|---|---|---|
| Ubuntu 24.04, Pop!_OS 22.04 (assumption for Pop) | `gnome-shell-extension-appindicator` (Ubuntu build) | `ubuntu-appindicators@ubuntu.com` | installed, enabled |
| Debian 12 | `gnome-shell-extension-appindicator` (apt) | `appindicatorsupport@rgcjonas.gmail.com` | not installed |
| Fedora 41+ | `gnome-shell-extension-appindicator` (dnf) | same | not installed |
| Arch | `gnome-shell-extension-appindicator` (pacman, extra) | same | not installed |
| openSUSE TW | `gnome-shell-extension-appindicator` (zypper; assumption) | same | not installed |
| KDE, COSMIC, Budgie 10.9 | native SNI host | n/a | n/a |
| Cinnamon (Mint 22) | `xapp-sn-watcher` plus the XApp Status applet | n/a | on (assumption) |

### A2. Packaging (fix now)

| ID | Sev | Scope | Finding and evidence | Fix |
|---|---|---|---|---|
| LNX-15 | L | All | **Runtime libraries.** `cargo tree` shows fontconfig is **linked**: `yeslogic-fontconfig-sys` without `dlopen`, and its build.rs only switches with `RUST_FONTCONFIG_DLOPEN`. Wayland (`wayland-sys` `dlopen`), X11 (`x11rb` `dl-libxcb`, `x11-dl`) and xkbcommon (`xkbcommon-dl`) are **dlopen'ed at run time**. So the binary needs `libfontconfig.so.1`, and at run time `libxkbcommon.so.0`, `libwayland-client.so.0`/`-cursor.so.0` (Wayland), and `libxcb.so.1`, `libX11.so.6`, `libX11-xcb.so.1`, `libXcursor.so.1`, `libXrandr.so.2`, `libXi.so.6` and `libxkbcommon-x11.so.0` (X11). Every target desktop has these (assumption). `libudev-dev` in `release.yml:58`, `ci.yml:27` and `README.md:118` is unused: no `udev` crate for the Linux target (`cargo tree -i udev`: nothing). | List the libraries in the README. Drop `libudev-dev`. Optional: build with `RUST_FONTCONFIG_DLOPEN=1` so a missing fontconfig gives an error message instead of a loader failure. |
| LNX-16 | L | All | **glibc.** The build runs on `ubuntu-22.04` (`release.yml:51`), so it needs glibc 2.35 or newer. It runs on Ubuntu 22.04+, Pop 22.04, Debian 12 (2.36), Mint 22 (2.39), Fedora 41+ (2.40+), and rolling Arch and Tumbleweed. It does not run on Debian 11 or Ubuntu 20.04, which are not targets. | Put "glibc 2.35+" in the README. |
| LNX-17 | M | Debian (root account set at install, so the user isn't in `sudo`), openSUSE | **`install.sh` relies on `sudo`** (`make-linux-tarball.sh:104-107`). On Debian with a root password, `sudo` refuses. On openSUSE it asks for the root password, while the README says "your password". `"Log out and back in"` (line 108) isn't needed for `uaccess`, since the trigger re-applies the ACL, only for `video`. | Use `pkexec` when present and `sudo` otherwise. Print `su -c '…'` when neither works. Change the README text to "your password (on openSUSE, root's)". Drop the logout line once LNX-03 removes `video`. |
| LNX-18 | L | Minimal Arch/KDE | **The URL scheme registration can be skipped silently.** `xdg-mime` and `update-desktop-database` are optional (`make-linux-tarball.sh:66-72`). Without xdg-utils there's no default handler, and nothing says so. `url_scheme::register()` does nothing on Linux (`url_scheme.rs:77-80`). | When `xdg-mime` is missing, append `x-scheme-handler/thinkvision=thinkvision-control.desktop` under `[Default Applications]` in `~/.config/mimeapps.list`. On KDE, run `kbuildsycoca6` if present. |
| LNX-19 | L | All | **Distribution formats.** The tarball works, but it needs root for the rule anyway. | **Recommended:** a `.deb` (cargo-deb) and an `.rpm` (cargo-generate-rpm) installing `/usr/bin/thinkvision-control`, `/usr/lib/udev/rules.d/60-thinkvision-i2c.rules`, `/usr/lib/modules-load.d/thinkvision-control.conf` (`i2c-dev`), `/usr/share/applications/…desktop` and the hicolor icon. The postinst runs `udevadm control --reload && udevadm trigger --subsystem-match=i2c-dev` and `modprobe i2c-dev`. No in-app pkexec is needed after that. An AUR PKGBUILD for Arch/EndeavourOS, and OBS for openSUSE. **Not recommended:** AppImage (it can't install the udev rule, and Ubuntu 24.04 has no `libfuse2` by default); Flatpak or Snap (`/dev/i2c-*` needs `--device=all` plus a host udev rule, and the tray, autostart and pkexec all run into the sandbox). |

### A3. Checked and correct (no change)

| Area | Evidence | Verdict |
|---|---|---|
| udev rule | `60-thinkvision-i2c.rules:14`: `SUBSYSTEM=="i2c-dev"`, `ATTRS{class}=="0x03*"`, `TAG+="uaccess"`. The `60-` prefix runs before `73-seat-late`. `ATTRS` walks connector → card → PCI, so DP-AUX children match. | Correct. Broader than ddcutil's rule, which matches `0x030000` only (assumption), so 3D-class (`0x0302`) and display-class (`0x0380`) GPUs are covered. Works alongside an `i2c`-group rule. `/bin/chgrp` exists on every target (merged /usr; Tumbleweed assumption). |
| i2c-dev missing | `i2c.rs:104-109`: `ENOENT` plus no `/dev/i2c-*` gives `ModuleMissing`. Discovery reads sysfs only, so it works without the module. | Correct. On kernels with `CONFIG_I2C_CHARDEV=y` the `modules-load.d` line is harmless (assumption about which distros build it in). |
| Bus locking | `i2c.rs:310-326`: `flock` with a 2 s timeout. | Works with ddcutil 2.x and libddcutil when their flock locking is on (assumption: on by default in 2.1+). |
| Autostart | `autostart.rs:207-215`: absolute, quoted `Exec`, `TryExec`, `X-GNOME-Autostart-enabled=true`. The install path is preferred and `(deleted)` is stripped (`:85-97`). | Correct for GNOME, KDE (XDG autostart, also through `systemd-xdg-autostart-generator`) and Cinnamon. Without `XDG_SESSION_ID` under the generator, `own_session_path` falls back to `User.Display` (`monitor_events.rs:482-500`), as it should. The tray race is LNX-02, not this file. |
| Single instance | `instance.rs:425-452`: `$XDG_RUNTIME_DIR` (pam_systemd sets it on every target; mode 0700), `flock` lock file, socket at 0600, `/tmp/tvc-<uid>` at 0700 with an owner check as the fallback. | Correct. L: the socket name `tvc-cmd` (`:429`) is generic; `thinkvision-control.sock` would be clearer. One instance per uid, which also covers two graphical sessions of the same user (acceptable). |
| URL scheme | `.desktop:7,12` has `Exec=… %u` and `MimeType=x-scheme-handler/thinkvision;`. `install.sh` rewrites every `Exec=` line, the actions included, to the quoted absolute path (`make-linux-tarball.sh:63`), then runs `xdg-mime default`. | Correct, apart from LNX-18. Snap Firefox on Ubuntu goes through the OpenURI portal (assumption: works with `mimeapps.list`). |
| Dark mode | Slint 1.18.1 reads `org.freedesktop.appearance color-scheme`, accent and GNOME `font-name` through the portal, and waits 500 ms for the first window (`xdg_desktop_settings.rs:17, 69-80`). | The window follows the theme live on GNOME, KDE and Cinnamon (Mint 21.2+ xdg-desktop-portal-xapp; assumption). With no portal it's light after 500 ms. Only the tray glyph is wrong (LNX-07). |
| Hotkey detection | `hotkeys.rs:49-58` | Correct (see LNX-09). The defaults avoid the virtual-console keys on Linux (`tvc-core/src/hotkey.rs:134-140`, CLI-01). |
| Setup facts | `setup.rs` scans `/proc` on a worker thread, with a 30 s cache (fixes APP-12). `XDG_CURRENT_DESKTOP` is matched case-insensitively per element (`ubuntu:GNOME`, `pop:GNOME`; `Budgie:GNOME` also counts as GNOME, which is harmless because Budgie has a watcher). | Correct. With `hidepid=2` on `/proc` only your own processes are visible, which is enough here. |

### A4. Needs testing on Linux

| ID | Sev | Scope | What to check | Expected or risk |
|---|---|---|---|---|
| LNX-20 | M | All | **Display off (DPMS) without a lock.** `SystemEvent::DisplayOff` exists only on Windows and macOS (`monitor_events.rs:23-25`). On GNOME's "blank screen" without a lock, the engine keeps writing to a sleeping monitor. `wake.rs:5` does nothing on Linux. | Poll `/sys/class/drm/*/dpms` every 3 s (assumption: atomic drivers update it). Check whether the P32p-20 NAKs in standby, and whether an input switch from standby works without a wake. |
| LNX-21 | M | Docks, MST, DisplayLink | MST connectors on i915 and amdgpu should carry a `ddc` link (`drm_connector_init_with_ddc` with `port->aux.ddc`; assumption). DisplayLink (evdi) connectors have no DDC at all and get the "try another cable" text. | If evdi is confirmed (`readlink /sys/class/drm/cardN/device/driver` → `evdi`), add the message "DisplayLink adapters don't pass monitor controls". |
| LNX-22 | L | Wayland with fractional scaling, X11 Cinnamon | HiDPI: winit uses `wp_fractional_scale_v1` on Wayland and `Xft.dpi` on X11 (assumption). Text metrics with Cantarell, Adwaita Sans and Noto Sans in a layout designed around Segoe UI. | No clipped labels at 100%, 125%, 150% or 200%. |
| LNX-23 | L | KDE | The SNI item id is fixed at `"slint-tray"` (`i-slint-core/items/system_tray/ksni.rs:41`). Every Slint app shares that id, so they share KDE's hide/show setting. | Upstream: let the id be set (e.g. from the app id). |
| LNX-24 | L | GNOME AppIndicator, Cinnamon | A left click calls SNI `Activate`, which opens the window; a right click opens the menu. Submenus (multi-display) render. | Check per host: some hosts open the menu on a left click. |
| LNX-25 | L | Cinnamon, X11 | `LockedHint` comes from cinnamon-screensaver (assumption). Without it, Locked and Unlocked never arrive. | Lock, then check the log for Locked. |

---

## (b) Verification matrix

P1 rows must pass before 0.1.0 ships Linux beyond "preview".

0.0.8 reported working on the user's Linux setup with the P32p-20 (distro and desktop to confirm). Treat that path (DRM connector, its i2c bus, DDC over i2c-dev) as known-good: 0.1.0 must not regress it.

| # | Distro | DE | Session | GPU | Focus | Pri |
|---|---|---|---|---|---|---|
| 1 | Ubuntu 24.04 LTS | GNOME 46 | Wayland | Intel or AMD | Baseline: install, udev, tray (ubuntu-appindicators), OSD focus (LNX-05), app_id (LNX-06), Copy (LNX-08), hotkey card | P1 |
| 2 | Ubuntu 24.04 LTS | GNOME 46 | X11 | Intel or AMD | X11 hotkeys, OSD placement and focus (X11 override-redirect fix) | P1 |
| 3 | Ubuntu 24.04 LTS | GNOME 46 | Wayland | NVIDIA proprietary, hybrid laptop | LNX-01 (NVIDIA buses), LNX-03 (backlight), panel + external | P1 |
| 4 | KDE neon / Kubuntu 24.04+ | Plasma 6 | Wayland | Intel or AMD | PowerDevil card and fix (LNX-10), tray glyph on a light panel (LNX-07), portal shortcuts (LNX-09) | P1 |
| 5 | KDE neon | Plasma 6 | X11 | Intel or AMD | X11 grabs vs KWin global shortcuts | P2 |
| 6 | Fedora Workstation 42+ | GNOME 48+ | Wayland | Intel or AMD | AppIndicator install via dnf + pkexec, GlobalShortcuts portal on GNOME, SELinux contexts of the written files | P1 |
| 7 | Fedora KDE 41+ | Plasma 6 | Wayland | AMD | PowerDevil with libddcutil, the flock interplay | P2 |
| 8 | Debian 12 | GNOME 43 | Wayland | Intel | AppIndicator via apt, pkexec present?, `sudo` missing for the user (LNX-17) | P2 |
| 9 | Debian 12 | Plasma 5.27 | X11 | Intel | 5.27 `POWERDEVIL_NO_DDCUTIL`, `gdbus` present? (LNX-11) | P3 |
| 10 | Linux Mint 22 | Cinnamon 6 | X11 | Intel or NVIDIA | SNI through xapp-sn-watcher, polkit agent, X11 hotkeys, LockedHint | P1 |
| 11 | EndeavourOS / Arch | Plasma 6 | Wayland | NVIDIA proprietary | LNX-01 on a desktop GPU, `xdg-mime` present? (LNX-18) | P2 |
| 12 | Arch | GNOME 48+ | Wayland | AMD | AppIndicator via pacman, ddcci-dkms installed (LNX-04) | P3 |
| 13 | openSUSE Tumbleweed | Plasma 6 | Wayland | Intel | root-password prompts, `glib2-tools`/gdbus (LNX-11), zypper | P2 |
| 14 | Pop!_OS 22.04 | GNOME 42 (COSMIC-styled) | X11 (NVIDIA ISO default) | NVIDIA proprietary | ubuntu-appindicators uuid present? NVIDIA buses | P2 |
| 15 | Pop!_OS 24.04 | COSMIC | Wayland | Intel or NVIDIA | SNI applet, OSD, hotkey card text, portal | P3 |

Extra columns on each P1 row: a USB-C dock (DP alt-mode), an MST hub with two monitors, and the P32p-20 on HDMI vs DP vs USB-C.

---

## (c) Manual test checklist

Before you start, keep `journalctl -f` and `tail -f ~/.config/ThinkVisionControl/log.txt` open in two terminals. Note the row number from (b) in each report. *Known* gives the finding that predicts a failure with today's code.

### 1. Install

| # | Step | Expected |
|---|---|---|
| 1.1 | `readelf -d thinkvision-control \| grep NEEDED` | Only `libfontconfig.so.1`, `libgcc_s.so.1`, `libm.so.6`, `libc.so.6` (and `ld-linux`). *Known* LNX-15. |
| 1.2 | `ldd --version`, then `./thinkvision-control --version` from the unpacked folder | glibc ≥ 2.35; prints the version line. |
| 1.3 | `tar xzf …tar.gz && ./thinkvision-control-*/install.sh`, answer **Y** | It asks for a password once. It writes `/etc/udev/rules.d/60-thinkvision-i2c.rules` and `/etc/modules-load.d/i2c-dev.conf`. It prints "Done". On Debian with a root account, sudo fails (*Known* LNX-17); write that down. |
| 1.4 | `ls -l /dev/i2c-*; getfacl /dev/i2c-N` (N = the monitor's bus from `ls -l /sys/class/drm/card*-DP-*/ddc`) | The nodes exist and the ACL has `user:<you>:rw-`, **without** logging out. |
| 1.5 | Run `install.sh` again | "Monitor access is already set up." |
| 1.6 | `grep -i thinkvision ~/.config/mimeapps.list; xdg-mime query default x-scheme-handler/thinkvision` | `thinkvision-control.desktop`. *Known* LNX-18 if `xdg-mime` is missing. |
| 1.7 | Open the app grid or menu and search "ThinkVision" | The entry has the app icon, and its right-click actions show Day, Night and Switch. |

### 2. First run

| # | Step | Expected |
|---|---|---|
| 2.1 | Start it from the launcher | The window opens in the desktop's light or dark theme. The dock or taskbar shows **the app's icon**, grouped with the launcher. *Known* LNX-06 on Wayland: generic icon. |
| 2.2 | With the P32p-20 on DP or USB-C | Status Connected; brightness and contrast match the OSD on the monitor. |
| 2.3 | Move the brightness slider | The monitor changes within about 0.3 s; `journalctl` shows no i2c errors. |
| 2.4 | Rows 3, 11, 14 (NVIDIA): same steps | Expected after LNX-01: connected. Today it says "can't be reached" or "No supported display". Record `ls /sys/class/drm/`, `ls -l /sys/class/drm/card*-*/ddc`, and `for b in /sys/bus/i2c/devices/i2c-*; do echo $b $(cat $b/name); done`. |

### 3. Setup cards (Settings → System setup); one button each

| # | Step | Expected |
|---|---|---|
| 3.1 | **Permission:** `sudo rm /etc/udev/rules.d/60-thinkvision-i2c.rules && sudo udevadm control --reload && sudo udevadm trigger --subsystem-match=i2c-dev`, then Try again | The "Allow access to monitor controls" card. Click **Allow access**: a polkit prompt appears, then the card disappears and the monitor connects without a logout. |
| 3.2 | Same as 3.1, but cancel the polkit prompt | No error dialog; the card stays (pkexec exit 126). |
| 3.3 | Same as 3.1 on a session with no polkit agent (e.g. `pkill -f polkit-gnome` on XFCE or Sway) | The card offers the copyable `sudo` command (pkexec exit 127). *Known* LNX-14 if it fails silently. |
| 3.4 | **Copy** on any card | The clipboard has the exact command (paste into a terminal). *Known* LNX-08 if wl-copy, xclip and xsel are all missing: `command -v wl-copy xclip xsel`. |
| 3.5 | **Module:** `sudo modprobe -r i2c_dev` (only if no other process holds it), then Try again | The "Load the i2c-dev kernel module" card. **Load the module**, then it connects. `/etc/modules-load.d/i2c-dev.conf` contains `i2c-dev`. |
| 3.6 | **AppIndicator** (rows 6, 8, 12): fresh GNOME, no extension | The "Install the AppIndicator extension" card. **Install and turn on**: one prompt, the package installs, and the message says to log in again. After the re-login the tray icon appears **without a second click**. *Known* LNX-14 (5): needs a second click today. |
| 3.7 | **AppIndicator** (rows 1, 14): `gnome-extensions disable ubuntu-appindicators@ubuntu.com` | The card with **Turn on**: the icon appears at once. |
| 3.8 | **PowerDevil** (rows 4, 7, 9): default Plasma | The "KDE's power manager also controls monitor brightness" card. **Turn off PowerDevil's DDC**: `systemctl --user show plasma-powerdevil -p Environment` lists `POWERDEVIL_NO_DDCUTIL=1`, and the card disappears within 30 s. Laptop brightness keys still work. |
| 3.9 | **PowerDevil** on a build without libddcutil: `grep ddcutil /proc/$(pgrep -f org_kde_powerdevil)/maps` is empty | No card. *Known* LNX-10 if it is shown. |
| 3.10 | **Hotkeys card** on Wayland | "Set desktop shortcuts" with 11 commands. Each starts with the absolute path `~/.local/share/thinkvision-control/thinkvision-control`. |

### 4. Tray

| # | Step | Expected |
|---|---|---|
| 4.1 | Settings → Run in background **on**, then close the window | The icon is in the top bar or panel and readable on its background. *Known* LNX-07 on light KDE or XFCE panels. |
| 4.2 | Left-click the icon | The window opens **in front**. *Known* LNX-13 on Wayland when it's already open behind others. |
| 4.3 | Right-click: Brightness up / down; Day / Night; Switch to this PC; Exit | Each acts once; Exit ends the process (`pgrep thinkvision` is empty). |
| 4.4 | Unplug the monitor | The icon gets the × badge; the tooltip says "Not connected". |
| 4.5 | Turn on Start when you log in, then log out and back in | The icon appears **every time** (try it 3 times). *Known* LNX-02: missing after some logins; check the log for a tray error. |
| 4.6 | `killall plasmashell; plasmashell &` (KDE) or disable and re-enable AppIndicator (GNOME) | The icon comes back. |

### 5. Hotkeys

| # | Step | Expected |
|---|---|---|
| 5.1 | X11 (rows 2, 5, 10): press each default in Settings → Keyboard shortcuts | Each action fires and the OSD shows. |
| 5.2 | X11 with NumLock on, and with CapsLock on | It still fires (assumption: global-hotkey grabs the lock variants). |
| 5.3 | X11: set a shortcut the DE already owns (e.g. Super+L) | It's marked as failed in the editor; nothing crashes. |
| 5.4 | Wayland: bind `…/thinkvision-control --step brightness=+5` as a custom shortcut and press it | The running copy changes the brightness, and the OSD shows (see 6.x). There's no second tray icon or window. |
| 5.5 | After a GlobalShortcuts portal is added (rows 4, 6) | The portal dialog lists the actions; once accepted, they fire with no window focused. |

### 6. OSD

| # | Step | Expected |
|---|---|---|
| 6.1 | Type in a text editor and press a brightness hotkey or shortcut mid-word | The OSD appears **and your typing stays in the editor**. *Known* LNX-05: the OSD takes focus (X11 and Wayland). |
| 6.2 | X11: check where it appears | Bottom centre of the primary monitor, 96 logical px above the bottom; not in the taskbar or Alt+Tab. |
| 6.3 | Wayland GNOME / KDE: same | After the LNX-05 fix: a shell OSD or notification, not a window. Today: a centred window that shows up in Alt+Tab. |
| 6.4 | Press it 5 times quickly | One OSD updates in place and hides 1.2 s after the last press. |

### 7. Multi-display

| # | Step | Expected |
|---|---|---|
| 7.1 | Two external monitors (one on a dock or MST hub) | Both are listed by EDID name; each slider drives only its own monitor. |
| 7.2 | Tray: "All linked displays" vs per-display submenus | They step the right monitors. |
| 7.3 | Unplug and re-plug the dock | Both reconnect within about 3 s, and there's no lasting permission card (LNX-12: note any flash). |
| 7.4 | A DisplayLink adapter, if available | Listed as not controllable, with a DisplayLink-specific message after LNX-21. |
| 7.5 | Two identical monitors | Shown as separate displays (serial or connector); a step goes to the right one. |

### 8. Built-in panel (laptops: rows 3 and any laptop)

| # | Step | Expected |
|---|---|---|
| 8.1 | `ls /sys/class/backlight; id -nG` | Note the device names and whether `video` is in your groups. |
| 8.2 | Select the built-in display and move the slider | The panel dims, and GNOME's or KDE's own brightness slider follows. *Known* LNX-03: PermissionDenied plus the wrong I²C card when you're not in `video`. |
| 8.3 | Hybrid laptop: `cat /sys/class/backlight/*/type` | The app drives the one that actually changes the panel (firmware > platform > raw). |
| 8.4 | Desktop with `ddcci` loaded (`lsmod \| grep ddcci`) | No phantom "built-in display". *Known* LNX-04. |

### 9. Autostart

| # | Step | Expected |
|---|---|---|
| 9.1 | Turn on Start when you log in, then `cat ~/.config/autostart/thinkvision-control.desktop` | `Exec="…/thinkvision-control" --startup`, `TryExec=`, `X-GNOME-Autostart-enabled=true`. |
| 9.2 | Log in again | Tray only, no window (Run in background on), and the monitor connects. `systemctl --user list-units 'app-*thinkvision*'` lists the unit on GNOME and Plasma (systemd generator). |
| 9.3 | KDE: System Settings → Autostart | The entry is listed; turning it off there removes the file. |
| 9.4 | Turn it off in the app | The file is gone. |

### 10. URL scheme

| # | Step | Expected |
|---|---|---|
| 10.1 | `xdg-open thinkvision://preset/night` with the app running | Night preset applied; no second instance. |
| 10.2 | The same with the app not running | It starts, applies the preset, shows the OSD and exits. |
| 10.3 | Click a `thinkvision://step/brightness/+10` link in Firefox (the Snap on Ubuntu) and in Chrome | A "Open with ThinkVision Control?" prompt, then the brightness rises. |

### 11. Single instance and CLI

| # | Step | Expected |
|---|---|---|
| 11.1 | Start it again from the launcher while it's running | No second process (`pgrep -c thinkvision-control` = 1); the existing window comes to the front (LNX-13 on Wayland). |
| 11.2 | `thinkvision-control --get all --json; echo $?` | JSON from the running copy; exit 0. |
| 11.3 | `ls -l $XDG_RUNTIME_DIR/tvc-cmd $XDG_RUNTIME_DIR/thinkvision-control.lock` | Socket at `srw-------`, lock at `-rw-------`, owned by you. |
| 11.4 | `kill -9` the app, then start it | It starts as the primary (stale socket removed) and shows no safe-mode banner unless the kill was during a capability read. |
| 11.5 | `env -u XDG_RUNTIME_DIR thinkvision-control --get brightness` | It uses `/tmp/tvc-$UID` (mode 700). |

### 12. Session and power

| # | Step | Expected |
|---|---|---|
| 12.1 | Lock the screen, wait 10 s, unlock | The log shows Locked and Unlocked; values refresh after unlock. |
| 12.2 | Suspend and resume | The log shows Suspending and Resumed; it reconnects within about 3 s. |
| 12.3 | `command -v gdbus` (rows 9, 13) | If it's missing, 12.1 and 12.2 log nothing. *Known* LNX-11. |
| 12.4 | Let the screen blank without a lock | Note whether writes fail while it's off (LNX-20). |

### 13. Uninstall

| # | Step | Expected |
|---|---|---|
| 13.1 | Tray → Exit, then `~/.local/share/thinkvision-control/uninstall.sh` and answer **y** | The binary, symlink, `.desktop`, icon and autostart entry are removed, and `/etc/udev/rules.d/60-thinkvision-i2c.rules` is gone. `~/.config/ThinkVisionControl` is kept. |
| 13.2 | `ls ~/.local/share/applications \| grep thinkvision; xdg-mime query default x-scheme-handler/thinkvision` | No file. A stale default may remain (harmless; note it). |
| 13.3 | `getfacl /dev/i2c-N` after `sudo udevadm trigger --subsystem-match=i2c-dev` | No user ACL, unless ddcutil's own rule is installed. |
