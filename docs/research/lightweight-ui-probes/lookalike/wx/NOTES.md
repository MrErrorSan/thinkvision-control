# wxWidgets look-alike (Rust + wxDragon): notes

A static copy of ThinkVision Control's main window, built with native wxWidgets controls. It has no monitor control. The aim is to see how close native Win32 controls get to the current Fluent/Avalonia design, and what they look like when you don't fight them.

There are two variants in one binary:
- **Native** (default): stock wx controls.
- **`--fluent`**: custom-drawn, anti-aliased Fluent controls that follow the system accent colour. See [Fluent variant](#fluent-variant---fluent) below.

**Screens**, all in `../screens/`:
- **Windows 11** (100 % scale, 96 DPI, 1008×720 visible window including the title bar):
  - native: `wx-win-{display,monitor}-{dark,light}.png`
  - fluent: `wx-fluent-win-{display,monitor}-{dark,light}.png`
- **Linux** (WSLg, Ubuntu 24.04, GTK 3.24, X11 backend, `GTK_THEME=Adwaita[:dark]`, 1022×727 client area). WSLg draws its own decorations, so these captures have no title bar.
  - native: `wx-linux-{display,monitor}-{dark,light}.png`
  - fluent: `wx-fluent-linux-{display,monitor}-{dark,light}.png`

## Binding

- **Rust + wxDragon 0.9.24**, the stack under evaluation. Built with `default-features = false`. The crate builds wxWidgets **3.3.4** from source with CMake and Ninja, and links it statically into a single exe.
- **First build on Windows:** about 5½ minutes. It needed two fixes:
  1. **libclang for bindgen.** VS Build Tools ships no `libclang.dll`. `pip install libclang` provides one; point `LIBCLANG_PATH` at its `clang/native` folder.
  2. **A common-controls v6 manifest.** Without it, wx 3.3 shows a blocking "doesn't use a correct manifest" warning dialog at start. `build.rs` embeds the manifest (plus PerMonitorV2 DPI awareness) and the app icon as `wxICON_AAA` via `winresource`.

  After those two fixes there were no C++ fallbacks, and incremental rebuilds take about 10 s.
- **Theme:** `--dark` and `--light` call `wxdragon::app::set_appearance(Appearance::Dark/Light)`, which wraps `wxApp::SetAppearance` (wx 3.3). The call happens in `OnInit` before any window exists. With no flag, the app follows the system theme. The Windows theme itself is never touched. `--monitor` opens on the second page.
- **Build profile:** `opt-level="z"`, `lto`, `codegen-units=1`, `strip`, `panic="abort"`.
- **Sources:**
  - `Cargo.toml` and `build.rs`.
  - `src/main.rs`: 566 lines. That is about 420 for the native look-alike, plus about 145 to wire in `--fluent` and the Linux font workaround.
  - `src/fluent.rs`: 576 lines of custom controls and accent reading.
  - `examples/plain.rs`: an empty-frame baseline.
  - `results/*.json`: raw measurement runs.

## What matched

- **Layout:**
  - 320 px nav pane on the left.
  - "Display" and "Monitor settings" with Fluent icons; the selected item has a tinted background and a red accent bar.
  - "App settings" at the bottom, below a separator.
  - `wxSimplebook` switches pages; each page is a `wxScrolledWindow`.
- **Typography:** Segoe UI at sizes close to the reference (page title, device name, section headers, card title and subtitle). Wrapped two-line subtitles work (Aspect ratio).
- **Icons:**
  - The Fluent System Icons TTF loads with `wxFont::AddPrivateFont`. Glyphs then draw in plain `wxStaticText` at any size and colour. This worked first time.
  - The TTF ships next to the exe (2.8 MB). It could be embedded and written to a temp file, or replaced with a subset of about 20 glyphs, which would be a few KB.
- **Cards:** a `wxPanel` per card, with a `wxPaintDC` rounded rectangle (5 px). Card fills match the reference closely in both themes: #2B2B2B on #202020, and white on a pink tint.
- **Dark mode is complete for every control used:**
  - title bar;
  - `wxChoice` (dark field and dark drop-down);
  - `wxCheckBox` and `wxRadioButton`;
  - `wxButton`;
  - `wxStaticLine`;
  - the scrollbar (thin dark Explorer-style scrollbar).

  None of these stayed light in dark mode. The only control that does not look "dark" is the slider (see below).
- **Light mode:** the title bar, controls and scrollbar are all light. The title bar is plain white rather than Mica-tinted.
- **Interactivity:**
  - Nav clicks switch pages.
  - Slider moves update the value label.
  - The chevron reveals the R/G/B sliders.
  - The check boxes flip the Off/On label.

## What could not be matched (and the effort to get closer)

| Area | wx result | Why | Effort to fix |
|---|---|---|---|
| **Slider** | Classic comctl32 trackbar: a thin grey track and a **blue** pointed thumb in both themes. There is no filled track and no accent colour. | `wxSlider` is the Win32 trackbar. Its colours come from the visual-style theme, and it has no accent or fill API. | A custom-drawn slider (a `wxControl` drawing a filled track and a round thumb, with mouse and keyboard handling and accessibility): **1–2 days**. |
| **Toggle switch** | `wxCheckBox` plus an "Off"/"On" label. | wxWidgets 3.3.4 has **no switch control** (no `wxSwitchCtrl` header in the source tree). | A custom-painted switch: **½–1 day**, including focus and accessibility. |
| **Accent colour** | The red ember accent appears only where we paint it (the nav bar). The radio dot, check mark, slider thumb and focus are system blue. | Native controls ignore app colours. | Only by custom-drawing each control. |
| **Combo box** | Native `wxChoice`: square, 1 px border, small chevron. Close in spirit, but it lacks the rounded Fluent field and the hover fill. | Native control. | Owner-drawn combo (`wxOwnerDrawnComboBox`): **½ day**, but it then stops being native. |
| **Radio buttons** | Small classic radio circles with a blue dot. The reference has large ringed Fluent radios with a red fill. | Native control. | Custom-drawn: **½ day**. |
| **Rounded cards** | Corners are GDI `RoundRect`, **not anti-aliased** (slightly jagged at 100 %). There is no 1 px card stroke and no shadow. | `wxPaintDC` is GDI. | `wxGraphicsContext` (Direct2D or GDI+) gives anti-aliased corners. wxDragon does not expose it on panels in an obvious way, so check that first: **2–4 h**. |
| **Mica / backdrop** | Flat solid colours. The light theme fakes the reference's pink Mica tint with a fixed #FBF1F3. | wx has no Mica API. | `DwmSetWindowAttribute(DWMWA_SYSTEMBACKDROP_TYPE)` would need FFI plus transparent backgrounds on every panel, which fights wx's opaque native controls: **1 day+, fragile**. |
| **Title bar** | Standard Windows title bar, dark or light, with the app icon. The reference extends content into the title bar. | Native frame. | A custom client-side title bar: **1–2 days**, and loses snap layouts unless reimplemented. |
| **Nav pane** | Hand-built from panels. There is no hover highlight, no rounded selection pill and no open/close animation; the hamburger does nothing. | wx has no NavigationView. | Hover: **1 h** (`on_mouse_enter` and `on_mouse_leave`). Rounded pill: **1 h**. Collapsible pane with animation: **1 day**. |
| **Expander** ("Custom colour") | Custom chevron. The revealed body is a separate square panel *below* the card, not part of one rounded card. There is no animation. | `wxCollapsiblePane` draws a "▸ label" button that looks nothing like a card. | Merge the body into the card and repaint the corners: **2–3 h**. |
| **Focus visuals** | Classic dotted focus rectangles (shown on keyboard use). The reference has a 2 px rounded focus ring. | Native. | Not fixable without custom drawing. |
| **Scrollbar** | A thin native dark or light scrollbar that is always visible. The reference's overlay scrollbar expands on hover. | Native. | Not practical to change. |
| **Input icon** | Glyph `U+E99A` renders as a plug, while the reference shows a keyboard and mouse. | `U+E99A` is `ic_fluent_plug_connected_24_regular`, which is what `Glyphs.Input` uses in today's code; the reference screenshot shows `keyboard_mouse`. | Left as is in the native variant. The fluent variant uses `U+F0D1E` (`ic_fluent_keyboard_mouse_20_regular`). |

**Overall:** pushed toward Fluent, wx gets the structure, typography, icons, card fills and complete dark mode cheaply, in under 400 lines. What is left is the look of the native controls themselves: a blue classic trackbar, a check box where a switch should be, classic radios, and no accent colour. In practice it looks like a tidy Win32 app with cards, not WinUI.

## Measurements (Windows 11, Ryzen 5 5500U, system in dark mode)

| Build | Shipped | Zipped | Warm start (ms, 3 runs) | Private MB | Working set MB | Threads |
|---|---|---|---|---|---|---|
| Look-alike exe + icon TTF | **6.46 MB** (exe 3.94 MB + TTF 2.84 MB) | **2.70 MB** | **696 / 597 / 710** (first run 670) | **3.8** | **20.2** | 4 |
| Look-alike exe only | 3.94 MB | 1.66 MB | – | – | – | – |
| Look-alike, `--light` | – | – | 417 / 480 (first run 497) | 3.8 | 20.1 | 4 |
| Empty `wxFrame`, same toolchain (`examples/plain.rs`) | 3.84 MB | 1.54 MB | **49 / 50** (first run 301) | 3.1 | 18.2 | 4 |

**Method:** the same as `../../windows/results.json` (`measure.ps1`). Start time runs from `Start-Process` until a visible "ThinkVision Control" window exists, polled every 10 ms. Memory is the mean of samples taken after 10 s idle.

**Startup is the surprise.** An empty wx frame is up in about 50 ms. The full look-alike takes about 420–710 ms: roughly 450 ms in light and 550–850 ms in dark, before the window is shown.
- **Not the icon font:** without loading the TTF it still took 520–580 ms.
- **Not profiled.** The likely suspects are:
  - creating about 150 native child HWNDs, all eagerly, across both pages;
  - per-control dark-mode theming;
  - creating about 60 `wxFont` objects by face name.
- **Cheap fixes:**
  - build the Monitor page lazily;
  - cache the fonts;
  - show the frame before filling the pages.

  These should cut it substantially, but that was not verified.

Memory is excellent: under 4 MB private and 20 MB working set.

**Second round** (same machine; this binary includes `fluent.rs`, so it is 3.97 MB). I measured with the CPU idle, after a parallel WSL build had finished; an earlier run during that build gave 0.8–1.8 s and was discarded.

| Build | Shipped (exe + TTF) | Zipped | Warm start (ms, 3 runs) | Private MB | Working set MB |
|---|---|---|---|---|---|
| Native (default) | 6.49 MB | 2.72 MB | 556 / 600 / 712 (first run 553) | 3.6 | 17.7 |
| **`--fluent`** | 6.49 MB | 2.72 MB | **587 / 495 / 539** (first run 866) | **3.8** | **18.8** |
| Empty frame | 3.66 MB | 1.54 MB | 80 / 52 (first run 252) | 3.1 | 17.1 |

The custom controls cost about 35 KB of exe and nothing measurable in start time or memory. Startup is still dominated by whatever makes the populated window take about 0.5 s (see above).

## Fluent variant (`--fluent`)

**Built in about 1.5 hours**, including the wxDragon API lookups and three fix-and-rebuild cycles. It is 576 lines in `src/fluent.rs` plus about 120 lines of wiring in `main.rs`:

| Part | Lines |
|---|---|
| Accent reading (Windows registry, XDG portal, GTK fallback) | ~95 |
| Theme palette | ~50 |
| Anti-aliased canvas | ~150 |
| Slider | ~90 |
| Toggle switch | ~45 |
| Radio group | ~65 |
| Button | ~45 |
| Card and nav painters | ~25 |

**How it draws.** wxDragon 0.9.24 does not expose `wxGraphicsContext` at all; it only offers the GDI-style `wxDC` with no anti-aliasing. So each control is a plain `wxPanel` with `BackgroundStyle::Paint`. Its paint handler rasterises rounded rectangles and circles into an RGBA buffer using signed-distance coverage, which gives about 1 px anti-aliasing. The buffer becomes a `wxBitmap` and is blitted with `DrawBitmap`. Text stays native (`wxStaticText` or DC text), so it keeps ClearType on Windows and Pango on Linux.
- In C++ wx you would use `wxGraphicsContext::Create(paintDC)` instead (Direct2D, GDI+ or Cairo). wxDragon would need about 50 lines of FFI to expose it.
- The workaround costs no extra crate, and it draws the same on Windows and GTK.

**What was built:**
- **Slider** (`fluent::slider`):
  - 4 px rounded track: accent fill to the left of the thumb, grey to the right.
  - 20 px thumb with a 1 px border and an accent inner dot: 6 px radius, 7 px on hover, 5 px while dragging.
  - Click-to-jump and drag with mouse capture.
  - Keys: ←/→/↑/↓ ±1, PgUp/PgDn ±10, Home/End.
  - Tested with posted mouse and key messages: dragging to 63 and pressing → three times both update the value label.
- **Toggle switch:**
  - 40×20 pill. Off: outlined, with a grey knob on the left. On: accent fill, with a TextOnAccent knob on the right. The knob grows on hover.
  - Click, Space or Enter toggles, and the Off/On label on the left follows.
  - Tested by click: the "On" state shows in the fill.
- **Radio buttons:**
  - 20 px Fluent circles. Selected: accent fill with a TextOnAccent dot. Unselected: a 1 px ring.
  - Hover fill; clicking the label also selects.
  - Arrow keys move the selection within the group.
  - Tested by click.
- **Check box:** none was left once the toggles were custom, so none was drawn.
- **Buttons** (Standby, Power off): rounded 4 px, with a 1 px border, hover and pressed fills, and native text.
- **Cards:** anti-aliased 5 px radius with a 1 px outline (#1C1C1C in dark, #EBE1E3 in light). This replaces the jagged GDI corners.
- **Nav:**
  - Anti-aliased rounded selection pill plus a 3×16 accent indicator pill.
  - Hover fill on items, including App settings.
- **Focus:**
  - A 2 px rounded ring, shown only after keyboard use (Tab or arrows), as in WinUI. A click hides it again.
  - Tab and Shift+Tab work through the custom controls. They need `wxWANTS_CHARS` to get arrow keys, and must then forward Tab with `Navigate()`.
  - Tested: Tab from Brightness moves the ring to Contrast.
- **Input glyph:** fixed to keyboard-and-mouse.

**Still different from real Fluent/WinUI:**
- **No animation:** no thumb grow, switch slide, or colour transitions. It would need a `wxTimer` per control: about ½ day.
- **Combo boxes are still native `wxChoice`.** A Fluent combo means a custom popup (`wxPopupTransientWindow` plus an owner-drawn list): about 1 day.
- **Accessibility.** The custom panels expose no role, name or value to Narrator or Orca. wxDragon has `set_accessibility_*` setters (not used here); a proper slider, switch and radio would need a `wxAccessible` subclass. Estimate: 1 day, MSW only, since wxAccessible is MSW-only in wx.
- **No DPI scaling.** The controls use fixed pixel sizes, which is correct at 100 %. Scaling them needs `FromDIP()` everywhere: about 1–2 h.
- **No Mica.** The title bar is native and the backdrop flat, as in the native variant.
- **The expander body is still a separate panel below the card**, and the hamburger does nothing.
- **Hover could not be verified by synthetic input.** wx checks the real cursor position, so posted mouse moves don't trigger it. The hover code is wired up but was not seen in a screenshot.
- **Linux:** the same drawing works under GTK. Fonts are the GTK default (no Segoe UI), so two-line subtitles wrap more.

## Accent colour findings

- **Windows: read and used.**
  - The variant reads `HKCU\Software\Microsoft\Windows\CurrentVersion\Explorer\Accent\AccentPalette`. This is 32 bytes holding 8 RGBA shades: Light3, Light2, Light1, Accent, Dark1, Dark2, Dark3, unused.
  - Like WinUI's AccentFillColorDefault, it uses **Light2 in dark mode** and **Dark1 in light mode**.
  - If the palette is missing, it falls back to `HKCU\Software\Microsoft\Windows\DWM\AccentColor` (0xAABBGGRR, shaded).
  - On this machine the accent is #D3452C. The app reports `accent #ED9675` (dark) and `#B43622` (light) from AccentPalette (run with `TVC_PRINT_ACCENT=1`). Those are the colours in the `wx-fluent-win-*` screenshots.
  - Only `advapi32!RegGetValueW` is used; no crates were added.
- **Live updates: wired, not tested.**
  - The frame binds `wxEVT_SYS_COLOUR_CHANGED`, re-reads the palette and repaints.
  - This is not verified, because the brief forbids changing the system theme.
  - Windows announces accent changes with `WM_SETTINGCHANGE("ImmersiveColorSet")` and `WM_DWMCOLORIZATIONCOLORCHANGED`. Whether wx 3.3.4 turns those into `wxEVT_SYS_COLOUR_CHANGED` was not checked. If it doesn't, it needs a raw `WM_SETTINGCHANGE` hook, which wxDragon doesn't expose: about 2 h with `MSWWindowProc` FFI, or a 2 s poll of the registry value.
- **Native Win32 controls do not follow the accent.** Measured from the native screenshots while the system accent was #D3452C:
  - the trackbar thumb is **#0078D7** in both themes;
  - the radio dot ring is **#60CDFF** (dark) and **#005FB8** (light).

  Those are Windows' *default blue* Light2 and Dark1, not the user's accent. comctl32 v6 draws from the visual style (`aero.msstyles`), which has fixed colours. `SetWindowTheme`, `wxWindow::SetForegroundColour` and wx's dark-mode code don't change them.

  The only Win32 routes are custom drawing: `NM_CUSTOMDRAW` for the trackbar thumb and channel, or owner-draw for buttons and check boxes. That is the same work as the fluent variant.
- **Linux: GTK3 does not follow the portal accent.**
  - The fluent variant first tries `gdbus call … org.freedesktop.portal.Settings.ReadOne org.freedesktop.appearance accent-color`. This setting is published by xdg-desktop-portal on GNOME 47+ and KDE Plasma 6.
  - On WSLg there is a session bus, but no `org.freedesktop.portal.Settings` interface ("No such interface"), so the variant falls back to GTK's theme selection colour. That is Adwaita's fixed #3584E4 in light, and #15539E in dark, lifted by 35 % toward white so it reads as a fill.
  - **GTK3's native controls never follow the portal accent.** Adwaita for GTK3 has a fixed blue. In `wx-linux-*` the native slider fill is #3584E4 (light) and #15539E (dark), and the radio fills are the same blues.
  - Only libadwaita (GTK4) apps re-colour from the accent setting, per the GNOME 47 release notes (not re-fetched here).
  - So on Linux a wx app gets the user's accent only through custom drawing plus the portal read, as done here.

## Linux build (WSL2 Ubuntu 24.04, wxDragon on GTK3)

- **The same wxDragon 0.9.24 build works on GTK3.** wx 3.3.4 is built from source and linked statically. No fallback to the system wx 3.2.4 was needed.
- **Obstacles, all environmental:**
  - **Flaky DNS inside WSL.** crates.io and archive.ubuntu.com failed to resolve intermittently. Fixed by vendoring the crates from Windows (`cargo vendor`), reusing the downloaded wx source via `WXWIDGETS_DIR`, and building with `--offline`.
  - **`libssl-dev` missing.** `wxdragon-sys`'s build script pulls in `reqwest` with native-tls. The .debs were downloaded on Windows and installed with `dpkg -i`.

  wx itself built in **9 min** on 12 threads.
- **Private fonts are compiled out on GTK.** wx's CMake defaults `wxUSE_PRIVATE_FONTS` to OFF unless the platform is Windows, macOS or Qt, and wxDragon doesn't override it. So `wxFont::AddPrivateFont` does nothing, and the first Linux screenshots showed hex boxes instead of icons.
  - The workaround in `main.rs` (non-Windows only) copies the TTF to `~/.local/share/fonts` before GTK starts.
  - The real fix is `-DwxUSE_PRIVATE_FONTS=ON`, which needs fontconfig and pangoft2; both are present. That is a 1-line change in wxdragon-sys's `cpp/CMakeLists.txt`, worth upstreaming.
- **Shipped size:**
  - the binary is **6.61 MB** stripped, single file;
  - **2.34 MB** as .tar.gz alone;
  - **3.50 MB** .tar.gz with the 2.84 MB TTF.
- **Runtime libraries beyond GTK3** (`ldd` of the binary minus `ldd` of `libgtk-3.so.0`):
  - `libSM.so.6` and `libICE.so.6` (X session management, from wx);
  - `libuuid.so.1`;
  - `libstdc++.so.6` and `libgcc_s.so.1`.

  All are present on any desktop distribution. libayatana-appindicator is not linked.
- **Not measured on Linux:** start time and PSS.

## Rebuild

Windows, from a short path (MSVC 260-character limit):
1. Call `vcvars64.bat`.
2. Put cmake and ninja on PATH (`pip install cmake ninja`).
3. `set LIBCLANG_PATH=<venv>\Lib\site-packages\clang\native` (from `pip install libclang`).
4. Optionally `set TVC_ICO=<repo>\assets\brand\ico\ThinkVisionControl.ico`.
5. `cargo build --release`.
6. Copy `src/ThinkVisionControl.Avalonia/Assets/Fonts/FluentSystemIcons-Regular.ttf` next to the exe, or set `TVC_ICON_FONT`.

Linux (Ubuntu 24.04 packages: build-essential, libgtk-3-dev, cmake, clang/libclang, libssl-dev, pkg-config):
1. `LIBCLANG_PATH=/usr/lib/x86_64-linux-gnu cargo build --release`.
2. Put the TTF next to the binary.
3. Run with `GTK_THEME=Adwaita:dark ./tvc-wx --dark [--fluent] [--monitor]`.
4. On WSLg, use `GDK_BACKEND=x11` so `xwd` can capture.
