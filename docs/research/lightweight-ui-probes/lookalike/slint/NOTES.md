# Slint look-alike of ThinkVision Control

A static visual copy of the main window, built with Slint 1.18.1. It uses the `fluent` style and the software renderer, with `accessibility` enabled. It has no monitor control. The point is to show which looks we lose by moving from Avalonia/WPF Fluent to Slint.

- **Screenshots** are in [`../screens/`](../screens/), named `slint-win-{display,monitor}-{dark,light}.png`:
  - Windows 11, 100 % scale (96 DPI).
  - Each is a 1010×752 capture of the window frame, title bar included.
  - They were taken with `PrintWindow`, so other windows can't get into the shot.
- **References:**
  - `docs/previews/avalonia/display-dark-connected.png`, `display-light-connected.png` and `monitor-dark.png`: the current app. Layout was measured from these.
  - `docs/screenshots/display.png` and `monitor-settings.png`: the WPF version.
- **Linux:** screenshots of both the look-alike and today's shipped app are covered in [Linux](#linux-wsl2-ubuntu-2404-wslg) below.
- **Polish:** fidelity work stopped after the first working pass.

## Build and run

```
set TVC_REPO=<repo root>        # only needed when building a copy outside this folder
cargo build --release
lookalike-slint.exe [--dark | --light] [--monitor] [--exact-accent]
```

- **Linux:** the same sources build on Linux. `Cargo.toml` adds `backend-winit-x11` and `backend-winit-wayland` for Linux only.
- **Capture helpers:** in `tools/`.
  - `capture.ps1`: Windows, `PrintWindow`.
  - `shot.sh` with `stub_portal.py`: WSLg, a private D-Bus session with a stub XDG Settings portal, then ImageMagick `import`.

- **Icon font:** not copied here. `build.rs` adds `src/ThinkVisionControl.Avalonia/Assets/Fonts/` as a Slint include path, so `import "FluentSystemIcons-Regular.ttf"` resolves to the repo's copy, which is embedded at build time.
- **Window icon:** `assets/brand/png/app-icon-32.png`, found the same way. The `.exe` has no embedded file icon.
- **`--dark` / `--light`:** sets `Palette.color-scheme`. It also tells winit the theme through `BackendSelector::with_winit_window_attributes_hook`, which needs the `unstable-winit-030` feature. Without that, a forced-light window on a dark desktop keeps a dark title bar.

## What matched

- **Layout geometry:** within about 1–2 px of the Avalonia previews.
  - 320 px navigation pane with 36 px items.
  - 36 px page padding and a 28 px semibold title.
  - Section headers 30 px above and 6 px below.
  - Cards 4 px apart, with the same per-card heights (69, 75, 78, 80 and 84 px).
  - Icon, title and control columns at the same x positions.
- **Content layer:** a lighter content area with a 1 px outline and an 8 px top-left corner, like FANavigationView.
- **Icons:** the same Fluent System Icons glyphs and codepoints, sized to the same visible size.
- **Type:** Segoe UI Variable Text at 28 / 20 / 14 / 12 px, the same ramp as the app.
- **Navigation:** selected and hover fills, and a 3×16 accent pill that animates its height. Clicking switches pages, and the hamburger collapses the pane to 48 px with an animation.
- **Controls:**
  - Slint's fluent Slider, ComboBox, Switch, Button, RadioGroup and ScrollView all read as Windows 11 controls.
  - The ComboBox popup and the overlay scrollbar are Fluent-like.
  - Focus rings exist.
- **Accent:** taken from the system automatically. Slint's fluent palette re-hues its default blue with the Windows DWM accent; on this machine that is #D3452C.
- **Theme:** dark and light both work, including a matching native title bar.

## What could not be matched, and why

| Difference | Why | Effort to close |
|---|---|---|
| **Mica backdrop.** The real app's wallpaper-tinted reddish surfaces become flat neutral greys (#202020 / #f3f3f3). | The software renderer paints an opaque buffer through softbuffer, so a DWM Mica backdrop can't show through. Neutral Mica fallback colours are used instead. | High. Would need a GPU renderer with a transparent surface, plus `DwmSetWindowAttribute(DWMWA_SYSTEMBACKDROP_TYPE)` through winit. That costs the software renderer's memory advantage (about 90 MB with femtovg). |
| **Title-bar integration.** The real app extends its content into the title bar (32 px custom bar, pane reaching the top). The copy has the standard native caption, and the icon looks slightly different. | winit's decorated window. A custom title bar needs `no-frame` plus your own caption buttons, drag regions and snap-layout support (`WM_NCHITTEST`). | Medium to high. Snap layouts on the maximise button are the hard part. |
| **Accent tone.** Slint draws #E6B0A5 (pale salmon) in dark mode and #825349 (brown) in light mode. WinUI would draw #ED9675 and #B43622 for this machine's accent (#D3452C). The reference screenshots' coral (#f46762) and red (#d20e1e) came from a different accent setting, so compare against the WinUI values, not those pixels. | Two causes. (1) On Windows, Slint reads `DwmGetColorizationColor`: a balance-blended, less saturated colour, not `AccentColor`. (2) `accentify` keeps the lightness of Slint's default blue and takes only hue and chroma from the OS colour. `Palette.accent-background` is an `out` property, so it can't be overridden. | Low for app-drawn elements, high for std widgets. See [Exact accent](#exact-accent-windows). |
| **ComboBox width.** Every combo is at least 160 px wide; the reference's "Normal" combo is 120 px. | `min-width: max(160px, …)` is fixed inside the fluent ComboBox. | Low if a custom combo is acceptable; otherwise needs a Slint patch. |
| **Radio spacing.** The radio buttons sit 8 px apart instead of 20 px, and HDMI can't show the "This PC" caption on a second line. | RadioGroup's internal GridLayout spacing isn't exposed, and RadioButton takes only text. | Low: hand-roll a radio row of about 40 lines. |
| **Expander.** Custom colour is hand-built: a header with a chevron that shows three slider rows. It has no expand/collapse animation, and its chevron is a glyph swap rather than a rotation. | Slint has no Expander widget. | Low to medium: animate height and rotate the chevron. |
| **Toggle switch.** Close but not identical. Slint's fluent Switch thumb and track proportions differ slightly, and the "On" track uses Slint's pale accent. | Style implementation detail. | Low to medium: restyle with a custom component. |
| **Font rendering.** Text is greyscale-antialiased and slightly softer and lighter than DirectWrite ClearType. Glyph spacing differs a little. | The software renderer rasterises with swash/skrifa, not DirectWrite, so there is no subpixel ClearType. | Not fixable while using the software renderer. |
| **Combo popup, flyouts and menus.** The popup is drawn inside the window, with no acrylic, no drop shadow outside the window bounds and no WinUI open animation. | Slint popups are in-window, or separate windows without a system backdrop. | High for acrylic; not worth it. |
| **Scrollbar.** Slint's fluent overlay bar is close (a thin line that widens on hover), but its rest width and fade timing differ from WinUI's. | Style detail. | Low. |
| **Animations.** WinUI's connected page-transition slide and its control-state easing curves are missing. The copy only animates nav fills, the pill and the pane width. | Not built-in. | Medium: page-entrance animation per page. |
| **Focus visuals.** Slint's fluent focus ring is similar, but not WinUI's double-stroke reveal rectangle on every element. | Style detail. | Low. |
| **Startup scroll glitch (fixed with a workaround).** Pages first rendered scrolled down by 8 to 100 px. | Wrapped subtitles settle their height after the first layout pass, which leaves the Flickable's content-y off. A 50 ms one-shot `Timer` resets `content-y` to 0. | Workaround in place. Needs a proper fix or a Slint bug report. |
| **Glyph choice differences.** Contrast is drawn half-filled horizontally, and Over Drive and Dynamic Contrast use the speedometer and dark-theme glyphs. | These are the codepoints the Avalonia code uses; the WPF screenshots used older glyphs. | None needed; this is a reference mismatch, not a Slint limit. |

## Measurements (Windows 11, Ryzen 5 5500U, 100 % scale, release build)

Same method as `windows/results.json`:
- **Startup:** stopwatch from `Start-Process` until a visible top-level window titled "ThinkVision Control", at least 100 px wide, exists. Polled every 10 ms.
- **Memory:** read after 10 s idle.

| Metric | Look-alike | Earlier minimal Slint software probe |
|---|---|---|
| Shipped size (single exe) | **10.39 MB** (10,896,896 B) | 7.05 MB |
| Zipped (Compress-Archive, Optimal) | **5.50 MB** | 4.10 MB |
| First run (not cold) | 205 ms | – |
| Warm start, 3 runs | **104, 103, 128 ms** | 64–87 ms |
| Private memory after 10 s idle | **6.4 MB** | 3.8 MB |
| Working set after 10 s idle | **24.4 MB** | 20.0 MB |
| Threads | 10 | – |

- **Why it is bigger:** about 3.3 MB more than the probe. The likely causes are AccessKit (the `accessibility` feature), the embedded icon font and window icon, and the extra widget code. This was not broken down further.
- **Later build:** the `--exact-accent` build is 10.43 MB (10,937,856 B). The timings above were taken on the earlier 10.39 MB build.

## Exact accent (Windows)

**What I tried:** overriding `Palette.accent-background`. This isn't possible in Slint 1.18:
- `Palette` exposes the accent as an `out` property.
- The internal `FluentPalette` that the std widgets read isn't exported.
- The only input is the OS colour, and the backend fetches that itself (`SlintInternal.accent-color`).

**What `--exact-accent` does instead:**
- `main.rs` reads `HKCU\Software\Microsoft\Windows\CurrentVersion\Explorer\Accent\AccentPalette` through plain advapi32.
- It takes Light2 for dark theme and Dark1 for light theme, the shades WinUI uses for `AccentFillColorDefault`.
- It passes them to an `AccentOverride` global.
- The nav pill, a hand-drawn `AccentSlider` and a hand-drawn `AccentRadio` use that colour. The radio also gets the 20 px spacing the std RadioGroup can't do.
- About 90 lines of `.slint`.

**Screenshots:** `slint-win-display-dark-exactaccent.png` and `slint-win-display-light-exactaccent.png`.

| Accent pixel | Slint default | `--exact-accent` | WinUI target |
|---|---|---|---|
| Dark (slider track, nav pill) | #E6B0A5 | #ED9675 | #ED9675 (Light2) |
| Light | #825349 | #B43622 | #B43622 (Dark1) |

**Cost:**
- Every accented std widget has to be re-implemented: Slider, RadioButton, Switch, CheckBox, primary Button and ComboBox focus.
- Each is roughly 30–60 lines.
- Each loses the std widget's built-in keyboard handling and accessibility unless they are added back by hand.
- The Monitor page's Switch still uses Slint's own accent in this build.

**Alternative:** pass the exact accent to Slint through a small upstream patch that lets apps set `accent-color` and opt out of `accentify`.

## Linux (WSL2 Ubuntu 24.04, WSLg)

**Build:**
- Same sources, built in `~/lookalike-slint` with `backend-winit-x11` and `backend-winit-wayland`.
- WSL DNS dropped out during the build, so the crates were vendored on Windows (`cargo vendor`) and built `--offline`.
- Fonts: Segoe UI isn't installed, so text falls back to DejaVu Sans, which is wider and rounder. The icon font is embedded and renders identically.

**Wayland or X11?**
- **Slint picks X11 on purpose under WSL.** When both backends are compiled in, i-slint-backend-winit 1.18.1 checks `/proc/sys/fs/binfmt_misc/WSLInterop` or `/run/WSL` and calls `with_x11()`. Its comment: "Under WSL, the compositor sometimes crashes ... This does not happen when using XWayland." So even with `WAYLAND_DISPLAY=wayland-0` set, the window was an XWayland window. `xdotool` found it.
- **A Wayland-only build** (no `backend-winit-x11`, no `unstable-winit-030`) does open a native Wayland toplevel: `xdg_toplevel.set_title`, `wl_shm` buffers attached.
  - **First run:** the connection broke ("Io error: Broken pipe", exit 1), and WSLg's Wayland socket was re-created at that moment, meaning the compositor restarted.
  - **Second run:** stayed up for 6 s with no error.
  - **Verdict:** native Wayland works but is flaky under WSLg, as Slint's comment warns.
- **Screenshots and measurements** below are all on X11/XWayland.
- **The earlier "Failed to open connection to X server" error:** I couldn't reproduce it with `DISPLAY` set. Without `DISPLAY`, the Wayland-only build fails with "Could not find wayland compositor" if it runs while the compositor is restarting.

**Default style:**
- Slint's default style on Linux is **fluent**. `native` (qt) is used only when the build includes the Qt backend and finds Qt.
- This build pins `fluent` in `build.rs` anyway. No other default applies, so there is no separate default-style screenshot.

**Theme forcing:**
- **Look-alike:** `--dark` / `--light`.
- **Both apps** were also run on a private `dbus-run-session` bus with `tools/stub_portal.py`. This is a stub `org.freedesktop.portal.Settings` that answers `org.freedesktop.appearance` with `color-scheme` and `accent-color`. It doesn't touch the real WSL desktop settings.
- **Why a stub:** WSLg's real portal (xdg-desktop-portal with the gtk backend) has **no Settings interface**. Without the stub, both apps see "no preference" and no accent.

**Does Slint follow the XDG portal accent? Yes**, through `org.freedesktop.appearance` / `accent-color`. It is still toned through `accentify`. Same stub, `--dark` / `--light`:

| Portal accent | Dark slider | Light slider |
|---|---|---|
| #D3452C (the Windows accent) | #FF8C70 | #B12003 |
| none | #60CDFF (Slint blue) | #005FB8 |

On Linux the portal colour goes into `accentify` unblended. That's why the same base accent comes out much more saturated than on Windows (#E6B0A5 / #825349).

**Screenshots** (client area only; WSLg draws the frame on the Windows side, so `import` can't capture it; 1008×720):
- `slint-linux-{display,monitor}-{dark,light}.png`: the look-alike, stub accent #D3452C.
- `today-linux-{display,monitor}-{dark,light}.png`: the shipped `build/linux-x64/thinkvision-control` (Avalonia + FluentAvalonia, NativeAOT, 104 MB), run from a copy in `~/tvc-today`.
  - It shows its "Monitor controls aren't turned on" state (no i2c-dev in WSL), so its cards are disabled and greyed. The look-alike shows connected, enabled cards, so compare layout and chrome, not enabled-state colours.
  - **Theme:** the app has **no theme flag or env var**. `App.axaml` sets `FluentAvaloniaTheme PreferSystemTheme="True" PreferUserAccentColor="True"`, and `Platform/Linux/LinuxThemeInfo.cs` reads the portal's `color-scheme` and `accent-color`. The stub portal is what forced its dark and light. It picked up the accent too (coral nav pill).
  - The Monitor page was reached by clicking the nav item with `xdotool`.

**Side by side on Linux:**
- Layout, spacing, icons and type ramp are near-identical; both fall back to DejaVu Sans.
- Today's app has a slightly darker card fill and a 1 px card outline.
- Its status line reads "Not connected" with a warning banner, because of the WSL state.

**Measurements** (look-alike, same `linux/measure/measure.sh` as the earlier probes: X11, cold after `drop_caches`, 3 warm runs, PSS after 10 s idle):

| Metric | Look-alike | Earlier Slint software probe |
|---|---|---|
| Shipped size (single binary, stripped) | **14.06 MB** (14,739,136 B) | 11.30 MB |
| tar.gz | **6.80 MB** | 5.77 MB |
| Must bundle | nothing | nothing |
| Cold start | 576 ms (run 1) / 198 ms (run 2) | 128 ms |
| Warm start | run 1: 162, 430, 230 ms; run 2: **71, 72, 94 ms** | 45 ms |
| PSS after 10 s idle | 15.6 MiB (run 1) / **14.3 MiB** (run 2) | 11.3 MiB |
| RSS / threads | 19.6 / 18.4 MiB, 6 threads | – |

- **The first run was noisy.** It overlapped with the wxWidgets agent's WSLg captures and a compositor restart. Run 2 is the representative one.
- **Today's app** wasn't re-measured here. The research doc's numbers still apply: 104 MB, about 2.1 s warm start before the AOT JSON fix.
