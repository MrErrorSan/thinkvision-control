# Lightweight cross-platform UI for ThinkVision Control

Research date: 2026-10-27/28. App version at the time: 0.0.3.

## Goal

- Under 10 MB shipped, and under 2 s cold start, on Windows 11 x64, macOS on Apple Silicon, and Linux (X11 and Wayland, GNOME and KDE).
- Stability and a small footprint now matter more than visual polish.

> **Update (same day, after look-alike prototypes):** wxWidgets with about 600 lines of custom-drawn Fluent controls now ranks first (80 vs 75). It measured 6.5 MB on both Windows and Linux, against Slint's 10.4 and 14.1 MB, and it uses the exact system accent. See [2026-10-28-ui-look-comparison.md](2026-10-28-ui-look-comparison.md). The analysis below is otherwise unchanged.

## Answer

1. **Rust + Slint (software renderer) ranks first.**
   - **Windows (measured):** 7.05 MB shipped (4.1 MB zipped). Opens in about 70 ms and idles at 3.8 MB private memory. Follows Windows dark mode and the accent colour.
   - **Linux (measured):** 11.3 MB shipped (5.8 MB tar.gz). Opens in about 45 ms warm and 128 ms cold.
   - **macOS:** not measured. Estimated at about 9 MB, based on a third-party Slint tray app.
2. **wxWidgets through Rust (wxDragon) ranks a close second.** It is the only mature option that uses each OS's own controls, like React Native does, on all three systems. It loses to Slint because its Windows and statically linked Linux sizes weren't measured, and because the Rust binding has one maintainer.
3. **Avalonia can't meet the target.** The NativeAOT-compiled minimal app is 31.6 MB on Windows, and the real app is 45 MB on Linux. Skia alone is 11 MB.
4. **There is a quick win before any rewrite.** The real app doesn't run under NativeAOT today: the shared core's JSON code relies on reflection, which NativeAOT disables. A 4 KB source-generated JSON patch fixes it. On Linux that shrinks the build from 104 MB to 45 MB and cuts warm start from 2.1 s to 0.2 s.

**Confidence:**
- *Moderate-high* that Slint or wxWidgets is the right family.
- *Moderate* on Slint over wxWidgets. The scores are close, and the wxWidgets gaps (Windows and statically linked Linux sizes) weren't measured.
- *Low* on every macOS number: there was no Mac to measure on.

---

## What was measured, and how

| Track | What | Where |
|---|---|---|
| Windows probes | 16 builds: Windows 11, Ryzen 5 5500U, AMD iGPU + GTX 1650, dark mode | [windows/results.json](lightweight-ui-probes/windows/results.json), sources in [windows/](lightweight-ui-probes/windows/) |
| Linux probes | 19 builds: Ubuntu 24.04 on WSL2/WSLg (software GL) | [linux/results.json](lightweight-ui-probes/linux/results.json), sources in [linux/](lightweight-ui-probes/linux/) |
| Web research (Claude) | macOS sizes, Wayland, tray/hotkey libraries, licences, maintenance, native-widget frameworks, iced/libcosmic | [web-findings.md](lightweight-ui-probes/web-findings.md) |
| Gemini, independent | Same question, separate investigation | [gemini-raw.md](lightweight-ui-probes/gemini-raw.md) (verbatim), [gemini-brief.md](lightweight-ui-probes/gemini-brief.md) |

**The probe window** was the same in every toolkit:
- 600×400 window
- a label
- a slider from 0 to 100
- a toggle switch, or a checkbox where the toolkit has none
- a combo box with 3 items
- a button

**Builds:** release, optimised for size (Rust: `opt-level` s/z, LTO, strip, `panic=abort`).

**Caveats:**
- **Windows "first run":** not a true cold start, since the file cache wasn't flushed.
- **Linux numbers:** comparable with each other but not with Windows. WSLg renders in software through llvmpipe, which inflates memory and thread counts for anything on a GL/Vulkan path.
- **macOS:** nothing was measured; all macOS figures are from published sources.

### Windows 11 (measured)

| Build | Shipped MB | Zipped MB | Warm start ms | Private MB | Working set MB |
|---|---|---|---|---|---|
| **Slint 1.18.1, software renderer** | **7.05** | 4.10 | **64–87** | **3.8** | 20.0 |
| Slint, default (femtovg/OpenGL) | 7.56 | 4.36 | 307–890 | 91.0 | 88.1 |
| iced 0.14, tiny-skia | 2.14 | 1.00 | 110 | 4.7 | 18.3 |
| iced 0.14, default (wgpu) | 4.75 | 2.19 | 2,120–2,150 | 264.4 | 220.9 |
| egui 0.37, glow | 4.48 | 1.98 | 238–268 | 92.4 | 91.7 |
| C# NativeAOT + raw Win32 controls | 0.73 | 0.39 | 46–91 | 3.3 | 14.6 |
| Qt 6.8.3 Widgets, pruned deploy | 22.03 | 10.00 | 142–158 | 6.2 | 36.0 |
| Qt Quick/QML | 52.90 | 21.83 | 300–317 | 42.0 | 66.0 |
| Avalonia 12.1.3 minimal, NativeAOT | 31.55 | 13.90 | 1,502–1,655 | 81.8 | 86.2 |
| Avalonia real app, NativeAOT | 41.37 | 18.06 | **crashes** (JSON reflection) | – | – |
| **WPF app today** (framework-dependent) | 0.98 + 171 MB runtime | 0.43 + 60 MB runtime download | 906–1,318 | 74.7 | 147.4 |
| Rust tray only (tray-icon + global-hotkey) | 0.23 | 0.12 | – | 1.6 | 10.6 |

### Linux, Ubuntu 24.04 under WSLg (measured)

"Must bundle" means libraries a default desktop doesn't already have, which the app would have to ship or depend on.

| Build | Shipped MB | tar.gz MB | Must bundle | Cold ms | Warm ms | PSS MiB |
|---|---|---|---|---|---|---|
| **Slint, software renderer** | **11.30** | 5.77 | nothing | **128** | **45** | **11.3** |
| Slint, default | 12.98 | 6.51 | nothing | 632 | 161–246 | 122.7 |
| iced, tiny-skia | 4.76 | 2.05 | nothing | 128 | 36–38 | 8.0 |
| iced, default (wgpu) | 8.61 | 3.68 | nothing | 490 | 145–152 | 133.8 |
| egui, glow | 8.69 | 3.79 | nothing | 334 | 103–112 | 113.1 |
| libcosmic | 13.47 | 5.98 | nothing | 739 | 192–207 | 141.9 |
| wxWidgets 3.2 (GTK3), dynamic | 0.11 | 0.02 | libwx (~10.5 MB) | 214 | 72–82 | 33.6 |
| Qt 6 Widgets, dynamic | 0.02 | 0.005 | Qt 6 (~21 MB; already present on KDE) | 225 | 48–55 | 25.0 |
| Lazarus/LCL, GTK3 | 2.43 | 0.82 | nothing | 135 | 58–63 | 62.2 |
| GTK 4 (C) | 0.01 | 0.003 | nothing on GNOME | 1,083 | 226–330 | 180.8 |
| Gir.Core GTK4 (C#, NativeAOT) | 2.76 | 1.18 | nothing | 495 | 212–240 | 185.7 |
| Eto.Forms (GTK3), NativeAOT | 44.32 | 15.03 | nothing | 235 | 127–130 | 65.6 |
| **Real app, NativeAOT + JSON patch** | **45.03** | 18.77 | nothing | **542** | **187–216** | 142.1 |
| **Real app, as shipped today** | **104.10** | 42.96 | nothing | **2,602** | **2,093–2,173** | 223.6 |

**Notes on these numbers:**
- **Skia is the size floor for Avalonia.** It ships `libSkiaSharp` at 11.1 MB on Windows and 11.2 MB on Linux, plus HarfBuzz at 1.7 and 2.8 MB. That alone is over the target before any app code.
- **GPU renderers cost more than they save for a small settings window.**
  - iced on wgpu loads Vulkan, D3D12, D3D11 and OpenGL drivers for both GPUs and idles at 264 MB.
  - Avalonia's default ANGLE path loads the NVIDIA driver, which probably wakes the discrete GPU.
  - Software renderers are both faster to start and 20–60× lighter in memory.
- **Slint's Linux binary (11.3 MB) is bigger than its Windows one (7.05 MB).** The Linux build used `opt-level="s"`, while Windows used `"z"`. The Linux build also carries X11, Wayland and font code. It has not been profiled with `cargo bloat`, so how far it shrinks is open.

---

## Ranking

### Scoring

Each option is scored 0–5 per criterion and weighted. The weights follow your priority: size, speed and stability first, look second.

| Criterion | Weight | What 5 means |
|---|---|---|
| Shipped size | 20 | Under 5 MB on every OS, nothing to bundle |
| Startup | 10 | Under 150 ms warm, under 500 ms cold |
| Idle memory | 10 | Under 15 MB private / PSS |
| Stability and maturity | 15 | Stable API, funded or long-lived, no blocking open bugs |
| Light/dark mode | 8 | Follows the OS automatically on all three |
| Native look | 5 | Uses or closely matches each OS's own controls |
| Tray, hotkeys, OSD | 10 | All three possible without fighting the toolkit (Wayland limits apply to everyone; see below) |
| Accessibility | 7 | Screen readers work on all three |
| Rewrite cost | 10 | Little or no code to rewrite |
| Licence | 5 | Fits GPL-2.0-or-later with no conditions |

### Results

| # | Option | Size | Start | Mem | Stable | Dark | Native | Tray/keys/OSD | A11y | Rewrite | Licence | **Score /100** |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| 1 | **Rust + Slint** (software renderer) | 4 | 5 | 5 | 4 | 4 | 3 | 4 | 4 | 2 | 4 | **79.0** |
| 2 | **Rust + wxWidgets** (wxDragon, static) | 4 | 5 | 4 | 3 | 3 | 5 | 4 | 5 | 2 | 5 | **76.8** |
| 3 | Lazarus/LCL (Free Pascal) | 5 | 5 | 4 | 3 | 2 | 5 | 3 | 4 | 1 | 5 | 73.8 |
| 4 | C# NativeAOT core + a separate native UI per OS | 4 | 4 | 3 | 3 | 2 | 4 | 5 | 5 | 2 | 5 | 72.2 |
| 5 | Qt 6 Widgets (C++) | 1 | 5 | 4 | 5 | 4 | 4 | 4 | 5 | 1 | 5 | 69.4 |
| 6 | Rust + iced (tiny-skia) | 5 | 5 | 5 | 2 | 3 | 1 | 2 | 1 | 2 | 5 | 66.2 |
| 7 | Rust + egui (glow) | 4 | 4 | 2 | 3 | 4 | 1 | 3 | 3 | 2 | 5 | 63.6 |
| 8 | Stay on Avalonia, add NativeAOT | 0 | 3 | 1 | 4 | 5 | 3 | 5 | 4 | 5 | 5 | 61.6 |
| 9 | Eto.Forms (C#, native widgets) | 0 | 3 | 3 | 3 | 3 | 4 | 3 | 4 | 3 | 5 | 52.4 |

Options 1 and 2 are within scoring noise. Pick Slint if a modern look, declarative UI files and fast iteration matter more. Pick wxWidgets if real native controls and screen-reader depth matter more.

**Ruled out before scoring:**
- **Tauri:** small binary, but browser-engine memory. Measured elsewhere at 125 MB on Ubuntu for an empty app; the Linux AppImage is 70+ MB.
- **GTK 4 everywhere:** no tray API, and 30–80+ MB to bundle on Windows and macOS.
- **Flutter:** `flutter_windows.dll` alone is about 12 MB.
- **Fyne:** non-native look; hotkeys are X11-only.
- **Dear ImGui:** no screen-reader support.
- **libcosmic:** Windows build broken since 2026-03; real apps are 23–89 MB.

### Everyday features compared

| | Slint | wxWidgets | Lazarus | C# + native per OS | Qt Widgets | iced | egui | Avalonia (today) |
|---|---|---|---|---|---|---|---|---|
| Follows OS light/dark | Yes on Windows, with accent (measured); macOS yes; Linux via portal *(verify)* | macOS yes; Windows yes from 3.3.4 (new); Linux follows the GTK theme, which may not follow GNOME's dark switch | Windows **no**; macOS yes; Linux follows GTK theme | Windows **no** with raw Win32 (measured: classic light controls); macOS and GTK yes | Yes on Windows (measured) and macOS; Linux depends on the platform theme | Windows yes; macOS **bug #3399** | Yes (its own palette) | Yes |
| Looks like the OS | Imitates: Fluent style on Windows, Cupertino on macOS | **Real native controls** | **Real native controls** (classic Win32) | **Real native controls**, but classic Win32, not Windows 11 Fluent | Imitates very closely | Own look | Own look | Fluent re-creation everywhere |
| Toggle switch | Yes | Yes (3.3 `wxSwitchCtrl`) | Checkbox | Checkbox on Win32 | Checkbox | Yes | Checkbox | Yes |
| Tray icon | Built in (1.17+), but no click position; use the `tray-icon` crate for that | Built in; AppIndicator works on Wayland since 3.3.4 | Built in | Existing repo code | Built in | Add-on crate | Add-on crate | Existing repo code |
| Global hotkeys | Our code, as now | Built in on Windows/macOS; our code on Linux | Our code | Existing repo code | Our code | Our code | Our code | Existing repo code |
| Transparent always-on-top OSD | Yes, plus click-through via winit | Yes | Partial | Existing repo code | Yes | Windows bug #3320 | Yes | Yes |
| Screen readers | AccessKit (UIA, NSAccessibility, AT-SPI) | Native | Native | Native | Native | **None upstream** | AccessKit | Yes |
| Native Wayland window | **Failed in WSLg probe** *(verify on a real desktop)* | Yes (GTK3) | Yes (GTK3) | Yes (GTK4) | Yes (needs qt6-wayland) | Yes | Yes | No (X11/XWayland) |
| UI defined as | Declarative `.slint` files, live preview | C++/Rust code; XRC optional | Visual designer | Code, three times over | Code or Designer `.ui` files | Code (Elm-style) | Code (immediate mode) | XAML |
| Licence | GPLv3-only option (the shipped binary becomes GPLv3, which "or later" allows), or Royalty-free with an About-Slint notice | wxWindows licence / MIT binding | Modified LGPL | Ours | GPL-2.0 also offered | MIT | MIT/Apache | MIT |

### Limits that apply to every option

None of these depend on which toolkit you pick, and the app already works around each one:
- **GNOME Wayland:** no app can position the flyout next to the tray or keep the OSD on top. Mutter has no wlr-layer-shell, and the proposed always-on-top protocol was closed unmerged. KDE, Sway, Hyprland and COSMIC do support layer-shell. The current fallback of showing a notification stays.
- **Linux tray icon position:** StatusNotifierItem never reports where the icon is, so a flyout can't be anchored to it on Linux.
- **Wayland global hotkeys:** these need the GlobalShortcuts portal, and the `global-hotkey` crate is still X11-only (its portal PRs #162 and #172 are unmerged). Our existing portal code in [PortalHotkeys.cs](../../src/ThinkVisionControl.Avalonia/Platform/Linux/PortalHotkeys.cs) already registers the app ID first, which xdg-desktop-portal 1.21+ requires; it would be ported, not replaced.
- **GNOME tray host:** stock Fedora Workstation still ships none.
- **KDE PowerDevil** drives DDC itself through libddcutil and can fight with the app. The README already documents the workaround.

---

## Why these rankings

### 1. Rust + Slint
- **Fits the goals:**
  - Size and memory meet the target on Windows, as measured.
  - The Linux tarball is 5.8 MB, though the unpacked binary is 11.3 MB.
  - Starts in well under 150 ms on both measured OSes.
- **Light/dark:** follows Windows dark mode and the accent colour with no theming work.
- **Accessibility:** through AccessKit.
- **Maturity:** stable 1.x API from a funded company, with releases about every two months.
- **Reference app:** **Duja**, a Rust + Slint DDC tray app with this feature set, ships 7.6 MB on Windows, 9.9 MB on Linux, and a 16.3 MB *universal* (Intel + Apple Silicon) macOS DMG.
- **Risks to verify in a spike:**
  1. Native Wayland failed in WSLg ("Failed to open connection to X server"), while egui and iced on the same winit version worked. This may be a missing Cargo feature or a WSLg quirk; it needs a real GNOME/KDE test.
  2. The Linux binary is 11.3 MB, so it needs `opt-level="z"` and a `cargo bloat` pass to get under 10 MB.
  3. macOS arm64 size is unmeasured.
  4. The tray `clicked()` event has no position (and doesn't fire on macOS when a menu is attached), so the Windows flyout anchor needs `tray-icon` or `Shell_NotifyIconGetRect`.
  5. Popups aren't separate OS windows.
- **Licence:** ship under Slint's GPLv3 option, or use the Royalty-free licence and show `AboutSlint`.

### 2. Rust + wxWidgets (wxDragon)
- **The direct answer to "a React Native for desktop".** Real Win32, Cocoa and GTK controls, with 30 years of maturity behind the C++ library.
- **Recent fixes for our needs (wx 3.3.4, 2026-10-10):**
  - tray on Wayland through AppIndicator;
  - complete Windows dark mode;
  - better Wayland support.
- **Size:** wxDragon builds 3.8–7.2 MB on macOS (published figures).
- **Unmeasured:** static-link sizes on Windows and Linux. The dynamic Linux probe needs a 10.5 MB libwx that isn't installed by default, so the static build matters.
- **Risks:**
  - wxDragon is a young binding (0.9.x) with one maintainer.
  - Hotkeys and transparency need raw wx calls.
  - On Linux, GTK3 apps don't always follow GNOME's dark switch.
- **Fallback:** writing it in C++ removes the binding risk, but raises the rewrite cost.

### 3–4. Lazarus, and C# + a separate native UI per OS
- **Lazarus** is small (2.4 MB), fast and native, but:
  - it means switching to Pascal;
  - its GTK3 widgetset is beta and prints GTK-CRITICAL warnings;
  - its Windows build has no dark mode.
- **C# + native UI per OS** keeps the C# core and most of the existing per-OS platform code, which is already raw OS calls. The costs:
  - three separate UIs to maintain;
  - raw Win32 controls look like Windows 7 and have no supported dark mode;
  - macOS would mean hand-written AppKit interop, since the .NET macOS AppKit support is still experimental under NativeAOT.
- **One surprise from this option:** the C# GTK4 bindings (Gir.Core) compiled to a native binary with zero warnings (2.76 MB), even though their tracking issue is still open.

### Below the line
- **Qt Widgets** is excellent technically but 21–26 MB unless built statically with Qt Lite, a custom build nobody has published numbers for.
- **iced** has the smallest numbers measured, but no upstream accessibility, open bugs on Windows and macOS, and no 1.0 release.
- **egui** has an immediate-mode style and a non-native look.
- **Avalonia with NativeAOT** is a real improvement, but stays 3–4× over budget.
- **Eto.Forms** needs whole assemblies kept to work under NativeAOT, which leaves it at 44 MB.

---

## Recommended path

1. **Now, in a 0.0.4 release, with no rewrite:** add source-generated JSON contexts to `ThinkVisionControl.Core`. The patch is in [linux/ava-real-aot/core-json-sourcegen.patch](lightweight-ui-probes/linux/ava-real-aot/core-json-sourcegen.patch), and it touches `ProfileStore`, `SettingsStore` and `BackupStore`. Then publish NativeAOT on Linux and macOS.
   - On Linux this gives 104 → 45 MB and a warm start of 2.1 s → 0.2 s.
   - Also consider software rendering for background mode. The earlier probe measured about 6 MB private memory tray-only.
   - The same change is needed later anyway if any C# remains.
2. **Spike, about 2–3 days:** build the real tray, flyout, OSD and one settings page in Rust + Slint, on all three OSes. Gates:
   - under 10 MB *uncompressed* on each OS;
   - under 2 s cold start;
   - a native Wayland window on GNOME and KDE;
   - light/dark switching on all three;
   - a screen reader reads the sliders.
3. **If the Slint spike fails a gate,** run the same spike with wxDragon.
4. **Migration order once a stack is chosen:**
   1. Port the DDC core and its tests to Rust first: about 3,100 lines, a mostly mechanical codec, queue and settings port. The existing Rust DDC crates are stale (`ddc-hi`'s last release was 2021), so port our own code rather than depend on them. `ddc-macos` is a useful reference for Apple Silicon.
   2. Then the platform layer (about 3,900 lines, already raw OS calls).
   3. Then the UI (about 3,300 lines).
   4. Retire WPF and Avalonia only after the new app matches them on a real P32p-20.

A middle path Gemini raised: the C# core can be compiled as a NativeAOT shared library with a C interface (about 1.6 MB measured as a console app) and called from a Slint or wx UI. That avoids porting the core, but leaves two toolchains and a garbage collector in one process. Porting 3,100 lines is cheaper over time.

---

## Cross-check with Gemini

Gemini researched the same brief independently, and its output was read only after the Claude track was written. Four of its citations were spot-checked:
- The Qt "minimal footprint" blog link returns 404.
- The Mutter issue #658 it cites is about screensavers, not layer-shell.
- The other two links resolve but don't support its claims (see conflicts below).

So Gemini counts here only where it agrees independently or a probe confirms it.

**Agreed by both tracks:**
- Slint first and wxWidgets second.
- Qt Widgets misses 10 MB with dynamic linking.
- GTK4 is too heavy to bundle on Windows and macOS, and has no tray.
- iced is immature and lacks accessibility.
- Tauri fails on memory.
- Dear ImGui has no accessibility.
- No toolkit can do the overlay or flyout placement on GNOME Wayland.
- The Slint licence works through the "or later" clause.

**Conflicts and rulings:**

| Claim | Gemini | Claude track / probes | Ruling |
|---|---|---|---|
| Slint size | 2.5–3.8 MB | 7.05 MB (Windows), 11.3 MB (Linux), measured | **Probes.** Gemini is optimistic by 2–4×. |
| Slint tray | "Slint 1.7+" | 1.17 (2026-06-24), confirmed from the release blog | **Claude.** |
| Gir.Core NativeAOT | "architected for AOT" | Tracking issue #975 still open; probe published cleanly at 2.76 MB | **Works in practice, not officially supported.** |
| wxWidgets hotkeys on X11 | via XGrabKey | `RegisterHotKey` documented as Windows/macOS only | **Claude.** The interface header is primary. |
| Qt on Linux | under 2 MB, "standard on Ubuntu" | Qt 6 is in *universe* and not on the default Ubuntu GNOME install; the probe needed about 21 MB of Qt libraries | **Claude.** |
| Qt licence | LGPL-3/GPL-3 only | qtbase also offered under GPL-2.0-only | **Claude** (the source headers say so). Either way it fits. |
| GNOME OSD with native UI per OS | "solved natively" via ShowOSD | ShowOSD is allow-listed (AccessDenied) | **Claude.** Gemini contradicts itself in its own §4. |
| iced memory | 35–70 MB | 4.7–8 MB with tiny-skia, 134–264 MB with wgpu (measured) | **Depends on the renderer.** Both are wrong as single numbers. |
| Fyne size | 16–28 MB | 7.8 MB stripped (2021 data) | **Unresolved.** Fyne is ruled out either way. |

**Found only by Gemini:**
- *GNOME OSD fallback through notification hints* (`value`, `x-canonical-private-synchronous`). Plausible but unverified: GNOME Shell may ignore these hints. Our current notification fallback stays.
- *C# core as a C-interface NativeAOT library.* Valid technique, covered above.

**Found only by the Claude track:**
- the Duja reference app;
- wxWidgets 3.3.4's Wayland tray;
- `global-hotkey` being X11-only, with its portal PRs unmerged;
- Slint's tray-click limits;
- the xdg-desktop-portal 1.21 app-ID rule, which our code already handles;
- KDE PowerDevil contending for DDC;
- libcosmic's broken Windows build.

## Still open

- **macOS:** every size and startup number is from published sources; none was measured. This needs a Mac, which the spike requires anyway because NativeAOT and Rust can't build for macOS from Windows.
- **Slint:** native Wayland on a real GNOME and KDE session, and how small the Linux binary gets after `cargo bloat`.
- **wxDragon:** Windows and statically linked Linux sizes.
- **Real cold start:** true cold-start numbers on Windows with a flushed cache; the Windows "first run" figures are not cold.
- **Light/dark on Linux:** whether each finalist follows GNOME 47+ and KDE dark-mode switching at runtime.

## Research process notes

- The `research` skill names the model `gemini-3.1-pro-high`, which no longer exists. This run used `gemini-3.8-flash-high`, without `--effort`.
- `--mode plan` auto-denies shell commands. The brief now tells Gemini to use web tools only.
- Installed for the probes:
  - **Windows:** Rust (in `%USERPROFILE%\.cargo` and `.rustup`, not on PATH). Qt, cmake and ninja went into a scratch virtualenv.
  - **WSL Ubuntu:** build tools, GTK, Qt, wx, Lazarus, Rust and the .NET 10 SDK.
- `src/` was not modified.
