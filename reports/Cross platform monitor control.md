# Port ThinkVision Control through one Avalonia UI

ThinkVision Control can run on Apple Silicon Macs and on GNOME and KDE Linux from one .NET 10 codebase. The monitor side ports cleanly. The desktop-integration side does not. DDC/CI is the same short MCCS exchange on every OS, and only the transport changes. Windows keeps `dxva2`. Linux writes the packets to `/dev/i2c-N` with five libc calls. macOS sends them through Apple's private `IOAVService` functions, which are undocumented but pass notarization. All three can be called from C#, with no native helper and no GPL code. For the UI, **Avalonia 12.1 is the only framework that covers all three desktops with a tray icon, an MIT licence and a Fluent look**. MAUI has no Linux, Uno has no Wayland and no cross-platform tray, and Avalonia XPF costs €9,500 or more per app. Avalonia is lighter than WPF only under one build configuration. Built NativeAOT with software rendering, a minimal Avalonia tray app idled at **6 MB private** on this laptop, against 12 MB for an equivalent WPF probe. On the default GPU path the same app used **56 MB**, and even the AOT build took **0.95 s** to open its window, against 0.5 s for WPF. That launch gap matters because the app exits when its window closes. Wayland and GNOME, not macOS, decide what has to degrade. Stock GNOME shows no tray icon without the AppIndicator extension. GNOME 46 has no global-shortcut portal. No Wayland compositor lets an ordinary app place a click-through overlay, so the OSD becomes a notification there. Linux users must also grant I²C access, and KDE users may need to stop PowerDevil from competing for the bus. Mac users need a Developer ID-signed, notarized bundle, which requires a $99 a year Apple membership. **The recommended route is staged.** Make Core cross-platform and move OS code behind interfaces now. Ship an Avalonia UI on macOS and Linux first. Replace WPF on Windows only once the Avalonia build matches WPF's launch time, Mica and memory. None of this has run on a Mac or a Linux machine yet, so the first gate is a short test on one Mac, one GNOME machine and one KDE machine.

## The monitor speaks one short protocol, and only the transport changes

The Windows app already isolates the monitor behind a small seam. `ThinkVisionControl.Core` defines `IMonitorLink` (`TryConnect`, `ReadCapabilities`, `Read`, `Write`). `Dxva2MonitorLink` implements it, and `DdcQueue` serialises every call. The only Windows-specific parts are that implementation and the project's `net10.0-windows` target. On Windows, `dxva2.dll` builds and times the DDC/CI packets itself ([Monitor Configuration API](https://learn.microsoft.com/en-us/windows/win32/monitor/using-the-high-level-monitor-configuration-functions)). On Linux and macOS the app builds the bytes itself. A port therefore needs a shared packet codec (framing, checksums, reply validation, capabilities reassembly) and two new transports. The capabilities parser, profile, safety policy and retry logic stay as they are.

The packets are small. A Get VCP request is `51 82 01 <vcp> <chk>`, with the checksum seeded from the display address `0x6E`. After at least 40 ms the host reads an 11-byte reply, `6E 88 02 <rc> <vcp> <type> <maxH> <maxL> <curH> <curL> <chk>`, whose checksum is seeded with `0x50`. A Set is `51 84 03 <vcp> <hi> <lo> <chk>` and gets no reply. These layouts come from a reference C implementation ([janouch ddc-ci.c](https://cgit.janouch.name/desktop-tools/tree/ddc-ci.c)). The spec requires the host to wait "40–200 ms (depending on operation)" between command and response, and **ddcutil spends about 90% of its elapsed time sleeping** ([ddcutil performance options](https://www.ddcutil.com/performance_options/)). Three details come from secondary summaries of the paywalled VESA spec, not from a primary source: the result-code meaning, the capabilities fragment size, and the "null message" busy reply. They should be checked against ddcutil's source before release.

| | Windows (today) | Linux | macOS (Apple Silicon) |
|---|---|---|---|
| Transport | `dxva2.dll` low-level monitor API | `/dev/i2c-N` char device via libc `open`/`ioctl`/`read`/`write` | Private `IOAVServiceWriteI2C`/`ReadI2C` in IOKit |
| Who builds packets | The OS | The app (shared codec) | The app (shared codec) |
| Finding LEN62A2 | Existing EDID match | Read EDID at I²C 0x50 on each bus, match bytes 8–11 | `CGDisplayVendorNumber`/`ModelNumber`, or ioreg `ProductAttributes` |
| Privileges | None | Read/write on `/dev/i2c-*` (udev rule) | None: no root, no SIP change, no entitlement |
| Hotplug | Existing device notifications | Kernel uevents (`drm`, `i2c-dev`), or polling sysfs | `CGDisplayRegisterReconfigurationCallback` + wake |
| Native helper needed | No | No | No (optional m1ddc dylib) |

### Linux: five libc calls, but permissions and quirks are the real work

On Linux the GPU driver exposes each connector's DDC lines as an I²C adapter, and the `i2c-dev` module turns them into `/dev/i2c-N` nodes ([kernel i2c dev-interface](https://docs.kernel.org/i2c/dev-interface.html)). The app opens each node and sets the slave address with the `I2C_SLAVE` ioctl. The kernel docs state that after this, plain `read(2)`/`write(2)` work and "you do not need to pass the address byte". The app reads the EDID at 7-bit address 0x50, checks for the `00 FF FF FF FF FF FF 00` header, and talks MCCS at 0x37 ([janouch ddc-ci.c](https://cgit.janouch.name/desktop-tools/tree/ddc-ci.c)). Lenovo's PnP ID "LEN" packs to `0x30AE` in EDID bytes 8–9, and product 0x62A2 appears little-endian as `A2 62` in bytes 10–11. That arithmetic follows the standard EDID layout and was not checked against a live dump. Everything needed fits in `[LibraryImport]` declarations, which are trim- and AOT-safe, and the research estimates **300–500 lines including retries and capabilities parsing**. The ioctl constants (`I2C_SLAVE = 0x0703`) come from the kernel header and were not fetched in this research. Errors map cleanly. `EACCES` means permissions. `ENXIO`/`EREMOTEIO` mean nothing answered at 0x37, so DDC/CI is off in the monitor menu or the bus is wrong. **Wayland versus X11 is irrelevant to this path**, because `/dev/i2c-N` is a kernel device that is independent of the display server.

The tempting shortcut is libddcutil, which is mature and handles many hardware quirks. It is **GPL-2.0, not LGPL** ([rockowitz/ddcutil](https://github.com/rockowitz/ddcutil)), so linking it from a non-GPL app raises a licensing question this project does not need. Its 1.x→2.x soname change would also force version probing. Running the `ddcutil` CLI as a separate process avoids the linking question and makes a good diagnostics button, but each call takes about a second. The hardware quirks still have to be handled in our own code:

- The NVIDIA proprietary driver fails I²C "on some graphics cards" unless the user sets `options nvidia NVreg_RegistryDwords=RMUseSwI2c=0x01;RMI2cSpeed=100` and reboots ([ddcutil NVIDIA](https://www.ddcutil.com/nvidia/)).
- MST docks expose the same monitor's EDID on two buses, and usually only one of them answers DDC ([drm/amd #1622](https://gitlab.freedesktop.org/drm/amd/-/issues/1622)).
- DisplayLink outputs have no usable DDC on Linux.

ddcutil's defaults show the retry budget a careful client needs: **4 tries for writes, 10 for write-read, 8 for multi-part reads**, with a sleep multiplier tuned per monitor ([ddcutil performance options](https://www.ddcutil.com/performance_options/)).

For hotplug, the cheapest robust approaches are a netlink uevent socket or libudev's monitor, filtered to the `drm` and `i2c-dev` subsystems. Polling `/sys/class/drm/*/status` every few seconds is a fallback. The research found no primary source for the uevent payloads, so that design is marked unverified. Whatever triggers it, the app must re-enumerate buses after hotplug and resume, because bus numbers are not stable across dock reconnects.

### macOS: a private API that every shipping tool uses, and notarization accepts

Apple Silicon has no public DDC API. Every open-source tool calls the same three IOKit exports: `IOAVServiceCreateWithService`, `IOAVServiceWriteI2C` and `IOAVServiceReadI2C`. The research confirmed that the notarized MonitorControl 4.4.0 binary imports these by name, so they are ordinary dynamic symbols that P/Invoke can bind ([MonitorControl Bridging-Header.h](https://github.com/MonitorControl/MonitorControl/blob/main/MonitorControl/Support/Bridging-Header.h), [MonitorControl 4.4.0](https://github.com/MonitorControl/MonitorControl/releases/tag/v4.4.0)). m1ddc finds the right service most precisely:

1. `CoreDisplay_DisplayCreateInfoDictionary(displayID)` gives the `IODisplayLocation`.
2. That location identifies the framebuffer entry.
3. The code walks to the `DCPAVServiceProxy` beneath that framebuffer whose `Location` is `"External"`.
4. The service is wrapped with `IOAVServiceCreateWithService`.

([m1ddc ioregistry.m](https://github.com/waydabber/m1ddc/blob/main/sources/ioregistry.m)). The frame goes to chip 0x37 with data address 0x51. The hardware emits the `0x51` source byte, so the payload starts at `0x84`. Both tools write twice with 10 ms gaps, wait 50 ms, read 11 bytes and retry up to five times ([Arm64DDC.swift](https://github.com/MonitorControl/MonitorControl/blob/main/MonitorControl/Support/Arm64DDC.swift)).

There is one protocol wrinkle. Both tools compute the Get-request checksum without the `0x51` byte that the spec includes, and most monitors evidently accept it. A careful implementation tries the spec form first and falls back to the legacy form. **Neither form has been tested on the P32p-20.** The bindings are about 200 lines of `LibraryImport` against IOKit, CoreFoundation, CoreGraphics and CoreDisplay. The sketch in the notes has not been compiled.

Three facts make this path shippable. First, **it needs no root, no SIP change and no entitlement**. MonitorControl ships Developer ID-signed, hardened-runtime, notarized and *not* sandboxed, with only `allow-jit` in its entitlements ([MonitorControl.entitlements](https://github.com/MonitorControl/MonitorControl/blob/main/MonitorControl/MonitorControl.entitlements)). Second, the App Store route is closed: MonitorControl Lite drops "Hardware DDC" to "conform to the App Store guidelines" ([MonitorControlLite](https://github.com/MonitorControl/MonitorControlLite)). Third, every reference implementation is MIT (m1ddc, MonitorControl, AppleSiliconDDC, Lunar), so porting their logic to C# only needs their copyright notices. The Intel-era `ddcctl` is GPL-3.0 and must not be copied ([kfix/ddcctl](https://github.com/kfix/ddcctl)). Intel Macs use a different IOFramebuffer path, which argues for shipping `osx-arm64` only.

The limits on macOS come from the connection, not the OS. MonitorControl does not support DDC over "the built-in HDMI port of all M1 Macs… and the built-in HDMI port of the entry level M2 Mac mini" ([MonitorControl README](https://github.com/MonitorControl/MonitorControl/blob/main/README.md)). m1ddc added a chip-0xB7 path for those ports in August 2026, but it was tested on a single M1 Mac mini ([m1ddc PR #60](https://github.com/waydabber/m1ddc/pull/60)). DisplayLink never supports DDC on macOS, and many hubs "block DDC requests completely" or forward them to only one monitor ([Lunar FAQ](https://lunar.fyi/faq)). After sleep the DDC endpoint can disappear from the I/O Registry, and Lunar's documented fix is to restart the app. The app should therefore **rebuild the display-to-service map on every reconfiguration callback and on wake** instead of caching handles ([Apple CGDisplayRegisterReconfigurationCallback](https://developer.apple.com/documentation/coregraphics/cgdisplayregisterreconfigurationcallback(_:_:))). For the P32p-20, the reliable Mac connection is USB-C or Thunderbolt straight to the monitor.

## Avalonia is the only toolkit that reaches all three desktops

The candidates fall away quickly:

- **.NET MAUI** supports Android, iOS, Mac Catalyst, Windows and Tizen, and "there is no Linux" ([MAUI supported platforms](https://learn.microsoft.com/en-us/dotnet/maui/supported-platforms)).
- **Uno Platform's** Skia desktop lists only "X11 and Framebuffer" on Linux, not Wayland ([Uno requirements](https://platform.uno/docs/articles/getting-started/requirements.html)). Its only tray package targets Windows hosts ([H.NotifyIcon.Uno](https://www.nuget.org/packages/H.NotifyIcon.Uno/2.5.0-dev.2)).
- **Avalonia XPF** would run the existing WPF code nearly unchanged on Mac and Linux. It costs **€9,500 per application** for the internal tier with 12 months of updates ([Avalonia XPF](https://avaloniaui.net/xpf)), and the P/Invoke tray, flyout and hotkey code would still need per-OS rewrites.
- **Eto.Forms** wraps native toolkits and has a `TrayIndicator` ([Eto TrayIndicator](https://pages.picoe.ca/docs/api/html/T_Eto_Forms_TrayIndicator.htm)). On Windows it renders WPF/WinForms controls without Fluent, so the settings UI would look worse there than it does today.
- **Tauri and Electron** add a web runtime. One developer measured about 80 MB and 120 MB idle respectively on Windows ([levminer](https://www.levminer.com/blog/tauri-vs-electron)).

Avalonia is current, permissively licensed and on the right runtime. Version **12.1.3 shipped on 22 September 2026** ([releases](https://github.com/AvaloniaUI/Avalonia/releases)). Avalonia 12 supports only .NET 8 and later and recommends .NET 10 ([breaking changes](https://docs.avaloniaui.net/docs/avalonia12-breaking-changes)). The core is MIT, and the paid tiers (€299 and €899 per seat per year) add only tooling and premium controls this app does not use ([pricing](https://avaloniaui.net/pricing)). It ships a built-in `TrayIcon` with `NativeMenu`. On Windows, right-click shows the menu and left-click runs a command. On macOS, "clicking the tray icon shows the menu". On Linux it uses the StatusNotifierItem protocol, and "GNOME users may need the AppIndicator extension" ([TrayIcon docs](https://docs.avaloniaui.net/controls/navigation/trayicon)).

Avalonia 12.1 added a native Wayland backend under MIT. It is **experimental, opt-in, and not selected by `UsePlatformDetect()`** ([Avalonia 12.1](https://avaloniaui.net/blog/release-12-1)). In its source, `Position`, `Activate()` and `SetTopmost()` are no-ops, and `Move()` is empty ("surfaces have no screen position") ([WindowImplBase.cs](https://github.com/AvaloniaUI/Avalonia/blob/master/src/Avalonia.Wayland/WindowImplBase.cs)). The OSD and a tray-anchored flyout both need positioning and topmost, so the Linux build should **stay on the default X11 backend, which runs under XWayland on Wayland sessions**, until that changes.

Avalonia can come close to the Windows 11 look, but it re-creates it rather than using the system's own controls. On Windows 11 it supports `TransparencyLevelHint="Mica"`, acrylic, a native title bar that follows the app's theme, per-monitor DPI and automatic light/dark switching ([Avalonia Windows guide](https://docs.avaloniaui.net/docs/platform-specific-guides/windows)). On macOS, `AcrylicBlur` maps to a real `NSVisualEffectView` blur, but the material is fixed to `HUDWindow` ([AutoFitContentView.mm](https://github.com/AvaloniaUI/Avalonia/blob/master/native/Avalonia.Native/src/OSX/AutoFitContentView.mm)). On Linux, KDE blur is only a planned follow-up. **FluentAvalonia 3.1.0** (22 August 2026, MIT, requires Avalonia 12.1 or later, 1.3 M downloads) ports WinUI controls ([NuGet FluentAvaloniaUI](https://www.nuget.org/packages/FluentAvaloniaUI)). It historically includes `NavigationView`, `SettingsExpander`, `InfoBar` and `ContentDialog`, which are exactly the four controls the earlier WPF design research found missing from WPF's built-in Fluent theme. Whether all of them survived 3.0 was not confirmed. The remaining differences are inferences, not measurements: Skia text rendering instead of DirectWrite/ClearType, slightly different control metrics, and different focus visuals. FluentAvalonia also has a single maintainer, publishes releases only on NuGet, and its README is out of date ([amwx/FluentAvalonia](https://github.com/amwx/FluentAvalonia)).

The porting effort is moderate and mostly mechanical. The app has 13 XAML files (1,508 lines), 67 triggers and 30 dependency properties. Avalonia has "no triggers": they become pseudo-class selectors, classes or bindings. `DependencyProperty` becomes `StyledProperty`, and Avalonia 12 requires `x:DataType` because compiled bindings are now the default ([WPF→Avalonia cheat sheet](https://docs.avaloniaui.net/docs/migration/wpf/cheat-sheet)). Most of the triggers live in the 618-line `Controls.xaml`. That file is better deleted than ported, because FluentTheme and FluentAvalonia already provide its looks. `UseWindowsForms` must also go. Most of the work is not in the XAML. It is the per-OS services (tray anchor, OSD placement, hotkeys, device arrival, autostart), and any framework would need those.

## Software rendering decides whether Avalonia is lighter or heavier than WPF

Footprint was measured on this laptop (Ryzen 5 5500U, AMD iGPU plus GTX 1650). The probe was a minimal Avalonia 12.1.3 app and a WPF app with the same shape ([Local measurement](../research_notes/Cross%20platform%20monitor%20control/ui_framework_data.json)). The current Release single-file WPF app, after the switch to plain Win32 message windows (commit `ff12850`), measures **24 MB private at a tray-only login launch and about 77 MB with the window open**.

| Build / mode | Private MB | Working set MB | Threads | Warm launch to window |
|---|---|---|---|---|
| **Current ThinkVision Control (WPF), tray-only login** | **24** | — | — | — |
| **Current ThinkVision Control (WPF), window open** | **~77** | — | — | — |
| WPF probe, never shown | ~12 | ~43 | 12 | — |
| WPF probe, window shown | ~43.5 | ~85–88 | 22 | ~0.52 s |
| Avalonia single-file trimmed, window, GPU | ~91–96 | ~110–119 | 53 | ~2.0 s |
| Avalonia NativeAOT, window, GPU | ~78 | ~77 | 50 | ~0.95 s |
| Avalonia NativeAOT, tray only, GPU | ~56 | ~51.5 | 24 | — |
| **Avalonia NativeAOT, window, software** | **~12.5** | ~36 | 9 | not measured |
| **Avalonia NativeAOT, tray only, software** | **~6** | ~21 | 6 | — |
| Avalonia single-file trimmed, tray only, software | ~20 | ~49 | 9 | — |

On the default GPU path, **Avalonia starts its ANGLE/Direct3D stack even when no window exists**. It reached 56 MB private as a bare tray icon, against 12 MB for never-shown WPF. The lesson from the earlier WPF work, never create a window at login, therefore buys nothing in Avalonia on that path. Software rendering removes that stack. With NativeAOT, the tray-only process drops to about 6 MB private, roughly half the WPF probe. With a window open it is about 12.5 MB against 43.5 MB. The UI is sliders, toggles and text, so CPU rendering costs little, and it also avoids waking the discrete GPU on a hybrid laptop. The probes are minimal apps. The real WPF app sits at roughly twice its probe (24 against 12 MB tray-only). If the Avalonia port carries similar overhead, expect somewhere around **12–18 MB tray-only and 25–45 MB with the window open**. That range is an extrapolation and has to be measured on the real port.

Two conditions apply. First, whether Mica still renders under software rendering is unverified, because the screenshot attempt failed. Second, launch time moves the wrong way. The earlier research showed that under exit-on-close, launch latency is the cost users actually feel. WPF with ReadyToRun opened in 467–491 ms. Avalonia NativeAOT on the GPU path took about 0.95 s warm, and the JIT single-file build about 2 s. Software-rendered startup was not timed. Disk size moves the right way: the AOT build is **34.8 MB in four files**, against 139.5 MB for the untrimmed self-contained WPF build, because WPF cannot be trimmed or compiled AOT. There are no measurements on macOS or Linux, where Metal and OpenGL paths will differ. One third-party claim that Avalonia starts faster than WPF (about 0.5 s against 0.8 s) **is contradicted by these local runs** ([tutorialpedia](https://www.tutorialpedia.org/blog/how-is-avalonia-to-develop-crossplatform-application/)).

## Wayland and GNOME, not macOS, set the floor for integration

macOS turns out to be the easier non-Windows target. **Global hotkeys** use Carbon `RegisterEventHotKey`, which is "sandbox-safe and requires no permissions — unlike event taps" ([KeyboardShortcuts](https://github.com/sindresorhus/KeyboardShortcuts)). Since Sequoia, a hotkey must include at least one modifier other than Shift or Option, or registration fails with -9868 ([Apple forums 763878](https://developer.apple.com/forums/thread/763878?page=2)). Ctrl+Alt+Up passes that rule. **The menu-bar item** is Avalonia's `TrayIcon`. To hide the Dock icon, set `MacOSPlatformOptions.ShowInDock = false`. The sources disagree on Info.plist: one Avalonia discussion says `LSUIElement` alone "doesn't work with Avalonia applications" because Avalonia sets the activation policy at runtime ([discussion #13568](https://github.com/AvaloniaUI/Avalonia/discussions/13568)). Setting both avoids a Dock-icon flash at launch, but that needs testing.

**The OSD** is feasible on macOS with a few Objective-C runtime calls on the native window handle: `setIgnoresMouseEvents:YES`, a status-level window level, a collection behaviour that joins all Spaces and full-screen apps, and `orderFrontRegardless` so it never takes focus. Avalonia itself "currently does not support WPF-like transparent click-through behavior" on any OS ([Avalonia windows how-to](https://docs.avaloniaui.net/docs/how-to/window-how-to)). **Start at login** uses `SMAppService.mainApp.register()`, which lists the app under Login Items and reports `requiresApproval` if the user disables it ([Apple SMAppService](https://developer.apple.com/documentation/servicemanagement/smappservice)). This works only from a signed bundle. **The tray flyout** is harder, because Avalonia's `TrayIcon` exposes no icon rectangle in the docs that were read. The flyout can be placed under the menu bar at top right, or a small Objective-C shim can read the status button's window frame.

On Linux, each feature depends on the session and the desktop:

- **Hotkeys.** On X11, `XGrabKey` works. It must also grab the NumLock and CapsLock variants. On Wayland the only sanctioned route is the xdg-desktop-portal **GlobalShortcuts** interface ([portal docs](https://flatpak.github.io/xdg-desktop-portal/docs/doc-org.freedesktop.portal.GlobalShortcuts.html)).
  - KDE has supported it since Plasma 5.27 ([Phoronix](https://www.phoronix.com/news/KDE-Plasma-5.27-Released)), and GNOME since **GNOME 48** ([GNOME 48 for developers](https://release.gnome.org/48/developers/)). GNOME 46, the version on Ubuntu 24.04 LTS, does not have it. That release mapping was not checked against a primary source in this research.
  - Before calling the portal, a non-Flatpak app must register an app ID that matches its installed `.desktop` file ([Registry docs](https://flatpak.github.io/xdg-desktop-portal/docs/doc-org.freedesktop.host.portal.Registry.html)).
  - A first bind shows a compositor dialog, and the user or the compositor chooses the final trigger.
  - SharpHook is the wrong tool. It is a keyboard hook that needs Accessibility on macOS and "elevated privileges" on Wayland ([SharpHook](https://github.com/TolikPylypchuk/SharpHook)).
  - The universal fallback is a CLI verb (`--step brightness=+5`) that users bind as a custom shortcut in GNOME or KDE settings.
- **The tray.** KDE hosts StatusNotifierItem natively. Stock GNOME does not. Its own "Status Icons" extension supports only XEmbed, and its author says "AppIndicators are not it" ([OMG! Ubuntu](https://www.omgubuntu.co.uk/2024/08/gnome-official-status-icons-extension)). Ubuntu preinstalls Canonical's AppIndicator extension, and Fedora packages it as `gnome-shell-extension-appindicator` but does not install it by default. If `org.kde.StatusNotifierWatcher` is missing from the session bus, the app should open its window rather than start invisibly.
- **The flyout.** Under the GNOME extension, a left click usually opens the menu. On Wayland a window cannot position itself anyway. On GNOME, quick controls should therefore live in the tray menu as presets and input choices.
- **The OSD.** On X11 it works, but click-through needs an empty XShape input region ([discussion #11911](https://github.com/AvaloniaUI/Avalonia/discussions/11911)). On Wayland a real overlay needs `wlr-layer-shell`. KDE supports it through layer-shell-qt, but Avalonia does not expose it, and GNOME's Mutter has only an open issue for it ([Mutter #973](https://gitlab.gnome.org/GNOME/mutter/-/issues/973)). The Wayland fallback is a `org.freedesktop.Notifications` notification updated in place with `replaces_id`. Whether XWayland windows keep topmost and position on GNOME and KDE is the biggest open question in the whole port.

Several features port cleanly:

- **Single instance.** .NET named pipes on Unix are Unix domain sockets. `CurrentUserOnly` sets mode 0600 and checks the peer's UID. Message mode throws, and the instance limit is not enforced across processes ([NamedPipeServerStream.Unix.cs](https://source.dot.net/System.IO.Pipes/System/IO/Pipes/NamedPipeServerStream.Unix.cs.html)). The design becomes a lock file plus a socket with length-prefixed messages. The socket path must stay under about 103 characters on macOS.
- **Quick actions.** Linux `.desktop` `Actions=` entries are the closest match to the Windows jump list. Launchers show them "as a submenu", and they run `Exec` commands with the app closed ([Desktop Entry spec](https://specifications.freedesktop.org/desktop-entry/latest/extra-actions.html)). macOS's Dock menu needs a Dock icon ([Avalonia macOS guide](https://docs.avaloniaui.net/docs/platform-specific-guides/macos)), so for a menu-bar-only app the status-item menu is the only quick-action surface. The research found no Mac surface that works while the app is closed.
- **Autostart on Linux.** Write `~/.config/autostart/<app-id>.desktop` per the XDG spec ([XDG Autostart](https://specifications.freedesktop.org/autostart/latest/)).
- **Theme.** Avalonia follows the system light/dark preference by default ([theme variants](https://docs.avaloniaui.net/docs/styling/theme-variants)). The Linux portal's `color-scheme` key is the fallback source ([portal Settings](https://flatpak.github.io/xdg-desktop-portal/docs/doc-org.freedesktop.portal.Settings.html)).
- **Efficiency mode.** EcoQoS is Windows-only. The research did not cover a macOS or Linux equivalent, so that service is a no-op elsewhere.

## Linux users must grant I²C access, and Mac users must trust a notarized bundle

On Linux, `/dev/i2c-*` is root-only by default. The cleanest fix is the udev rule that the ddcutil package installs, `60-ddcutil-i2c.rules`. It "uses tag uaccess to grant read/write permissions to /dev/i2c devices associated with a video controller" ([ddcutil I2C permissions](https://www.ddcutil.com/i2c_permissions/)). The person at the active local session gets access without a group change and without logging out. The fallback is an `i2c` group plus a custom rule, which needs a re-login ([ddcutil group i2c](https://www.ddcutil.com/i2c_permissions_using_group_i2c/)). The module itself may need a `modules-load.d` entry ([ddcutil kernel module](https://www.ddcutil.com/kernel_module/)).

On a first run the app should check, in order, that `/dev/i2c-*` exists, that it is readable and writable, and that the module is loaded. Then it should print the exact commands to fix whatever is missing. NVIDIA-proprietary users may additionally need the modprobe option from the previous section.

**KDE adds a conflict.** Plasma 6's PowerDevil links libddcutil and controls external-monitor brightness by default ([Arch FS#78536](https://bugs.archlinux.org/task/78536)). DDC/CI holds state between a write and its read, and the kernel does not serialise that pair. Two clients on one bus therefore corrupt each other's replies, and PowerDevil will also overwrite brightness on idle and resume. The documented opt-out is `Environment=POWERDEVIL_NO_DDCUTIL=1` in `systemctl --user edit plasma-powerdevil.service`, followed by a re-login ([KDE Discuss](https://discuss.kde.org/t/disable-kde-monitor-brightness-control/25134)). The app should detect PowerDevil and offer that step. **GNOME's user step** is the AppIndicator extension, unless the distribution already ships it.

**Packaging follows from the I²C requirement.** Snap's `i2c` interface is not auto-connected, names one fixed device path, and is "intended to be used with Ubuntu Core" ([Snapcraft i2c](https://snapcraft.io/docs/reference/interfaces/i2c-interface/)). Flatpak has no I²C permission short of `--device=all` ([Flatpak permissions](https://docs.flatpak.org/en/latest/sandbox-permissions.html)), and host udev rules still apply inside it. That leaves **.deb/.rpm**, which can install the udev rule, a `modules-load.d` entry and a `.desktop` file with actions, plus an **AppImage** that comes with a one-time setup command. PupNet Deploy builds AppImage, deb, rpm and Flatpak from one config, but not macOS. Its Flatpak output "is unsuitable for use with Flathub" ([PupNet Deploy](https://github.com/kuiperzone/PupNet-Deploy)).

**On macOS, DDC and hotkeys need no permission prompt.** The friction is trust. Software distributed with Developer ID "must be notarized", including .NET apps ([Microsoft Learn: macOS notarization](https://learn.microsoft.com/dotnet/core/install/macos-notarization-issues)). Notarization requires the **$99-a-year Apple Developer Program** ([Apple Developer Program](https://developer.apple.com/programs/)). Avalonia documents the full pipeline ([Avalonia: Deploying on macOS](https://docs.avaloniaui.net/docs/deployment/macos)):

1. Publish for `osx-arm64`.
2. Build the `.app` and write `Info.plist` by hand.
3. Sign with `codesign --timestamp --options=runtime` and an entitlements file, signing nested binaries first and avoiding `--deep`.
4. Submit with `xcrun notarytool submit … --wait`.
5. Staple the ticket to the app.
6. Notarize and staple the DMG separately.

Avalonia says `allow-jit` "is required to run Avalonia apps under hardened runtime". A NativeAOT build has no JIT, so it should not need that entitlement, but this is an inference to test. Without the membership, users must approve an unsigned app under System Settings → Privacy & Security → "Open Anyway" ([Apple Support 102445](https://support.apple.com/en-us/102445)). Sources disagree on how much harder Sequoia made this.

## Conclusion

The port has an unusual difficulty profile. The parts that sound hardest, raw I²C on Linux and a private Apple API on macOS, are small, well charted and fully covered by MIT reference code. The parts that sound routine, a tray icon, a hotkey and a popup, are where the platforms diverge, and on GNOME Wayland several of them do not exist for ordinary apps. As a result, the app on that desktop is a different product: a window, a menu, a CLI and launcher actions, not a resident tray utility with an overlay. The exit-on-close default and the jump-list-first design already chosen on Windows suit that platform well. The quick-access surface that needs no running process (jump list, `.desktop` actions, CLI verbs bound to desktop shortcuts) is also the one that works everywhere.

The framework decision is less about looks than about the build. Avalonia beats today's WPF memory only with NativeAOT and software rendering, it loses on launch time on the configuration measured so far, and FluentAvalonia's AOT-safety is unproven. That is why the safe order is to build the cross-platform head for the platforms that have nothing today. WPF should be retired on Windows only once measurements show the Avalonia build costs nothing there.

## Recommendations for ThinkVision Control

### 1. UI strategy: three options, with a staged path recommended

| Criterion | A. Avalonia everywhere, replace WPF now | B. Keep WPF on Windows + Avalonia for macOS/Linux, permanently | C. Staged: Avalonia for macOS/Linux first, then replace WPF on Windows behind gates |
|---|---|---|---|
| Windows look | Fluent re-creation: Mica, acrylic, themed title bar, FluentAvalonia controls; Skia text instead of ClearType | Unchanged (WPF Fluent theme + hand-built controls) | Unchanged until the gate, then as A |
| Windows regression risk | Highest: every Windows user moves at once | None | Low: WPF stays the default until the Avalonia build passes side-by-side review |
| UI maintenance | One UI | **Every page, card, dialog and quick control built twice**, in two XAML dialects | Two UIs for one or two releases, then one |
| Windows footprint | Lower than today **only** with NativeAOT + software rendering (probe: 6 vs 12 MB tray-only); higher on GPU path (56 MB) | Today's 24 MB tray-only / ~77 MB open | As B, then as A once measured |
| Windows launch time (matters under exit-on-close) | Slower on measured config (~0.95 s AOT GPU vs ~0.5 s WPF); software-path launch unmeasured | Today's | Gate on it |
| Time to macOS/Linux | Slowest: Windows parity work comes first | Fastest | Fast: Windows parity is deferred |
| Dependency risk | FluentAvalonia (one maintainer) and AOT compatibility hit all users | Hits only Mac/Linux users | Hits Mac/Linux first, then all users after evidence |
| Accessibility | UIA on Windows; AT-SPI2 on Linux (Avalonia 12) | WPF UIA work (commit `de01ed9`) preserved | Preserved until switch |

**Recommendation: C.** Ship the Avalonia UI on macOS and Linux first. Keep WPF as the Windows release while the same Avalonia build runs on Windows as a preview. Delete WPF only when all four gates pass:

1. Mica still renders under software rendering, or the GPU-path memory is acceptable.
2. Warm launch to first window is within about 100 ms of the WPF ReadyToRun build.
3. Tray-only private memory is at or below 24 MB.
4. A side-by-side visual review of every page passes.

Choose **B** if Windows fidelity is non-negotiable and you accept double UI maintenance. Choose **A** only if one UI matters more than any Windows regression. Do not pick MAUI (no Linux), Uno (no Wayland, no tray outside Windows), XPF (€9,500+ per app), Eto (no Fluent on Windows) or a web shell (memory).

Pin **Avalonia 12.1.x and FluentAvalonia 3.1.x**, use only the MIT packages, run the X11 backend on Linux (XWayland on Wayland sessions), and set `ShowInDock=false` on macOS.

### 2. Architecture split: shared Core plus per-OS platform services

| Project | Target | Contents |
|---|---|---|
| `ThinkVisionControl.Core` | `net10.0` (drop `-windows`) | Profile, capabilities parser, `ControlService`, `DdcQueue`, `SafetyPolicy`, `RetryBurst`, settings model, command-line verbs, **new MCCS packet codec** (frame, checksum incl. legacy variant, reply validation of opcode, result code and echoed VCP, capabilities reassembly), platform interfaces below, an `IAppPaths` for settings/log/lock locations |
| `ThinkVisionControl.ViewModels` | `net10.0` | Framework-neutral view models (no `System.Windows`, no Avalonia types), so WPF and Avalonia can both bind to them during the staged period |
| `ThinkVisionControl.Platform.Windows` | `net10.0-windows` | Existing `Dxva2MonitorLink`, `TrayAnchor`, `HotkeyManager`, `MonitorArrivalWatcher`, `DisplayEvents`, `EfficiencyMode`, `StartupRegistration`, jump list, moved behind the interfaces |
| `ThinkVisionControl.Platform.MacOS` | `net10.0` (osx-arm64) | IOAVService link, CG reconfiguration watcher, Carbon hotkeys, SMAppService, NSWindow OSD shim, status-item frame shim |
| `ThinkVisionControl.Platform.Linux` | `net10.0` (linux-x64/arm64) | i2c-dev link, uevent/udev or polling watcher, XGrabKey + GlobalShortcuts portal (Tmds.DBus), XDG autostart, XShape OSD, notification OSD, SNI-watcher probe, permission and PowerDevil diagnostics |
| `ThinkVisionControl` (WPF) | `net10.0-windows` | Current UI, kept during the staged period |
| `ThinkVisionControl.Avalonia` | `net10.0` | New UI, NativeAOT, software rendering, compiled bindings |

| Interface | Responsibility | Windows | macOS | Linux |
|---|---|---|---|---|
| `IMonitorLink` (exists) | Connect, read caps, Get/Set VCP | dxva2 | IOAVService (chip 0x37, data 0x51) | `/dev/i2c-N` @ 0x37, EDID @ 0x50 |
| `IMonitorWatcher` | Arrival, removal, resume → re-enumerate | Device notifications + display/resume events | `CGDisplayRegisterReconfigurationCallback` + wake | netlink uevents or libudev; sysfs polling fallback; logind resume |
| `IHotkeyService` | Register gestures, report failures and final trigger text | `RegisterHotKey` | Carbon `RegisterEventHotKey` (reject Shift/Option-only) | X11 `XGrabKey`; Wayland GlobalShortcuts portal; else `Unavailable` |
| `ITrayPresenter` | Icon, menu, click → flyout or menu | Avalonia `TrayIcon` (or current Shell_NotifyIcon) | `TrayIcon` + template icon | `TrayIcon` (SNI); reports `NoHost` when no watcher |
| `IFlyoutPlacement` | Where quick controls appear | `Shell_NotifyIconGetRect` | Status-item frame shim, or top-right under the menu bar | X11 near click; Wayland/GNOME → `MenuOnly` |
| `IOsdPresenter` | Value feedback | Click-through topmost window | NSWindow with ignoresMouseEvents, all Spaces | X11 XShape window; Wayland → notification with `replaces_id` |
| `IAutostart` | Start at login, status, open settings | Run key | `SMAppService.mainApp` (+ `requiresApproval` UI) | `~/.config/autostart/<app-id>.desktop` |
| `ICommandChannel` | Single instance + argument forwarding | Named pipe (existing) | Lock file + Unix socket (≤103-char path) + macOS reopen | Lock file in `$XDG_RUNTIME_DIR` + Unix socket |
| `IQuickActions` | Off-process quick actions | Jump list | None while closed; status-item menu while running | `.desktop` `Actions=` |
| `IPowerPolicy` | Background efficiency | EcoQoS + low memory priority | No-op (not researched) | No-op (not researched) |
| `IDdcDiagnostics` | Explain why DDC is unavailable | DDC/CI off hint | "Not available on this connection" (HDMI/dock/DisplayLink) | Module, permissions, NVIDIA, PowerDevil, optional `ddcutil environment` |
| `IPlatformCapabilities` | Tells the UI which features exist, so it hides or relabels them | All true | Per row in §3 | Per session type and desktop |

### 3. Per-OS feature matrix

Legend: **Full** = same as Windows. **Degraded** = works with a substitute or caveat. **Unavailable** = hidden, with the fallback named.

| Feature | Windows 11 | macOS (Apple Silicon) | Linux X11 (GNOME/KDE) | KDE Plasma 6 Wayland | GNOME Wayland |
|---|---|---|---|---|---|
| DDC/CI brightness, contrast, input | Full | **Full over USB-C/TB/DP**; Unavailable on M1 / entry-M2 built-in HDMI, DisplayLink, many docks | Full after I²C setup; Degraded on NVIDIA proprietary (modprobe) and MST (pick the answering bus); Unavailable on DisplayLink | Same as X11, **plus PowerDevil contention** until disabled | Same as X11 |
| Monitor hotplug / resume | Full | Full (reconfig callback + wake re-scan) | Full (uevents; polling fallback) | Full | Full |
| Global hotkeys | Full | Full (no permission; ≥1 modifier other than Shift/Option) | Full (`XGrabKey`) | Degraded: portal, user confirms trigger in a dialog | GNOME 48+: Degraded as KDE; **GNOME 46 (Ubuntu 24.04): Unavailable → CLI verb as custom shortcut** |
| Tray / menu-bar icon | Full | Full (menu bar, no Dock icon) | KDE Full; GNOME needs AppIndicator extension (Ubuntu yes, Fedora no) | Full | Needs AppIndicator extension; **without it Unavailable → window opens on launch** |
| Tray-anchored flyout | Full | Degraded (approximate placement or ObjC shim; click shows menu by default) | KDE Degraded (place near click); GNOME menu-only | Degraded: centred window or menu-only | Unavailable → quick controls as menu items |
| Click-through OSD | Full | Full (via ObjC calls) | Full (XShape) | Degraded → notification (layer-shell not exposed by Avalonia); XWayland window behaviour untested | Unavailable → notification |
| Start at login | Full | Full in signed bundle (`SMAppService`); unsigned → LaunchAgent fallback | Full (XDG autostart) | Full | Full |
| Single instance + CLI verbs | Full | Full | Full | Full | Full |
| Quick actions with app closed | Full (jump list) | **Unavailable** → status-item menu while running | Full (`.desktop` actions) | Full | Full |
| Light/dark + accent | Full | Full (accent unverified) | Full via portal | Full | Full |
| Window material | Mica | Degraded (fixed HUD blur) | None | None (KDE blur planned upstream) | None |
| Background efficiency | EcoQoS | No-op | No-op | No-op | No-op |
| Exit-on-close default, background optional | Full | Full | Full | Full | Full (the natural mode here) |

### 4. Packaging

| OS | Format | Contents and steps | User-visible setup |
|---|---|---|---|
| Windows | As today (single-file, no compression), plus NativeAOT once on Avalonia | Avalonia AOT probe: 34.8 MB in 4 files vs 139.5 MB WPF self-contained | None |
| macOS | Notarized `.app` in a notarized, stapled DMG; optional Homebrew cask (as MonitorControl) | `dotnet publish -r osx-arm64` (AOT) → bundle + `Info.plist` (`CFBundleIdentifier`, `LSUIElement`, `NSHighResolutionCapable`) → sign nested binaries then app with Developer ID, hardened runtime, timestamp, minimal entitlements (test whether AOT drops `allow-jit`) → `notarytool submit --wait` → `stapler staple` → DMG notarize + staple. Automate on a GitHub macOS runner. Ship arm64 only | Approve Login Item if prompted. Unsigned builds: "Open Anyway" in Privacy & Security |
| Linux (primary) | `.deb` and `.rpm` (PupNet or hand-written) | Install binary, `.desktop` with `Actions=` (file name = reverse-DNS app ID, reused for portal Registry and autostart), icons, a uaccess udev rule (copied from ddcutil's `60-ddcutil-i2c.rules` after reading its exact content) or `Recommends: ddcutil`, and `modules-load.d/i2c-dev.conf` | Replug or reboot once; GNOME: AppIndicator extension; KDE: optional PowerDevil opt-out; NVIDIA: optional modprobe line |
| Linux (secondary) | AppImage | Same binary; first-run "integrate" step writes `.desktop` and offers a `pkexec` helper for the udev rule | Run the setup step once |
| Linux (later) | Flatpak | Needs `--device=all`; autostart via Background portal; Flathub acceptance unknown | As AppImage, plus host udev rule |
| Linux | Snap | **Do not ship** (i2c interface is Ubuntu Core-only) | — |

### 5. Verification needed on real hardware

The developer has only the Windows laptop. Everything below the first row is unverified until it runs on the target machine. One cheap step is to boot a live USB of Ubuntu and Fedora on the same laptop with the P32p-20 attached. That exercises the Linux I²C path, the hybrid AMD/NVIDIA case and GNOME, though which GPU drives the external port decides which driver's I²C bus answers. Virtual machines will not stand in for DDC hardware (an inference: VM display adapters do not pass monitor I²C through).

| # | Where | What to verify | Pass criterion |
|---|---|---|---|
| 1 | Windows laptop (now) | Avalonia AOT + software rendering: Mica renders; real-app private memory tray-only and open; warm launch; FluentAvalonia controls under AOT/trim; UIA names | Meets the four gates in §1 |
| 2 | Linux live USB on the laptop | i2c-dev present; EDID scan finds LEN62A2; Get/Set 0x10/0x12/0x60 and capabilities via managed code; errno paths; NVIDIA vs AMD bus | Values round-trip; capabilities string matches the Windows probe |
| 3 | GNOME 46 (Ubuntu 24.04) and GNOME 48+ (Fedora), Wayland and X11 | Tray with and without AppIndicator; left-click behaviour; SNI-watcher detection; GlobalShortcuts portal + Registry flow; XWayland OSD topmost/position; notification fallback | Every row of §3 behaves as labelled |
| 4 | KDE Plasma 6, Wayland and X11 | PowerDevil contention (checksum failures, overwritten brightness); `POWERDEVIL_NO_DDCUTIL=1` fix; portal hotkeys; SNI `Activate` coordinates; XWayland OSD | Detection and guidance fire; no corrupted replies after the opt-out |
| 5 | Apple Silicon Mac over USB-C/TB | IOAVService discovery; **spec vs legacy Get checksum on the P32p-20**; Set read-back; sleep/wake and replug re-enumeration; `CGDisplayVendorNumber == 0x30AE`, `ModelNumber == 0x62A2` | Stable control across 10 sleep/wake cycles |
| 6 | Same Mac | Signed, notarized AOT bundle launches from a quarantined download; entitlements needed; `SMAppService`; Carbon hotkey; OSD over full-screen apps and across Spaces; template tray icon; `ShowInDock`/`LSUIElement` | No Gatekeeper block; no permission prompts |
| 7 | Mac via HDMI and a dock (if available) | The "not available on this connection" state | Clear message, no hang |
| 8 | All three | Idle footprint tray-only and window-open on each OS | Recorded next to the Windows numbers |

### 6. Risks, ranked

| # | Risk | Severity | Mitigation |
|---|---|---|---|
| 1 | **Linux Wayland integration**: no positioning or topmost on native Wayland; XWayland behaviour on GNOME/KDE untested; no layer-shell via Avalonia | High | X11 backend by default; notification OSD; menu-only quick controls; capability flags hide what is missing |
| 2 | **GNOME tray absent** without the AppIndicator extension (Fedora, vanilla GNOME) | High (product) | Detect `StatusNotifierWatcher`; open the window instead; document the extension; `.desktop` actions + CLI as the primary quick path |
| 3 | **No real-hardware testing yet** on macOS or Linux | High (process) | §5 before committing to the port; live USB now; borrow or buy a Mac before the macOS release |
| 4 | **Linux onboarding and support load**: i2c permissions, NVIDIA modprobe, MST phantom buses, PowerDevil contention | High | Packaged udev rule; first-run diagnostics with exact commands; per-bus serialisation and checksum retry; PowerDevil detection |
| 5 | **macOS private API** (`IOAVService`) undocumented; could change in a macOS release | Medium | Isolate in one class; track MonitorControl/m1ddc releases; re-enumerate on wake; graceful "unavailable" state |
| 6 | **Windows launch and footprint regression** if Avalonia replaces WPF (GPU path 56 MB tray-only; ~0.95 s launch; Mica under software unverified) | Medium | Staged option C with measured gates; software rendering + NativeAOT |
| 7 | **FluentAvalonia bus factor and AOT safety** | Medium | Use few controls; pin versions; small enough to vendor; AOT publish in CI on all RIDs from day one |
| 8 | **macOS connection limits** (M1/entry-M2 HDMI, DisplayLink, docks) | Medium | Recommend direct USB-C/TB; optional MCDP29xx 0xB7 path later; clear UI state |
| 9 | **Notarization overhead**: $99/yr, certificate custody, CI secrets | Medium | Automate in CI; publish unsigned builds with "Open Anyway" instructions only as a stop-gap |
| 10 | **Double UI maintenance** if staging stalls into option B | Medium | Framework-neutral view models; time-box the parallel period to one or two releases |
| 11 | **Windows visual differences** (Skia text, metrics, focus visuals) | Low–medium | Side-by-side review gate |
| 12 | **GPL entanglement** via libddcutil or ddcctl | Low (if avoided) | Own managed transports; ddcutil CLI only as an out-of-process diagnostic; MIT notices for m1ddc/MonitorControl-derived code |
