# Lightweight UI stack: web findings

Researched 2026-10-27/28. This covers the facts local probes can't give: macOS sizes, Wayland/tray/hotkey behaviour, licences and maintenance status.

- Every claim carries a URL. Anything without one is marked **[unsourced]**.
- The research was split across several sub-agents.
- For GitHub, crates.io and NuGet, some numbers were read through the official APIs (`gh api`, crates.io API, NuGet flat container); the URLs given are the canonical pages for that data.
- Network access was flaky at times. Each section's "Unverified" list says what could not be confirmed.

Targets: **< 10 MB shipped**, **< 2 s cold start**, on Windows 11 x64, macOS Apple Silicon, and Linux (X11 + Wayland, GNOME + KDE).

---

## Cross-cutting conclusions (read this first)

1. **Some constraints apply to every toolkit.** No toolkit choice fixes these, because they come from the platforms:
   - **Flyout and overlay on GNOME Wayland:** a client cannot position a window or keep it above others. Mutter has no wlr-layer-shell; KDE, Sway, Hyprland and COSMIC do. See §9.
   - **Flyout anchoring on Linux:** StatusNotifierItem never gives the icon's screen rect, so no Linux toolkit can anchor a flyout to the tray icon. See §1 and §9.
   - **Wayland global hotkeys:** these need the GlobalShortcuts portal. Since xdg-desktop-portal 1.21.0 the portal requires a valid app ID, which for a non-Flatpak app means calling `Register` and shipping a `.desktop` file. See §9.
   - **Tray on stock GNOME:** needs the AppIndicator extension. Ubuntu ships it; Fedora Workstation does not. See §9.
2. **Rust plus Slint looks size-feasible on all three OSes.**
   - A Slint maintainer reports an empty app at about 3.5 MB on Windows.
   - A third-party Slint tray app is 9.0 MB on macOS.
   - Duja, a Rust + Slint DDC tray app with this feature set, ships a 7.6 MB Windows zip, a 9.9 MB Linux tar.gz and a 16.3 MB *universal* macOS DMG. An arm64-only build is likely under 10 MB [unverified].
   - Slint 1.17+ has a built-in tray, but its click event carries no position.
   - `global-hotkey` is X11-only. The repo's own portal code stays.
3. **Qt Widgets with dynamic linking misses 10 MB uncompressed**: about 26.4 MB on Windows and about 20.8 MB on macOS arm64. It fits only as a compressed download (about 10.8 MB and 8.4 MB zipped) or with an unmeasured static Qt Lite build.
4. **Avalonia cannot reach 10 MB.** `libSkiaSharp` alone is 10.6–11.1 MB on Windows and Linux. No non-Skia backend ships cross-platform.
5. **Truly native cross-platform frameworks:**
   - wxWidgets 3.3.4, used from C++ or from Rust via **wxDragon**, has native controls, an AppIndicator tray on Wayland, and is 3.8–7.2 MB on macOS.
   - Lazarus/LCL is the other credible native option.
6. **Cold-start data is almost absent for every stack.** The only figures are from 2023, on Linux: iced about 230 ms and egui about 280 ms to final render. Measure locally.

---

## 1. Rust stack viability

**Takeaway:** The size goal looks reachable: Slint is about 3.5 MB on Windows, and egui with glow about 4.3 MB. The gaps:
- `global-hotkey` 0.9.0 is still X11-only, so the repo's portal code stays.
- winit cannot keep a window on top on Wayland.
- `ddc-hi` hasn't had a release since 2021.
- `ddc-macos` does support Apple Silicon.

Slint 1.17+ has a built-in tray. Under Slint's GPLv3 option, the combined binary would ship under GPLv3, which the "or later" clause allows.

### `tray-icon` (tauri-apps)
- **Version:** 0.26.1 (2026-10-07). About 10 releases in 2026, so actively maintained. https://crates.io/api/v1/crates/tray-icon
- **Platforms:** Windows, macOS, and Linux/BSD. https://github.com/tauri-apps/tray-icon
- **Default Linux backend (`libappindicator`):**
  - Needs GTK3 plus libappindicator3 or libayatana-appindicator3.
  - A GTK event loop must run on the thread.
  - Source: https://github.com/tauri-apps/tray-icon
- **New GTK-free `ksni` backend:** added in 0.25.0 via PR #201 (merged 2026-09-08).
  - Pure StatusNotifierItem over D-Bus.
  - Runs its own worker thread.
  - Wins if both backends are enabled.
  - Since 0.26.0 it no longer fails when no `StatusNotifierWatcher` exists yet.
  - Sources: https://raw.githubusercontent.com/tauri-apps/tray-icon/dev/CHANGELOG.md · https://github.com/tauri-apps/tray-icon/pull/201
- **macOS:** the tray must be created on the main thread with the event loop running. https://github.com/tauri-apps/tray-icon
- **Flyout anchoring:** `TrayIconEvent::Click` carries `position` and `rect`.
  - Linux AppIndicator emits no events at all.
  - Linux KSNI emits only left and middle clicks, with an empty `rect`.
  - So anchoring works only on Windows and macOS.
  - Source: https://docs.rs/tray-icon/latest/tray_icon/enum.TrayIconEvent.html
- **Windows fixes in 2026:** survives Explorer restarts (0.24.1), stable registration via `with_guid` (0.25.0), and the menu no longer opens behind the taskbar (0.23.1). https://raw.githubusercontent.com/tauri-apps/tray-icon/dev/CHANGELOG.md

### `global-hotkey` (tauri-apps)
- **Version:** 0.9.0 (2026-10-16). Earlier: 0.8.0 (2026-05-01), 0.7.0 (2025-05-07). https://crates.io/api/v1/crates/global-hotkey
- **The README says "Linux (X11 Only)".** The Linux dependencies are only `x11rb` and `xkeysym`; there is no portal or D-Bus code. https://github.com/tauri-apps/global-hotkey · https://raw.githubusercontent.com/tauri-apps/global-hotkey/dev/Cargo.toml
- **Wayland tracking:** issue #28 has been open since 2022-03-01. https://github.com/tauri-apps/global-hotkey/issues/28
- **PR #162 "Wayland support":**
  - Uses the XDG GlobalShortcuts portal through a separate `wl_register_all()` API.
  - Opened 2025-09-14 and **still unmerged** as of 2026-10-10.
  - Testers report it working on KDE 6.4/6.5, GNOME 48/49 and Fedora 43. GNOME 48 needs 48.8 for rebinding.
  - Source: https://github.com/tauri-apps/global-hotkey/pull/162
- **PR #172 "Unified Wayland support":**
  - Uses `ashpd` and falls back to X11.
  - Opened 2026-03-27 and still open.
  - Gaps: no backend for COSMIC, wlroots or Sway, and left/right modifiers can't be told apart.
  - Source: https://github.com/tauri-apps/global-hotkey/pull/172
- **Threading:** on Windows, a win32 message loop must run on the creating thread. On macOS, the main thread with the event loop. https://github.com/tauri-apps/global-hotkey
- **Tauri plugin bugs:** a "doesn't work in wayland" report (plugins-workspace #3267) and a Windows deadlock (#3590). https://github.com/tauri-apps/plugins-workspace/issues/3590

### `ksni`
- **Version:** 0.3.7 (2026-10-20). A pure-Rust KDE/freedesktop StatusNotifierItem implementation.
- Both `tray-icon` (its `ksni` feature) and Slint's tray use it. https://crates.io/api/v1/crates/ksni

### Slint
- **Version:** 1.18.1 (2026-09-21). Earlier: 1.17.0 (2026-06-24), 1.16.0 (2026-04-16). https://crates.io/api/v1/crates/slint
- **Licence:** `GPL-3.0-only OR LicenseRef-Slint-Royalty-free-2.0 OR LicenseRef-Slint-Software-3.0`. https://github.com/slint-ui/slint/blob/master/LICENSE.md
  - **Royalty-free 2.0:** free for desktop apps, including proprietary ones. You must show the `AboutSlint` widget or put a Slint badge on the download page.
  - **GPL option:** GPLv3 *only*.
  - **Commercial:** also available.
- **Slint's FAQ on the GPL option:** your own code may stay under any GPL-compatible licence, but "the work as a whole must be licensed under the GPL". https://github.com/slint-ui/slint/blob/master/FAQ.md
- **What that means for a GPL-2.0-or-later app:** the source files keep GPL-2.0-or-later, and the distributed binary is effectively GPLv3, which the "or later" clause permits. https://www.gnu.org/licenses/gpl-faq.html#AllCompatibility (gnu.org could not be re-fetched in this session; secondary confirmation: https://lists.scummvm.org/pipermail/scummvm-devel/2016-January/011610.html)
  - Alternative: use the Royalty-free licence with `AboutSlint` shown.
- **Renderers and features:**
  - Default features: `renderer-femtovg`, `renderer-software`, `accessibility`, `system-tray`.
  - Optional renderers: `renderer-skia`, `renderer-femtovg-wgpu`, and an experimental `renderer-vello`.
  - `backend-winit-x11` and `backend-winit-wayland` exist to slim the build.
  - Source: https://docs.slint.dev/latest/docs/rust/slint/docs/cargo_features/
- **Sizes:**
  - **Maintainer, 2025-09-29:** an empty Windows release build is about **3.5 MB**, and about **2.8 MB** with the software renderer and accessibility off. A small real app is estimated at 3–4 MB with LTO.
  - A user's Windows app was 6.5 MB.
  - **Skia** pushed that same user's build to about **23 MB**, which the maintainer called expected.
  - Source: https://github.com/slint-ui/slint/discussions/9570
- **Renderer comparison, Windows 10 (2023, older Slint):** https://github.com/slint-ui/slint/discussions/3376

  | Renderer | Size | RAM |
  |---|---|---|
  | femtovg | 3.45 MB | 51 MB |
  | winit-software | 3.87 MB | 14 MB |
  | skia-opengl | 19.5 MB | 52.6 MB |
  | skia-d3d | 20.3 MB | 17.5 MB |

- **A third-party Slint tray app** reports a **9.0 MB macOS binary** (40 MB footprint) and 13.6 MB on Linux. In its Linux tests, femtovg with accessibility was 17.5 MB and software without accessibility 12.9 MB. The software renderer mostly saves RAM (20 vs 92 MB RSS), not disk. https://github.com/pohunekd/pohunek-companion/issues/8
- Turning off skia-safe's pdf/jpeg/icu features gives no size win. https://github.com/slint-ui/slint/issues/13011
- **Accessibility:** AccessKit has been in Slint since 1.1 (issue #32, PR #2865).
  - Slint uses `accesskit 0.25` and `accesskit_winit 0.34`.
  - AccessKit covers Windows UIA, macOS NSAccessibility and Unix AT-SPI "at rough feature parity", but not every element and property.
  - Sources: https://github.com/slint-ui/slint/issues/32 · https://raw.githubusercontent.com/slint-ui/slint/master/internal/backends/winit/Cargo.toml · https://github.com/AccessKit/accesskit
- **Tray: built in since 1.17** as the `SystemTrayIcon` element. Request #6053 closed 2026-05-06; implementation PRs #11234, #11481, #11505 and #11575. https://slint.dev/blog/slint-1.17-released
  - Backends: Linux SNI via ksni (XEmbed unsupported; GNOME needs the extension), macOS `NSStatusItem`, Windows `Shell_NotifyIcon`.
  - **Limitations:**
    - Its only callback is `clicked()`, with no position or rect.
    - On macOS, `clicked` doesn't fire when a menu is attached.
    - Menu shortcuts are ignored.
    - Source: https://docs.slint.dev/latest/docs/slint/reference/window/systemtrayicon/
  - Open bugs: #14419, checkmarks not drawn on checkable menu items; #12360, themed icons. https://github.com/slint-ui/slint/issues?q=tray
- **Windows and overlay:**
  - `Window` has `always-on-top`, `no-frame` and `background`. **There is no click-through property.** https://docs.slint.dev/latest/docs/slint/reference/window/window/
  - Escape hatch: `WinitWindowAccessor` exposes the underlying `winit::Window`, which has `set_cursor_hittest`. https://docs.rs/i-slint-backend-winit/latest/i_slint_backend_winit/trait.WinitWindowAccessor.html
  - **winit's `WindowLevel` (always-on-top) is unsupported on Wayland.** https://docs.rs/winit/latest/winit/window/enum.WindowLevel.html
  - Popups as real OS windows (#1143, #6000) and modal windows (#6607) were still on the roadmap as of Oct 2025. https://slint.dev/blog/making-slint-desktop-ready

### egui / eframe
- **Version:** 0.37.0 (2026-10-15). Pre-1.0, with frequent breaking releases. https://crates.io/api/v1/crates/eframe
- **Idle:** "only repaints when there is interaction … or an animation". https://github.com/emilk/egui
- **Accessibility:** AccessKit is on by default. https://github.com/emilk/egui
- **Size:**
  - The default renderer became wgpu (#5889, closed 2025-11-13).
  - An almost-empty app measured **glow 4.29 MB vs wgpu 8.82 MB**.
  - Source: https://github.com/emilk/egui/issues/5889
- **Multi-window and overlay:**
  - Real OS windows ("viewports"). Immediate viewports waste CPU.
  - `ViewportBuilder` has `with_mouse_passthrough`, `with_transparent` and `with_window_level`.
  - Sources: https://docs.rs/egui/latest/egui/viewport/index.html · https://docs.rs/egui/latest/egui/viewport/struct.ViewportBuilder.html
- **Tray:** none built in; use `tray-icon`.

### iced
See **§11** for the detailed section.

### DDC crates

| Crate | Latest release | Notes |
|---|---|---|
| `ddc-hi` | **0.4.1, 2021-11-13** | Umbrella crate (`ddc-i2c`, `ddc-winapi` + `nvapi`, `ddc-macos`). Effectively unmaintained. |
| `ddc-winapi` | 0.3.0-alpha.2 / 0.2.2, 2024-07-22 | dxva2 wrapper |
| `ddc-i2c` | 0.2.2, 2023-02-16 | Linux `/dev/i2c-*` |
| `ddc-macos` | 0.2.2, 2024-11-05 | docs.rs failed to build it; pinned to `ddc 0.2` |

Sources: https://crates.io/api/v1/crates/ddc-hi · https://github.com/arcnmx/ddc-hi-rs · https://crates.io/api/v1/crates/ddc-winapi · https://crates.io/api/v1/crates/ddc-i2c · https://crates.io/api/v1/crates/ddc-macos · https://docs.rs/ddc-macos/latest/ddc_macos/

- **`ddc-macos` supports Apple Silicon.** Its `arm.rs`:
  - links the private `IOAVServiceCreateWithService`, `IOAVServiceReadI2C` and `IOAVServiceWriteI2C`;
  - matches `DCPAVServiceProxy` nodes with `Location == External`;
  - uses address 0xB7 for the M1 built-in HDMI.
  - Source: https://raw.githubusercontent.com/haimgel/ddc-macos-rs/master/src/arm.rs
- **MonitorControl's `Arm64DDC.swift`, for reference:**
  - Walks `DCPAVServiceProxy` nodes, uses chip address 0x37 with data register 0x51 and an XOR checksum.
  - Timing: 10 ms before each write, writes sent twice, 50 ms before a read, up to 5 retries.
  - Matches services to displays by a score built from EDID, location and serial.
  - Source: https://raw.githubusercontent.com/MonitorControl/MonitorControl/main/MonitorControl/Support/Arm64DDC.swift
  - Limits: no DDC over the M1 built-in HDMI, the entry-level M2 Mac mini HDMI, or DisplayLink. https://github.com/MonitorControl/MonitorControl
  - `IOAVServiceReadI2C` reads are reported unreliable in some cases, and App Store sandboxing blocks this private API. https://developer.apple.com/forums/thread/776442
- The DDC logic is small, so porting the existing C# DDC code to Rust over these private APIs is low-risk either way [unsourced judgement].

### Unverified
- That `tray-icon` (used for its click rect) coexists cleanly with Slint's winit loop in one process.
- The GNU FAQ wording on GPLv2+ combined with GPLv3, checked first-hand.
- XGrabKey through XWayland only firing while an XWayland window has focus [unsourced belief].
- Whether Slint `background: transparent` gives a truly transparent window on every platform.
- A measured Slint macOS arm64 size for this feature set. The only data point is the 9.0 MB third-party app.

---

## 2. Qt 6

**Takeaway:** Qt Widgets with dynamic linking is about 26 MB uncompressed on Windows and about 21 MB per architecture on macOS (10.8 MB and 8.4 MB zipped). Under 10 MB only works as a compressed download, or with a static Qt Lite build nobody has published numbers for. The licence is fine: qtbase is also offered under GPL-2.0-only. The gaps are Wayland global hotkeys and always-on-top, and the tray on stock GNOME.

### Size
These figures come from per-file sizes in the official PyQt6-Qt6 **6.11.2** wheels, which repackage Qt's own binaries. https://pypi.org/pypi/PyQt6-Qt6/json

**Windows x64** ([wheel](https://files.pythonhosted.org/packages/f6/56/62457dd9b5738f65b9fb4ac6b3120bc0bfa0bc335857fdd456b94b0a6079/pyqt6_qt6-6.11.2-py3-none-win_amd64.whl)):

| File | Uncompressed | Zipped |
|---|---|---|
| Qt6Core | 9.88 MB | 3.49 MB |
| Qt6Gui | 9.10 MB | 4.03 MB |
| Qt6Widgets | 6.20 MB | 2.80 MB |
| qwindows.dll | 0.95 MB | 0.41 MB |
| qmodernwindowsstyle | 0.23 MB | 0.11 MB |
| **Total** | **≈ 26.4 MB** | **≈ 10.8 MB** |

- The optional `opengl32sw.dll` (19.7 MB) and `d3dcompiler_47.dll` (4.0 MB) bring this to about 50 MB. That matches the long-standing "~50 MB windeployqt" forum figure. https://forum.qt.io/topic/89451/reducing-the-size-of-the-deployed-app
- windeployqt can leave these out with `--no-opengl-sw` and `--no-system-d3d-compiler`.
- Qt's Windows binaries don't need the ICU DLLs. https://doc.qt.io/qt-6/windows-deployment.html

**macOS** ([arm64 wheel](https://files.pythonhosted.org/packages/a0/72/f49e11b46eaa887e1250658b9c038f13c654ca7478c7c904ada13bb5133e/pyqt6_qt6-6.11.2-py3-none-macosx_11_0_arm64.whl)):
- arm64: QtCore 5.78 + QtGui 8.05 + QtWidgets 5.79 + libqcocoa 0.94 + libqmacstyle 0.25 ≈ **20.8 MB** (≈ 8.4 MB zipped).
- x86_64 comes to ≈ 22.8 MB, so a universal build is ≈ 43.6 MB (arithmetic, not measured).

**Linux** (Debian trixie installed sizes):
- libqt6core6t64 6.5 + libqt6gui6 11.3 + libqt6widgets6 8.6 + libqt6dbus6 0.97 ≈ 27 MB, plus qt6-wayland 1.0 MB.
- libicu76 is 37.4 MB. That only matters if the app bundles Qt itself, e.g. as an AppImage.
- Source: https://packages.debian.org/trixie/libqt6widgets6

**Static builds and Qt Lite:**
- Qt's "Reducing Binary Size" blog series (Qt 6.8–6.10, 2025) covers `-feature-*` options and static linking, but measures only Qt Quick, in charts. https://www.qt.io/blog/reducing-binary-size-of-qt-applications-part-3-more-platforms
- The only Qt 6 Widgets hello-world figure found is "16 MB" stripped (2021, not stated whether static). https://lists.qt-project.org/pipermail/interest/2021-August/037412.html
- Older Qt 5 static reports range from 4 to 15 MB. https://forum.qt.io/post/333990

### Licensing
- qtbase headers carry `LicenseRef-Qt-Commercial OR LGPL-3.0-only OR GPL-2.0-only OR GPL-3.0-only`. A GPL-2.0-or-later app can therefore use Qt under GPL-2.0 directly. https://github.com/qt/qtbase/blob/dev/src/widgets/kernel/qapplication.cpp
- FSF: LGPLv3 is compatible with GPLv3, so GPLv2+ code combines with it under v3. https://www.gnu.org/licenses/license-list.en.html
- Qt's LGPL obligations: with static linking you must let users relink. https://www.qt.io/licensing/open-source-lgpl-obligations
  - A GPL app that ships its full source already meets that, so static linking is fine [inference].
- Some add-on modules are GPLv3-only; Core, Gui, Widgets and DBus are not among them. https://doc.qt.io/qt-6/licensing.html

### Tray
- `QSystemTrayIcon` on Linux supports StatusNotifierItem (KDE, GNOME, Xfce, LXQt, DDE) and the X11 XEmbed tray. https://doc.qt.io/qt-6/qsystemtrayicon.html
- Stock GNOME still needs the AppIndicator extension; see §9.

### Global hotkeys
- Qt has no global-hotkey API [unsourced; none found].
- **QHotkey** (Skycoder42, BSD-3):
  - Last release is 1.5.0, "Qt 6 support", from 2021-10-28. Commits continue to 2025-09-30.
  - **"Wayland is not supported."**
  - Sources: https://github.com/Skycoder42/QHotkey · https://github.com/QtCommunity/QHotkey
- On Wayland you'd write a GlobalShortcuts portal client with QtDBus. https://flatpak.github.io/xdg-desktop-portal/docs/doc-org.freedesktop.portal.GlobalShortcuts.html

### Always-on-top overlay on Wayland
- xdg-shell "doesn't allow windows to position themselves, which is why the Qt API for it doesn't work on Wayland". The workaround is layer-shell-qt, which works on KDE only. https://discuss.kde.org/t/kde-plasma-support-for-qt-windowstaysontophint-flag-in-wayland/3106
- layer-shell-qt 6.x is packaged by distros. https://packagehub.suse.com/packages/layer-shell-qt6

### Qt as a system library
- On Ubuntu, `libqt6core6` is in *universe*, i.e. not part of the default GNOME install [inference from component label]. https://packages.ubuntu.com/jammy/libqt6core6
- Plasma 6 distros (Fedora KDE, Kubuntu, KDE neon) necessarily ship Qt 6 [unsourced].

### Unverified
- Static Qt Lite Widgets sizes per OS.
- The Qt 6.12.0 release date.
- KGlobalAccel being KDE-session-only.
- Which distros install Qt 6 by default.

---

## 3. GTK 4 on Windows and macOS

**Takeaway:** GTK4 is over budget on Windows and macOS once its runtime is bundled. There is no standard bundler, its widgets are non-native, it has **no tray API on any OS**, and the macOS backend only gained accessibility and native window controls in 4.18 (Feb 2025).

- **Bundle size:**
  - No published figure for a bundled GTK4 Windows app.
  - The nearest data point is a GTK3 MSYS2 app: a 128 kB exe plus about 22 MB of stripped DLLs, plus about 3 MB per locale. https://discourse.gnome.org/t/how-to-build-distribution-package-for-windows/11454
  - The MSYS2 `gtk4` package alone installs 81 MB before glib, pango and cairo. https://packages.msys2.org/packages/mingw-w64-x86_64-gtk4
  - The gvsbuild 2026.10.0 GTK4 x64 zip is 285 MB (full SDK). https://github.com/wingtk/gvsbuild/releases/expanded_assets/2026.10.0
- **No standard bundler on Windows.** You copy icon themes, GSettings schemas and pixbuf loaders by hand. gvsbuild calls its own binaries untested. https://github.com/wingtk/Gvsbuild
- **macOS:** gtk-mac-bundler misses libraries loaded with dlopen. No GTK4 `.app` size has been published. https://github.com/GNOME/gtk-mac-bundler
- **macOS maturity:**
  - GTK 4.18 added AccessKit accessibility on Windows and macOS "for the very first time", and raised the minimums to macOS 10.15 and Windows 10. https://blogs.gnome.org/gtk/2025/02/01/whats-new-in-gtk-winter-2025-edition/
  - Native window controls became opt-in in 4.18. https://en.ubunlog.com/Gnome-improves-its-applications-on-MacOS-among-the-highlights-of-this-week/
  - 4.20.x fixed a macOS dyld startup error (#7734). https://lists.freebsd.org/archives/freebsd-desktop/2026-March/007647.html
- **Tray:**
  - GtkStatusIcon was removed in GTK4. GNOME points to GNotification or the Background portal instead. https://discourse.gnome.org/t/system-tray-icons-in-gtk4/22615
  - libayatana-appindicator is GTK3-only, so projects implement StatusNotifierItem over D-Bus themselves. https://zff.dev/chewing/ibus-chewing/issues/177

**Unverified:** actual Windows and macOS bundle sizes (estimated 30–50 MB [unsourced]).

---

## 4. wxWidgets 3.2 / 3.3

**Takeaway:** wxWidgets uses native controls on every OS. 3.3.4 and 3.2.12 (2026-10-10) **added an AppIndicator/SNI tray, so it works on Wayland**. 3.3 has Windows dark mode, and Apple Silicon has been supported since 3.1.5. `RegisterHotKey` works on MSW and macOS only. There are no current static-size numbers for C++ wx, but the Rust wxDragon binding builds to 3.8–7.2 MB on macOS (§10).

- **Releases:**
  - 3.3.0 on 2025-06-06, 3.3.2 on 2026-03-03, 3.3.3 on 2026-07-07, **3.3.4 on 2026-10-10**, and 3.2.12 the same day.
  - 3.3 is "suitable to be used in production".
  - Supports Windows 7–11, GTK 3.6+, and macOS 10.10 to 27 on x86-64 and ARM.
  - Sources: https://raw.githubusercontent.com/wxWidgets/wxWidgets/v3.3.4/docs/changes.txt · https://raw.githubusercontent.com/wxWidgets/wxWidgets/v3.3.4/docs/readme.txt
- **Tray:**
  - The 3.3.4 readme says "wxTaskBarIcon also works under Wayland now (with AppIndicator)".
  - Implemented in PR #26845 (merged 2026-08-22, fixes #25992). It uses libayatana-appindicator3 with a fallback to libappindicator3. Enable it with `--with-appindicator` or `wxUSE_APPINDICATOR`.
  - Sources: https://github.com/wxWidgets/wxWidgets/pull/26845 · https://raw.githubusercontent.com/wxWidgets/wxWidgets/v3.2.12/docs/changes.txt
  - **Caveat:** with AppIndicator, only `wxEVT_TASKBAR_LEFT_DCLICK` arrives, and only with AppIndicator ≥ 0.6. The system shows the menu. https://github.com/wxWidgets/wxWidgets/blob/v3.3.4/interface/wx/taskbar.h
- **Wayland:** 3.3.4 brings "Better Wayland support in wxGTK" (restoring window positions where the protocols exist, #26324), and `SetClassName` sets the Wayland app-id (#24668). Source: changes.txt above.
- **Dark mode on Windows:**
  - Experimental support since 3.3.0 (#23028).
  - Light/dark switching in 3.3.3 (#26516).
  - Dark common dialogs and switching at runtime in 3.3.4 (#26780, #26734).
  - Source: changes.txt.
- **Apple Silicon:** native ARM support since 3.1.5 (2021). 3.2.10 fixed issues on macOS 26. https://www.wxwidgets.org/news/2021/04/wxwidgets-3.1.5-released/ · https://wxwidgets.org/news/2026/03/wxwidgets-3.2.10-and-3.3.2-released
- **Hotkeys:** `RegisterHotKey` is "currently only implemented under MSW and macOS". https://github.com/wxWidgets/wxWidgets/blob/v3.3.4/interface/wx/window.h
- **Size:**
  - Debian: libwxgtk3.2 is 18.9 MB installed and libwxbase 3.7 MB. https://packages.debian.org/trixie/libwxgtk3.2-1t64
  - Static Windows figures are old: about 1.2 MB of wx code per the wiki. https://wiki.wxwidgets.org/Reducing_Executable_Size

**Unverified:** static C++ wx 3.3 sizes per OS, and `wxSTAY_ON_TOP` and transparency on Wayland (presumably the same xdg-shell limits as everyone else).

---

## 5. Tauri 2

**Takeaway:** the binaries are small on Windows and macOS: the docs claim under 600 KB, and installers run 2–6 MB. But:
- the Linux AppImage is 70+ MB;
- the WebView2 or WebKitGTK process tree costs about as much RAM as Electron;
- tray click events are not emitted on Linux;
- the global-shortcut plugin is X11-only on Wayland sessions.

So Tauri meets "< 10 MB shipped" on Windows and macOS but works against "lightweight".

- **Size:**
  - "a minimal Tauri app can be less than 600KB". https://v2.tauri.app/start/
  - AppImage size grows "from the 2-6 MB range to 70+ MB". https://v2.tauri.app/distribute/appimage/
- **Linux dependencies:**
  - webkit2gtk-4.1 (packaged on Debian/Ubuntu, Arch, Fedora, Gentoo and Alpine).
  - The tray needs libayatana-appindicator3 or libappindicator-gtk3.
  - WebView2 is preinstalled on Windows 10 1803+.
  - Source: https://v2.tauri.app/start/prerequisites/
- **Memory** (issue #5889, loading postman.com):

  | OS | Tauri | Electron |
  |---|---|---|
  | Windows | 399 MB | 318 MB |
  | Ubuntu | 581 MB | 240 MB |
  | macOS | 421 MB | 337 MB |

  An empty app on Ubuntu is 125 MB USS for Tauri vs 118 MB for Electron. https://github.com/tauri-apps/tauri/issues/5889
- **Startup:** Microsoft says a WebView2 cold launch "can introduce a noticeable delay". https://learn.microsoft.com/microsoft-edge/webview2/concepts/performance
- **Tray:** "Linux: Unsupported. The event is not emitted even though the icon is shown." https://v2.tauri.app/learn/system-tray/
- **Hotkeys:** the plugin uses `global-hotkey`, so the same X11-only limit and PRs #162 and #172 apply (§1). Tauri apps document this, e.g. https://pot-app.com/en/docs/wayland.html
- **Linux rendering:** known problems with DMABUF and NVIDIA. https://v2.tauri.app/develop/debug/linux-graphics/

**Unverified:** idle RAM and process count for a minimal Tauri tray app on Windows 11, and cold-start time.

---

## 6. Staying on .NET

**Takeaway:** **Avalonia with Skia can't get under 10 MB.** libSkiaSharp is 11.1 MB on Windows and 10.6 MB on Linux, before HarfBuzz or any .NET code. The .NET NativeAOT floor is about 1.4–1.8 MB, so .NET only meets the target as a NativeAOT core with a thin UI written separately for each OS. Of the ready-made UIs:
- Gir.Core is not AOT-ready;
- WinForms and WPF have no AOT;
- WinUI 3 supports AOT but is Windows-only and needs the Windows App SDK runtime;
- Eto.Forms has no AOT on any backend (§10).

### (a) Gir.Core
- Latest is 0.8.1 (2026-07-12); 0.9.0-preview.1 followed on 2026-09-15. https://github.com/gircore/gir.core/releases
- **NativeAOT is not supported.**
  - Issue #975, "allow trimming / native aot", has been open since 2023-11-13.
  - The blocker it links (2026-08-03) is #698, LibraryImport.
  - Source: https://github.com/gircore/gir.core/issues/975
- It presents itself as Linux-first ("write native dotnet linux apps"). Windows and macOS would need a bundled GTK runtime (§3). https://gircore.github.io/

### (b) WinForms, WPF and WinUI 3
- **WinForms:** trimming and AOT are still unsupported.
  - Epic #4649 is in milestone "Future".
  - "No updates on WinForms in dotnet 10" (2025-05-22).
  - "no update for AOT in WinForms not even in .net 11 Pre-release" (2026-09-14).
  - Source: https://github.com/dotnet/winforms/issues/4649
- **The SDK disables trimming for WinForms and WPF.** https://learn.microsoft.com/en-us/dotnet/core/deploying/trimming/incompatibilities
- **WPF:** #3811 is in milestone "Future". https://github.com/dotnet/wpf/issues/3811
- **WinUI 3:** NativeAOT since Windows App SDK 1.6 (2024-09-04). https://blogs.windows.com/windowsdeveloper/2024/09/04/whats-new-in-windows-app-sdk-1-6/
  - It still needs the Windows App Runtime, either installed machine-wide or bundled.
  - A WinUI 2.8 Islands NativeAOT template is 14.83 MiB. https://feed.nuget.org/packages/WinUIIslands/1.1.0

### (c) Avalonia: can Skia be dropped or shrunk?
- **Avalonia 12** shipped on 2026-04-07. It uses SkiaSharp 3.0, removed Direct2D1, and publishes no size figures. https://avaloniaui.net/blog/avalonia-12
- **Native library sizes** (Avalonia.Skia 12.1.3 pins SkiaSharp 3.119.4 and HarfBuzzSharp 8.3.1.3):

  | Library | win-x64 | linux-x64 | osx |
  |---|---|---|---|
  | libSkiaSharp | 11.09 MB | 10.65 MB | 14.50 MB universal (arm64 slice 6.83 MB) |
  | libHarfBuzzSharp | 1.73 MB | 2.68 MB | 2.79 MB |

  Sources: https://www.nuget.org/packages/SkiaSharp.NativeAssets.Win32 · https://www.nuget.org/packages/SkiaSharp.NativeAssets.macOS
- **No official non-Skia backend:**
  - A community port, MIR.Direct2D1ForAvalonia 12.1.0, is Windows-only with about 850 downloads. https://www.nuget.org/packages/MIR.Direct2D1ForAvalonia
  - Vello is only a proof of concept that "won't ship in 12.0". https://avaloniaui.net/blog/vello-poc
  - The Impeller partnership with Google was announced 2025-11-07; no package exists. https://avaloniaui.net/blog/avalonia-partners-with-google-s-flutter-t-eam-to-bring-impeller-rendering-to-net
- **Static linking** (Optris.StaticGraphics) produces a single exe but not a smaller one. https://www.nuget.org/packages/Optris.StaticGraphics.Avalonia.Software
- **Reported sizes:**
  - The AvaloniaAOT template is "~18MB" *with UPX*. https://github.com/lixinyang123/AvaloniaAOT
  - The official docs give no figures. https://docs.avaloniaui.net/docs/deployment/native-aot

### (d) NativeAOT size floor
- .NET 8 hello-world: **1.84 MB** on Linux x64 (stripped) and **1.77 MB** on Windows x64. https://learn.microsoft.com/en-us/dotnet/core/whats-new/dotnet-8/runtime
- .NET 10 hello-world is about **1.4 MB** on Ubuntu 26.04 (secondary summary). https://discourse.ubuntu.com/t/net-10-on-ubuntu-26-04-lts/80945
- **Size options:**
  - `OptimizationPreference=Size`.
  - `InvariantGlobalization`, `UseSystemResourceKeys` and `StackTraceSupport=false`.
  - `UseSizeOptimizedLinq` (.NET 10).
  - Sources: https://learn.microsoft.com/en-us/dotnet/core/deploying/native-aot/optimizing · https://learn.microsoft.com/en-us/dotnet/core/deploying/trimming/trimming-options
- **Reference points:**
  - bflat is 1–2 MB by default and 708 kB on Windows with every size option. https://github.com/bflattened/bflat
  - zerosharp is about 400 kB with its runtime. https://github.com/MichalStrehovsky/zerosharp

### (e) NativeAOT on osx-arm64
- Supported since .NET 8; needs Xcode Command Line Tools. https://learn.microsoft.com/en-us/dotnet/core/deploying/native-aot/
- **No cross-OS compilation**, so osx-arm64 must be built on a Mac. https://learn.microsoft.com/en-us/dotnet/core/deploying/native-aot/cross-compile
- Combine x64 and arm64 builds with `lipo` for a universal binary. https://docs.avaloniaui.net/docs/deployment/native-aot
- The .NET-for-macOS (AppKit) NativeAOT docs still say "experimental". https://github.com/dotnet/macios/blob/main/docs/nativeaot.md
- Several osx-arm64 NativeAOT codegen bugs were reported against 10.0.x. https://github.com/dotnet/runtime/issues/135616

### Tray and hotkey libraries
- **H.NotifyIcon:** "Trimming/NativeAOT support"; WPF, WinUI and Console. https://github.com/HavenDV/H.NotifyIcon
- **SimpleTrayIcon 3.0.2:** Windows-only, claims "fully AOT compatible". https://www-0.nuget.org/packages/SimpleTrayIcon
- **SharpHook 5.3.8** (libuiohook): Wayland support "lacks several features and requires elevated privileges". https://github.com/TolikPylypchuk/SharpHook

### Unverified
- Exact .NET 10 hello-world sizes for osx-arm64 and win-x64.
- Size of the ANGLE `av_libglesv2.dll`.
- Whether MIR.Direct2D1ForAvalonia drops HarfBuzz and works with AOT.

---

## 7. Other toolkits, briefly

**Fyne (Go).**
- Size: about 7.8 MB stripped on Windows (`-ldflags "-w -s"`), and 2.8 MB with UPX. That figure is from 2021, so current v2.7 sizes are unverified. https://github.com/fyne-io/fyne/issues/2393
- Tray: built in since v2.2 (`SetSystemTrayMenu`), plus `SetSystemTrayWindow` since v2.7. https://docs.fyne.io/explore/systray/
- Global hotkeys: none. The usual add-on, golang.design/x/hotkey, is **X11-only** on Linux. https://pkg.go.dev/golang.design/x/hotkey
- Widgets are not native.

**Flutter desktop.**
- `flutter_windows.dll` alone is about 11.6–12.2 MB uncompressed (6.7 MB compressed in Flutter 3.7). https://gist.github.com/loic-sharma/e35a59f7c8d5605dd94cb27e5bc59ef2 · https://www.dll-files.com/flutter_windows.dll.html
- Over 10 MB on Windows once the app and ICU data are added [inference].
- Tray: the `tray_manager` package needs ayatana on Linux. https://pub.dev/packages/tray_manager
- Hotkeys: `hotkey_manager` uses keybinder (X11) on Linux. https://pub.dev/documentation/hotkey_manager/latest/
- Flutter draws all of its own controls. https://docs.flutter.dev/resources/architectural-overview

**Dear ImGui.**
- Tiny: the 2017 demo exes were under 1 MB in total. https://skia.googlesource.com/external/github.com/ocornut/imgui/+/b148f70f83d6c3a5c01a53617a8a43f3df843bc9%5E%21
- **No accessibility**: screen readers read nothing (#4122, #8022, #7131). https://github.com/ocornut/imgui/issues/4122
- No tray, no native widgets.
- With GLFW 3.4, `GLFW_MOUSE_PASSTHROUGH` covers click-through. https://glfw.org/docs/3.4/news.html

**Lazarus/LCL (Free Pascal).**
- Native widgets on each OS:

  | OS | Lazarus 4.8 (2026-06) widgetsets |
  |---|---|
  | Windows | win32 |
  | macOS | Cocoa 10.12+ (aarch64 builds since 4.0RC1, 2024-10) |
  | Linux | gtk2, qt5, qt6 (GTK3 still alpha) |

  Sources: https://lists.freepascal.org/pipermail/fpc-other/2026-June/000230.html · https://sourceforge.net/projects/lazarus/files/Lazarus%20macOS%20aarch64/
- Size: about 1.6 MB minimal on Win32 and 2.8 MB on macOS (2012 data). https://lists.lazarus-ide.org/pipermail/lazarus/2012-March/202440.html
- Tray: `TTrayIcon` tries AppIndicator first, then XEmbed, on Linux. https://wiki.lazarus.freepascal.org/TrayIcon
- Global hotkeys: none built in.

---

## 8. Native-per-OS reference points

**Takeaway:** Windows tools built on the OS runtime are tiny: Monitorian's MSI is 0.65 MB. The macOS menu-bar apps ship 16–29 MB DMGs, although MonitorControl 4.0.0 was 4.3 MB. Twinkle Tray, on Electron, is 92 MB. The closest analogue to this app, **Duja (Rust + Slint)**, is 7.6 MB on Windows, 9.9 MB on Linux and 16.3 MB as a universal macOS DMG.

Sizes are exact bytes from the GitHub releases API unless noted.

| App | Stack / licence | Latest | Shipped asset |
|---|---|---|---|
| MonitorControl (macOS) | Swift, MIT | v4.4.0, 2026-09-15 | DMG 20,391,423 B (v4.0.0 in 2021: 4,271,343 B) |
| Lunar (macOS) | Swift; Pro code encrypted | v6.11.3, 2026-10-25 | DMG 16,186,116 B |
| BetterDisplay (macOS) | proprietary | v5.1.1, 2026-10-06 | DMG 29,221,663 B |
| Twinkle Tray (Windows) | Electron, MIT | v1.17.2, 2025-12-25 | installer 92,407,640 B |
| Monitorian (Windows) | WPF on .NET Framework 4.8 (in the OS), MIT | 4.16.2, 2026-10-12 | MSI zip 648,603 B |
| ClickMonitorDDC (Windows) | closed freeware | 7.2 | about 233 KB (https://www.majorgeeks.com/files/details/clickmonitorddc.html) |
| ddcutil (Linux) | C, GPL-2.0 | 3.0.2, 2026-09-23 | Debian .deb 464 kB |
| ddcui (Linux) | Qt 5 (system Qt) | 0.6.0, 2025-02-17 | .deb 242 kB |
| Luminance (Linux) | C / GTK, GPL-3.0 | 1.5.0, 2026-06-23 | .deb 41,294 B |
| display-switch (all OSes, headless) | Rust, MIT | 1.4.1, 2025-09-28 | Windows 641 KB, macOS universal 1.46 MB, Linux 817 KB (zips) |
| **Duja** (all OSes: tray, flyout, hotkeys) | **Rust + Slint software renderer**, MIT/Apache | v0.1.6, 2026-08-08 | Windows zip 7,596,719 B; macOS universal DMG 16,301,777 B; Linux tar.gz 9,877,540 B |

Sources:
- https://github.com/MonitorControl/MonitorControl
- https://github.com/alin23/Lunar
- https://github.com/xanderfrangos/twinkle-tray
- https://github.com/emoacht/Monitorian
- https://packages.debian.org/trixie/amd64/ddcutil/download
- https://packages.debian.org/trixie/ddcui
- https://github.com/sidevesh/Luminance
- https://github.com/haimgel/display-switch
- https://github.com/itabajah/duja

- **Duja's README:**
  - The Linux tray "needs a StatusNotifierItem host … an extension away on GNOME".
  - "global hotkeys are not available on Linux at all".
  - It claims about 24 MB RAM.
  - Caveat: 0 stars and new, so treat it as a size data point only.
  - Source: https://github.com/itabajah/duja
- **Monitorian:** hotkeys are a paid Store add-on, so the free build has no global hotkeys. https://github.com/emoacht/Monitorian
- **MonitorControl:** uses the private `OSDManager` for the native macOS OSD. https://github.com/MonitorControl/MonitorControl/tree/main/MonitorControl/Support
- **KDE PowerDevil** already drives DDC through libddcutil (optional at build time, `WITH_DDCUTIL`):
  - Per-monitor brightness since Plasma 6.2. https://kde.org/announcements/plasma/6/6.2.0/
  - D-Bus interface `org.kde.ScreenBrightness.Display`. https://github.com/KDE/powerdevil/blob/master/CMakeLists.txt · https://github.com/KDE/powerdevil/tree/master/daemon/dbus
  - **This app and PowerDevil may contend for the same monitor.** Users work around DDC regressions with `POWERDEVIL_NO_DDCUTIL=1`. https://discuss.kde.org/t/ddc-ci-monitor-brightness-setting/42009
- **GNOME extension "Brightness control using ddcutil"** (JS, GPL-3.0, GNOME 49/50) needs `i2c-dev`, the udev rule and the i2c group. https://extensions.gnome.org/extension/2645/brightness-control-using-ddcutil/ · https://github.com/daitj/gnome-display-brightness-ddcutil

**Unverified:** why MonitorControl's DMG grew from 4.3 MB to about 20 MB, Twinkle Tray's installed size, and ClickMonitorDDC's licence.

---

## 9. Wayland reality check (independent of toolkit)

**Takeaway:**
- **Flyout and overlay:** on GNOME Wayland a client cannot place a flyout at the tray or keep an OSD on top. On KWin, Sway, Hyprland, COSMIC and niri, layer-shell solves both.
- **Click-through:** an empty input region works everywhere.
- **Global shortcuts:**
  - The portal works on KDE (5.27+), GNOME (48+) and Hyprland. It does not exist on wlroots or Sway.
  - xdg-desktop-portal 1.21+ requires a valid app ID.
- **Tray:** stock GNOME on Fedora has no tray host.

- **wlr-layer-shell support** (wayland.app):
  - Supported: KWin 6.7, Hyprland, COSMIC, niri, Sway 1.11, labwc, Wayfire.
  - **Not supported: Mutter (GNOME) 51**, Weston, Cage.
  - Source: https://wayland.app/protocols/wlr-layer-shell-unstable-v1
  - Mutter issue #973: https://gitlab.gnome.org/GNOME/mutter/-/issues/973. GNOME developers have "repeatedly refused" it (secondhand): https://zff.dev/dnkl/fuzzel/issues/303
- **No always-on-top request for xdg-toplevel:**
  - wayland-protocols MR !369, which proposed one, was opened 2024-12-15 and closed unmerged 2024-12-19. https://gitlab.freedesktop.org/wayland/wayland-protocols/-/merge_requests/369
  - xdg-toplevel-tag-v1 (wayland-protocols 1.43, 2025-04-08; merged in Mutter for GNOME 49) only gives the compositor or user a stable tag to hang window rules on. https://phoronix.com/news/Wayland-Protocols-1.43 · https://www.phoronix.com/news/Mutter-Merges-Toplevel-Tag
  - xx-zones-v1, which allows client positioning, has "No compositor support". https://wayland.app/protocols/xx-zones-v1
- **Click-through:** `wl_surface.set_input_region` is core protocol, so an empty region means no input. https://wayland.app/protocols/wayland#wl_surface:request:set_input_region
- **GlobalShortcuts portal (v2):**
  - The app calls `CreateSession` then `BindShortcuts`.
  - `preferred_trigger` is only a hint, and a session's shortcuts can be bound only once.
  - Source: https://flatpak.github.io/xdg-desktop-portal/docs/doc-org.freedesktop.portal.GlobalShortcuts.html
- **xdg-desktop-portal 1.21.0 (2026-01-21):**
  - Added `ConfigureShortcuts`.
  - **"Require a valid AppID from apps to use the Global Shortcuts Portal" (#1817).** The same release also requires a valid AppID for background/autostart.
  - Source: https://github.com/flatpak/xdg-desktop-portal/blob/main/NEWS.md
- **Unsandboxed apps must register:**
  - Call `org.freedesktop.host.portal.Registry.Register(app_id)` **before any other portal call**.
  - The app ID must match a `.desktop` file basename.
  - Source: https://flatpak.github.io/xdg-desktop-portal/docs/doc-org.freedesktop.host.portal.Registry.html
  - **Check this repo's existing portal code for that.**
- **GNOME:**
  - Backend added in xdg-desktop-portal-gnome 48.rc; activation token fixed in 50.alpha. https://github.com/GNOME/xdg-desktop-portal-gnome/blob/main/NEWS
  - GNOME 48 release notes: global shortcuts "fully supported". https://release.gnome.org/48/developers/
- **KDE:** Plasma 5.27 "gained support for the Global Shortcuts portal". https://kde.org/announcements/plasma/5/5.27.0/
- **Hyprland:** works via XDPH, but the user binds keys in the config. https://wiki.hypr.land/hypr-ecosystem/user/xdg-desktop-portal-hyprland/
- **wlroots:** xdg-desktop-portal-wlr has no GlobalShortcuts. https://github.com/emersion/xdg-desktop-portal-wlr
- **Tray (SNI) on GNOME:**
  - Fedora Workstation has never shipped a tray host by default. Issue #246 was only migrated to Fedora Forge in 2026-02, never resolved; upstream wants a new freedesktop spec first. https://pagure.io/fedora-workstation/issue/246
  - Ubuntu ships the appindicator extension. https://discourse.ubuntu.com/t/status-of-the-top-icons/495
  - GNOME's simplified status-icon spec is still only a design draft. https://gitlab.gnome.org/Teams/Design/os-mockups/-/issues/150
- **Native OSD fallbacks:**
  - **KDE:** D-Bus `org.kde.plasmashell /org/kde/osdService`, with `brightnessChanged(int)`, the per-display `screenBrightnessChanged(...)` and `showText(icon, text)`. https://github.com/KDE/plasma-workspace/blob/master/shell/osd.h
  - **GNOME:** `org.gnome.Shell.ShowOSD` is limited to an allow-list of callers (since GNOME 41), so third parties get `AccessDenied`. https://github.com/GNOME/gnome-shell/blob/main/js/ui/shellDBus.js · https://forum.manjaro.org/t/unable-to-call-org-gnome-shell-showosd-dbus-accessdenied/96605

**Unverified:**
- That XWayland windows on GNOME still honour `_NET_WM_STATE_ABOVE` and absolute position, which would be a possible fallback.
- Whether current Ubuntu 25.10–26.10 still enables appindicator by default.

---

## 10. Native-widget cross-platform frameworks

**Takeaway:**
- **wxWidgets is the strongest truly native option**: C++ directly, or Rust via **wxDragon**. wxDragon:
  - is actively released;
  - wraps wx 3.3.4 statically;
  - has `TaskBarIcon`;
  - builds to 3.8–7.2 MB on macOS.
- **Lazarus/LCL is second.**
- Every other candidate fails at least one of: having a Linux backend, being truly native, being maintained, supporting AOT (so it can stay small), or having a Wayland tray.
- **No native framework solves Wayland hotkeys or the GNOME overlay** (§9).

**What "native" means:**
- **Qt Widgets** draws its own widgets through QStyle, which uses native theme engines on Windows and Mac to "look exactly like the equivalent native widgets". So Qt imitates native look rather than wrapping native controls. https://doc.qt.io/qt-6/qstyle.html
- **Flutter** has "its own implementations of each UI control". https://docs.flutter.dev/resources/architectural-overview
- **Slint, egui and iced** draw everything themselves.
- **Linux has no single OS toolkit.** "Native" means GTK on GNOME and Qt on KDE, and the two are "equally native". https://github.com/andlabs/libui/issues/30 (no freedesktop or HIG statement found)

| Framework | Language | Windows | macOS | Linux | Truly native? | Small-app size | Last release | Tray | Global hotkey | A11y | GPL-2.0+ fit |
|---|---|---|---|---|---|---|---|---|---|---|---|
| **wxDragon** (wx 3.3.4) | Rust | Win32 | Cocoa | GTK3 | **Yes** | macOS stripped + LTO: 7.20 MB default, 4.51 MB with features off, 3.81 MB with #224 | 0.9.24, 2026-10-26 | **Yes**, `TaskBarIcon` | Not exposed (needs wx FFI; MSW and macOS only) | Native, plus `set_accessibility_*` | MIT OR Apache-2.0: take **MIT** |
| wxWidgets (C++) | C++ | Win32 | Cocoa | GTK3 | **Yes** | old: about 1.2 MB of wx code | 3.3.4, 2026-10-10 | Yes, AppIndicator on Wayland | MSW and macOS | Native | wxWindows Licence (LGPL + exception) [https://www.wxwidgets.org/about/licence/ not re-fetched] |
| **Lazarus/LCL 4.8** | Object Pascal | win32 | Cocoa (aarch64) | gtk2 / qt5 / qt6 | **Yes** | about 1.9–2.9 MB (2009–2012 data) | 4.8, 2026-06 | Yes, AppIndicator | No | Native [unsourced] | LCL is modified LGPL with static-link exception [unsourced] |
| Eto.Forms 2.13 | C# | WinForms / WPF / Direct2D | Cocoa (Eto.macOS) | GTK3 (GtkSharp) | Mostly (some WPF controls are Eto-drawn since 2.12) | No AOT on any backend, so likely ≫ 10 MB [unverified] | 2.13.0, 2026-10-17 | Yes, `TrayIndicator` | No | Native [unsourced] | BSD-3: OK |
| libui-ng | C | Win32 | Cocoa | GTK3 | Yes | small [unverified] | **No releases**; last commit 2025-03-15; "mid-alpha" | **No** (#28) | No | minimal | MIT |
| Yue | C++ / JS / Lua | **Windowless, UxTheme-drawn** | Cocoa | GTK3 | Partly | libyue about 4.3 MB zip | v0.15.6, 2025-01-24 (last push) | Yes (AppIndicator only) | Yes (Linux X11 only) | ? | LGPL-2.1 with static exception: OK |
| IUP | C / Lua | Win32 | **No official Cocoa** | GTK / Motif | Yes on Windows and GTK | ? | 3.32, 2025-01-06; "on hold since 2023" | GTK < 3.14 only | No | ? | MIT |
| SWT | Java | Win32 | Cocoa | GTK3 | Yes | jar about 2.4–3.1 MB **plus a JVM** (jlink image about 23 MB) | 3.135.0, 2026-09-07 | GtkStatusIcon, so **none on GTK4 or Wayland** | No | Yes [unsourced] | **EPL-2.0, GPL-incompatible** |
| React Native Windows / macOS | JS / C++ | WinAppSDK Composition (**not** XAML controls) | AppKit | **None** | Windows no | large [unverified] | RNW 0.85.0, 2026-10-20; RN-macOS 0.83.0, 2026-09-30 | No | No | ? | MIT, but **no Linux** |
| .NET MAUI | C# | WinUI 3 | **Mac Catalyst** (UIKit) | **None** | Windows yes; Mac is Catalyst | ? | .NET 10 | Add-on (Windows) | No | ? | **No Linux** |
| native-windows-gui | Rust | Win32 | – | – | Windows only | – | 1.0.13, 2022 ("done") | Yes | – | – | MIT |
| cacao | Rust | – | AppKit | – | macOS only | – | 0.4.0-beta2, 2023 | ? | – | – | MIT / Apache |
| gtk4-rs / relm4 | Rust | GTK, not native | GTK, not native | GTK4 | GNOME only | runtime bundle on Windows and macOS | gtk4 0.11.5, 2026-09-20 | **None** in GTK4 | No | GTK | MIT |
| `libui` crate (over libui-ng) | Rust | Win32 | Cocoa | GTK3 | Yes | small | 0.5.0, 2026-09-04 | No | No | – | MIT |

Findings by framework:
- **wxDragon**
  - Releases: 0.9.22 (2026-09-17), 0.9.23 (2026-10-15), 0.9.24 (2026-10-26). https://crates.io/api/v1/crates/wxdragon
  - Builds wx **3.3.4** and links it statically. https://github.com/AllenDang/wxDragon/blob/main/rust/wxdragon-sys/build.rs
  - Modules include `taskbar_icon`, `notification_message`, `single_instance_checker` and `appearance` (dark mode). https://docs.rs/wxdragon/latest/wxdragon/
  - The `WxWidget` trait has accessibility setters but **no hotkey or transparency methods**. https://docs.rs/wxdragon/latest/wxdragon/window/trait.WxWidget.html
  - The size figures come from issue/PR #225. https://github.com/AllenDang/wxDragon/issues/225
  - Prebuilt wx libraries cut build time to under 3 minutes. https://docs.rs/crate/wxdragon/latest
- **Other wx Rust bindings:** wxRust2 is dormant (0.0.1-alpha, 2022). https://github.com/kenz-gelsoft/wxRust2
- **Lazarus Cocoa:** in 2023 the Cocoa maintainer had been inactive and about 130 patches were waiting. No 2025–26 maturity statement was found. https://lists.lazarus-ide.org/pipermail/lazarus/2023-April/240361.html
- **Eto.Forms**
  - **"None of the backends support AoT now"** (issue #2769, open since 2025-04). https://github.com/picoe/Eto/issues/2769
  - GtkSharp's AOT issue #300 has been open since 2021. https://github.com/GtkSharp/GtkSharp/issues/300
  - 2.12 added themes that follow system light/dark, and better Wayland support. https://github.com/picoe/Eto/releases/tag/2.12.0
  - The Linux tray uses libappindicator3 or ayatana. https://github.com/picoe/Eto/blob/develop/src/Eto.Gtk/Forms/LinuxTrayIndicatorHandler.cs
- **libui-ng:** README and status at https://github.com/libui-ng/libui-ng; the tray request is https://github.com/libui-ng/libui-ng/issues/28
- **Yue**
  - Windows controls are windowless and drawn with UxTheme. https://github.com/yue/yue/blob/main/docs/guides/faq.md
  - Tray: https://libyue.com/docs/latest/js/api/tray.html
  - The GTK hotkey implementation uses XGrabKey. https://github.com/yue/yue/blob/main/nativeui/gtk/global_shortcut_gtk.cc
- **IUP:** "on hold" since 2023: https://www.tecgraf.puc-rio.br/iup/en/home.html. Tray limits: https://www.tecgraf.puc-rio.br/iup/en/dlg/iupdialog.html
- **SWT**
  - On GTK4, `getSystemTray()` now returns null. https://github.com/eclipse-platform/eclipse.platform.swt/pull/3663
  - EPL-2.0 without the GPL secondary-licence designation is GPL-incompatible. https://www.gnu.org/licenses/license-list.html#EPL2
- **RNW:** Fabric on Windows renders with Composition. https://microsoft.github.io/react-native-windows/docs/new-architecture/
- **MAUI**
  - NativeAOT is documented for iOS and Mac Catalyst only. https://learn.microsoft.com/en-us/dotnet/maui/deployment/nativeaot?view=net-maui-10.0
  - Windows AOT support is still being worked on (#31227). https://github.com/dotnet/maui/issues/31227

**Unverified:**
- Windows and Linux sizes for wxDragon.
- Current Lazarus 4.8 hello-world sizes.
- Whether Eto.macOS can be published with NativeAOT.
- Accessibility depth for Eto, libui-ng, Yue, IUP, RNW and MAUI.

---

## 11. iced (and System76's libcosmic) in detail

**Takeaway:** Upstream iced is still **0.14.0 (2025-12-07)**; neither 0.15 nor 1.0 has shipped, though master (`0.15.0-dev`) gets commits daily. Upstream has **no accessibility, no tray and no global hotkeys**, and on Wayland it has no always-on-top or window positioning. There are open bugs affecting this app:
- unfocused windows don't present on Windows (#3320);
- theme changes don't fire on macOS (#3399);
- empty windows are GPU-hungry (#3331).

libcosmic is MPL-2.0 (GPL-compatible), but its Windows build has been broken since March 2026, it has no releases, and its smallest real apps install at 23–90 MB. **It does not fit the target.**

- **Version and roadmap:**
  - Latest is 0.14.0 (2025-12-07); before that, 0.13.1 (2024-09-19). The README still says "experimental software". https://github.com/iced-rs/iced/releases · https://docs.rs/crate/iced/latest
  - Master is `0.15.0-dev`. Default features include `wgpu`, `tiny-skia`, `x11` and `wayland`; there is **no a11y feature**. https://github.com/iced-rs/iced/blob/master/Cargo.toml
  - hecrj committed through 2026-10-21, including a wgpu bump (#3420). https://github.com/iced-rs/iced/commits/master
  - ROADMAP.md only links to an external board. https://github.com/iced-rs/iced/blob/master/ROADMAP.md
- **Upstream vs the pop-os fork:**
  - The fork is at 0.14.0. Its defaults are `a11y`, `tiny-skia`, `tokio`, `wayland` and `x11`, and it uses its own winit, sctk and cctk. https://github.com/pop-os/iced/blob/master/Cargo.toml
  - The fork has **284 commits** upstream lacks (https://github.com/iced-rs/iced/compare/master...pop-os:iced:master); upstream has **676** the fork lacks (https://github.com/pop-os/iced/compare/master...iced-rs:iced:master).
  - Fork-only work includes Wayland popups and layer surfaces, session lock, and "do not redraw on mouse move". https://github.com/pop-os/iced/commits/master
  - It is described as a soft fork, rebased on each upstream release (secondhand, HN). https://news.ycombinator.com/item?id=46185323
  - libcosmic pulls iced in as an unpinned submodule, which has broken builds. https://github.com/pop-os/libcosmic/blob/master/Cargo.toml · https://github.com/pop-os/libcosmic/issues/1152
- **Renderers and size:**
  - Two renderers: `iced_wgpu` and the software `iced_tiny_skia`.
  - 0.14 defaults wgpu to HighPerformance (#2813), which may wake a discrete GPU [inference]. https://github.com/iced-rs/iced/releases/tag/0.14.0
  - **Measured sizes:**

    | Year, platform | Build | Size | Notes |
    |---|---|---|---|
    | 2022, Windows, counter app | release | 6 MB | |
    | | `opt-level=z` + LTO | 3.1 MB | |
    | | glow instead of wgpu | 1.5 MB | 27 vs 76 MB RAM |
    | Dec 2025, HN anecdote | tiny-skia counter | 4 MB | 7 MB RAM |
    | 2023, Linux, `todos` | release | 17 MB | about 230 ms startup |

    Sources: https://github.com/iced-rs/iced/discussions/1531 · https://news.ycombinator.com/item?id=46185323 · https://lukaskalbertodt.github.io/2023/02/03/tauri-iced-egui-performance-comparison.html
  - **Real installed sizes on Arch** (include assets, so they are upper bounds):

    | Package | Installed size |
    |---|---|
    | cosmic-osd 1.10.0 | 23.6 MB |
    | cosmic-app-library | 33.8 MB |
    | cosmic-text-editor | 40.9 MB |
    | cosmic-terminal | 42.4 MB |
    | cosmic-settings | 55.8 MB |
    | cosmic-files | 89.0 MB |
    | halloy (upstream iced) | 53.1 MB |
    | sniffnet (iced) | 45.1 MB |

    Sources: https://archlinux.org/packages/extra/x86_64/cosmic-osd/ · https://archlinux.org/packages/extra/x86_64/cosmic-files/ · https://archlinux.org/packages/extra/x86_64/halloy/ · https://archlinux.org/packages/extra/x86_64/sniffnet/
- **Accessibility:**
  - Upstream issue #552 has been open since 2020. https://github.com/iced-rs/iced/issues/552
  - Community PR #3281 was closed unmerged on 2026-03-14 ("I'll work on this myself"). https://github.com/iced-rs/iced/pull/3281
  - The pop-os fork's `iced_accessibility` uses a **wash2/accesskit git fork**, which fails to compile for Windows (E0782). https://github.com/pop-os/iced/blob/master/accessibility/Cargo.toml · https://github.com/pop-os/libcosmic/issues/1152
- **Idle CPU and redraws:**
  - Reactive rendering arrived with PR #2662 and is the default in 0.14. https://github.com/iced-rs/iced/pull/2662
  - **But:** #3331 reports about 25% GPU for an empty window on Windows (2026-05, no maintainer reply). https://github.com/iced-rs/iced/issues/3331
  - #3386: redraw is tied to `WM_PAINT`. https://github.com/iced-rs/iced/issues/3386
  - Wayland CPU reports: #2558 and #2616. https://github.com/iced-rs/iced/issues/2558
- **Windows, overlay and popups:**
  - `window::Settings` has `transparent`, `decorations`, `level` and `position`. https://docs.rs/iced/latest/iced/window/struct.Settings.html
  - `Level::AlwaysOnTop` exists, and so does `enable_mouse_passthrough`. https://docs.rs/iced/latest/iced/window/fn.enable_mouse_passthrough.html
  - winit doesn't support `WindowLevel` or `set_outer_position` on Wayland. https://docs.rs/winit/0.30.12/winit/window/enum.WindowLevel.html
  - **#3320** (open): on Windows, **unfocused windows don't present after a state change**. The reporter uses transparent undecorated windows, the same pattern as this app's OSD. https://github.com/iced-rs/iced/issues/3320
  - Layer-shell is available via `iced_layershell` 0.19.1 (iced ^0.14), which helps on KDE but not GNOME. https://docs.rs/crate/iced_layershell/latest
  - Dialogs need `rfd` or `iced_dialog`. https://jl710.github.io/iced-guide/faq.html
- **Tray and hotkeys:**
  - "Iced has currently no build in tray icon support". https://jl710.github.io/iced-guide/faq.html
  - tray-icon has a plain **winit** example (using `EventLoopProxy`), but there is no iced-specific example or crate. https://github.com/tauri-apps/tray-icon/blob/dev/examples/winit.rs
  - Use tray-icon's ksni backend to avoid the GTK loop.
  - `global-hotkey` is X11-only (§1).
- **Windows and macOS quality:**
  - 0.14 added IME support (#2777, #2790, #2918).
  - Open issues include #3189 (Windows IME), #3233 (macOS multi-window leak) and **#3399** (`theme_changes` doesn't fire on macOS 15.7, which breaks "follow OS light/dark"). https://github.com/iced-rs/iced/issues/3399
- **libcosmic outside COSMIC:**
  - The book claims "Windows, Mac" support. https://pop-os.github.io/libcosmic-book/introduction.html
  - The repo is MPL-2.0 with no releases. https://github.com/pop-os/libcosmic
  - Windows build broken on main since 2026-03-06 (#1152). https://github.com/pop-os/libcosmic/issues/1152
  - "No luck running libcosmic samples on windows". https://github.com/pop-os/libcosmic/discussions/489
  - No libcosmic app ships Windows or macOS binaries (e.g. Forecast is Flathub-only). https://github.com/cosmic-utils/forecast
  - MPL-2.0 combines with GPL as a "Larger Work". https://www.mozilla.org/en-US/MPL/2.0/FAQ/
- **COSMIC context:** Epoch 1 shipped with Pop!_OS 24.04 on 2025-12-11. The latest is epoch-1.11.0 (2026-10-20). https://en.wikipedia.org/wiki/COSMIC_desktop · https://github.com/pop-os/cosmic-epoch/releases

**Unverified:**
- The 0.15 or 1.0 date.
- A stripped, LTO size for a minimal iced 0.14 app on each OS.
- How a libcosmic app looks on Windows, macOS, GNOME or KDE.
- Whether AccessKit in the fork actually works with NVDA or VoiceOver.
