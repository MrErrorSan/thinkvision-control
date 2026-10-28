# How the app would look: today vs Slint vs wxWidgets

These are look-alike prototypes of the main window: static data, no monitor control. Each was built twice:
- **Windows 11:** 100% scale, window including the title bar.
- **Linux:** Ubuntu 24.04 under WSL2/WSLg, client area only, because WSLg draws the frame.

The columns:
- **Today:** WPF on Windows (dark), Avalonia on Windows (light), and the shipped Avalonia build on Linux. On Linux, today's app shows its "Monitor controls aren't turned on" state, because WSL has no i2c, so its cards are greyed out.
- **Slint:** Rust + Slint 1.18.1, software renderer, its Fluent style.
- **wx native:** Rust + wxDragon 0.9.24 (wxWidgets 3.3.4, static), using the OS's own controls.
- **wx custom:** the same exe run with `--fluent`. About 600 lines of custom-drawn slider, toggle switch, radio buttons, buttons, cards and nav, all in the system accent. Drop-downs stay native.

Sources and detailed notes are in [lookalike/slint](lightweight-ui-probes/lookalike/slint/NOTES.md) and [lookalike/wx](lightweight-ui-probes/lookalike/wx/NOTES.md). Background research: [2026-10-28-lightweight-ui.md](2026-10-28-lightweight-ui.md).

## Windows

### Display page, dark

| Today (WPF) | Slint | wx native | wx custom |
|---|---|---|---|
| ![](../screenshots/display.png) | ![](lightweight-ui-probes/lookalike/screens/slint-win-display-dark.png) | ![](lightweight-ui-probes/lookalike/screens/wx-win-display-dark.png) | ![](lightweight-ui-probes/lookalike/screens/wx-fluent-win-display-dark.png) |

### Display page, light

| Today (Avalonia) | Slint | wx native | wx custom |
|---|---|---|---|
| ![](../previews/avalonia/display-light-connected.png) | ![](lightweight-ui-probes/lookalike/screens/slint-win-display-light.png) | ![](lightweight-ui-probes/lookalike/screens/wx-win-display-light.png) | ![](lightweight-ui-probes/lookalike/screens/wx-fluent-win-display-light.png) |

### Monitor settings, dark

| Today (WPF) | Slint | wx native | wx custom |
|---|---|---|---|
| ![](../screenshots/monitor-settings.png) | ![](lightweight-ui-probes/lookalike/screens/slint-win-monitor-dark.png) | ![](lightweight-ui-probes/lookalike/screens/wx-win-monitor-dark.png) | ![](lightweight-ui-probes/lookalike/screens/wx-fluent-win-monitor-dark.png) |

### Monitor settings, light

| Today | Slint | wx native | wx custom |
|---|---|---|---|
| *(no capture)* | ![](lightweight-ui-probes/lookalike/screens/slint-win-monitor-light.png) | ![](lightweight-ui-probes/lookalike/screens/wx-win-monitor-light.png) | ![](lightweight-ui-probes/lookalike/screens/wx-fluent-win-monitor-light.png) |

### Slint with the exact system accent (`--exact-accent`)

| Dark | Light |
|---|---|
| ![](lightweight-ui-probes/lookalike/screens/slint-win-display-dark-exactaccent.png) | ![](lightweight-ui-probes/lookalike/screens/slint-win-display-light-exactaccent.png) |

## Linux (WSLg; GTK theme Adwaita)

### Display page, dark

| Today (Avalonia) | Slint | wx native (GTK3) | wx custom |
|---|---|---|---|
| ![](lightweight-ui-probes/lookalike/screens/today-linux-display-dark.png) | ![](lightweight-ui-probes/lookalike/screens/slint-linux-display-dark.png) | ![](lightweight-ui-probes/lookalike/screens/wx-linux-display-dark.png) | ![](lightweight-ui-probes/lookalike/screens/wx-fluent-linux-display-dark.png) |

### Display page, light

| Today (Avalonia) | Slint | wx native (GTK3) | wx custom |
|---|---|---|---|
| ![](lightweight-ui-probes/lookalike/screens/today-linux-display-light.png) | ![](lightweight-ui-probes/lookalike/screens/slint-linux-display-light.png) | ![](lightweight-ui-probes/lookalike/screens/wx-linux-display-light.png) | ![](lightweight-ui-probes/lookalike/screens/wx-fluent-linux-display-light.png) |

### Monitor settings, dark

| Today (Avalonia) | Slint | wx native (GTK3) | wx custom |
|---|---|---|---|
| ![](lightweight-ui-probes/lookalike/screens/today-linux-monitor-dark.png) | ![](lightweight-ui-probes/lookalike/screens/slint-linux-monitor-dark.png) | ![](lightweight-ui-probes/lookalike/screens/wx-linux-monitor-dark.png) | ![](lightweight-ui-probes/lookalike/screens/wx-fluent-linux-monitor-dark.png) |

### Monitor settings, light

| Today (Avalonia) | Slint | wx native (GTK3) | wx custom |
|---|---|---|---|
| ![](lightweight-ui-probes/lookalike/screens/today-linux-monitor-light.png) | ![](lightweight-ui-probes/lookalike/screens/slint-linux-monitor-light.png) | ![](lightweight-ui-probes/lookalike/screens/wx-linux-monitor-light.png) | ![](lightweight-ui-probes/lookalike/screens/wx-fluent-linux-monitor-light.png) |

## Numbers (look-alike apps, measured)

| | Today | Slint | wxWidgets (native or custom, same exe) |
|---|---|---|---|
| **Windows shipped** | WPF 0.98 MB + 60 MB .NET runtime download | 10.4 MB exe (5.5 MB zipped) | **6.5 MB**: 3.97 MB exe + 2.84 MB icon font (2.7 MB zipped) |
| **Linux shipped** | 104 MB (43 MB tar.gz) | 14.1 MB (6.8 MB tar.gz) | **6.6 MB** (3.5 MB tar.gz with the font) |
| **Windows warm start** | 0.9–1.3 s | **0.10–0.13 s** | 0.50–0.59 s custom, 0.56–0.71 s native (an empty window takes 50 ms, so this is fixable but unprofiled) |
| **Linux warm start** | 2.1 s | **0.07–0.09 s** | not measured |
| **Windows idle memory** (private) | 75 MB | 6.4 MB | **3.8 MB** |
| **Linux idle memory** (PSS) | 224 MB | 14.3 MB | not measured |
| **Extra Linux libraries** | none (bundled) | none | none beyond GTK3 |

Neither prototype has been trimmed for size yet. The Slint numbers include accessibility support and the embedded icon font; wxWidgets ships the font as a separate file.

## System accent colour

| | Slint | wx native controls | wx custom controls |
|---|---|---|---|
| **Windows** | Follows the accent, but softens it (it keeps the hue, not the exact shade). `Palette` is read-only, so the exact shade only works on controls the app draws itself (`--exact-accent`). | **No.** Win32 trackbars, radio buttons and check boxes stay system blue whatever the accent; Windows gives no way to recolour them. | **Yes, the exact shade.** Read from `AccentPalette`, using the lighter shade in dark mode and the darker in light mode, as WinUI does. Live re-read on accent change is wired but untested. |
| **macOS** | Follows (softened) | **Yes:** native Cocoa controls follow the accent automatically | Yes, if the app reads `controlAccentColor` (not built) |
| **Linux** | Follows the XDG portal accent (GNOME 47+, KDE), softened | **No:** GTK3/Adwaita blue is fixed; only GTK4/libadwaita apps follow the accent | Yes, from the portal accent. WSLg has none, so these shots show GTK's blue. |

## What each one gives up

**Slint**
- **Looks:** layout, icons and cards match within 1–2 px on both OSes, and the Linux version is nearly identical to today's Linux app.
- **What's lost:**
  - The Mica wallpaper tint. Getting it back needs the GPU renderer, at about +90 MB of memory.
  - ClearType text (it uses greyscale anti-aliasing).
  - The exact accent shade.
  - Combo boxes can't be narrower than 160 px.
  - Combo pop-ups open inside the window.
  - Page animations.
- **Size:** 10.4 MB on Windows and 14.1 MB on Linux, so it needs a trimming pass to meet the 10 MB target.
- **Wayland:** Slint forces X11 under WSL; native Wayland still needs a test on a real desktop.

**wxWidgets, native controls**
- On Windows these are classic Win32 controls: blue trackbar sliders, check boxes instead of switches, system blue instead of the accent.
- On Linux they are plain GTK3/Adwaita: dark slider knobs and the fixed blue.
- Dark mode is complete on both, but it doesn't look like a Windows 11 app.

**wxWidgets with custom controls**
- **Looks:** close to today's look on Windows, in the exact system accent, with native ClearType text and native drop-downs.
- **Cost:** the custom controls took about 1.5 hours and 600 lines.
- **Still to do** (about 3–4 days in total):
  - screen-reader support for the custom controls, which are currently invisible to them (about 1 day);
  - DPI scaling (1–2 h);
  - animations (about ½ day);
  - a custom drop-down, if a Fluent drop-down is wanted (about 1 day);
  - the expander, which opens below its card rather than inside it.
- **Startup:** about 0.5 s for the full window. That's well under the 2 s target, but 5× slower than Slint until profiled.
- **Binding risk:** wxDragon is young, with one maintainer. Linux needed a workaround for its icon-font loading, and it doesn't expose `wxGraphicsContext`.

## Updated ranking

These prototypes replace the estimates in the earlier report (it scored Slint 79.0 and wxWidgets 76.8):
- **wxWidgets:** size is now measured at 6.5–6.6 MB, and dark mode with the accent works through the custom controls.
- **Slint:** the Linux build came out at 14.1 MB.

| | Size | Start | Mem | Stable | Dark/accent | Native | Tray/keys/OSD | A11y | Rewrite | Licence | **Score** |
|---|---|---|---|---|---|---|---|---|---|---|---|
| **wxWidgets + custom controls** | 5 | 4 | 5 | 3 | 4 | 4 | 4 | 4 | 2 | 5 | **80.0** |
| **Slint** | 3 | 5 | 5 | 4 | 4 | 3 | 4 | 4 | 2 | 4 | **75.0** |

Weights as before: size 20, start 10, memory 10, stability 15, dark/accent 8, native 5, tray/keys/OSD 10, a11y 7, rewrite 10, licence 5.

## Verdict

- **Your priority order (size, then speed and stability, then looks) favours wxWidgets with custom controls.**
  - It's about 6.5 MB on both OSes and has the lowest memory of anything measured.
  - It uses the exact system accent, with ClearType text and native drop-downs.
  - It inherits the real native look on macOS for free.
- **Slint remains the lower-risk build.**
  - A funded maintainer behind it.
  - Screen-reader support for every control out of the box.
  - Declarative UI files.
  - Fastest start (0.1 s).
  - But it has to be trimmed by about 30% on Linux to reach 10 MB, and it can't use the exact accent on its built-in controls.

**Before committing to wxWidgets**, check its three weak spots in the spike:
1. Profile and fix the 0.5 s startup, e.g. by building the Monitor page only when first opened.
2. Add screen-reader support to the custom controls.
3. Build and run it on a Mac.

If any of those turns out hard, Slint is the fallback, and nothing in the core or platform-layer port changes.
