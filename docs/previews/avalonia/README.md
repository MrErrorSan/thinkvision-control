# Avalonia preview of ThinkVision Control (throwaway spike)

These screenshots come from a throwaway Avalonia app that copies layout A from `docs/mockups/thinkvision-control-ui.html`. It uses mock data and does no DDC. The code is not in this repo.

- **Libraries:** Avalonia 12.1.3 and FluentAvaloniaUI 3.1.0. FA 3.1.0 targets `net10.0` and Avalonia 12.1, and it worked, so this preview does not use Avalonia's built-in FluentTheme. The FA 3.x types have an `FA` prefix: `FANavigationView`, `FASettingsExpander`, `FAInfoBar`, `FAFontIconSource`.
- **Theme:** `FluentAvaloniaTheme` with `PreferSystemTheme` and `PreferUserAccentColor` turned on. The system accent on this machine is red, so the accents are red. When `--theme` is passed, `RequestedThemeVariant` overrides the system theme.
- **Backdrop:** the main window uses `TransparencyLevelHint = [Mica, AcrylicBlur, None]` with a transparent background. The flyout uses `AcrylicBlur` plus an `ExperimentalAcrylicBorder` for the tint.
- **Build and capture:** .NET 10, Release, framework-dependent JIT build at 100% scaling on a 1920x1080 screen. Each window was brought to the foreground and captured with `Graphics.CopyFromScreen` using its DWM frame bounds.

## Images

| File | What it shows |
|---|---|
| `display-dark-connected.png` | Display page in dark mode, connected. Mica is on and the native Windows title bar is used. |
| `display-dark-connected-fullpage.png` | The same page with a taller window, so the Input and Presets sections are visible. |
| `display-light-connected.png` | Display page in light mode, connected. Mica is on. |
| `display-dark-notfound.png` | Not-found state: InfoBar with "Check again", controls disabled, sliders showing "—". |
| `monitor-dark.png` | Monitor settings page with ComboBoxes, ToggleSwitches (On/Off label on the left) and the Power buttons. |
| `flyout-dark.png` | Borderless tray flyout, 360 wide, acrylic, dark mode. |
| `flyout-light.png` | The same flyout in light mode. |
| `display-dark-software.png` | `--software` (Win32 software rendering). **There is no Mica:** `ActualTransparencyLevel` is `None` and the background is solid. |
| `display-dark-extended-titlebar.png` | Extra shot with `ExtendClientAreaToDecorationsHint=true`, where Avalonia 12 draws the title bar itself. Its full-screen caption button had to be hidden in code, and this mode has no window icon. |
| `compare-wpf-vs-avalonia-dark.png` | Not-found state side by side: the current WPF app on the left, Avalonia on the right, at the same 1008x720 outer window size. |

## Numbers

Each mode was started 6 times. The table shows the median of runs 2 to 6. "Window" is the time from `Start-Process` until a visible top-level window appears. "First frame" is the time from process start to the first rendered frame, as logged by the app. Memory was read 3 s after the window appeared.

| Mode | Window (ms) | First frame (ms) | Private (MB) | Working set (MB) | Threads |
|---|---|---|---|---|---|
| Main window, GPU (default) | ~1,600 | ~1,600 | ~109 | ~154 | 36 |
| Main window, `--software` | ~1,450 | ~1,470 | ~49 | ~106 | 12 |
| Flyout only, GPU | ~1,250 | ~1,260 | ~93 | ~143 | 36 |

For comparison, the earlier probe in `research_notes/Cross platform monitor control/ui_framework.md` measured the WPF window at about 43 MB private, about 86 MB working set and about 0.5 s to the window. With FluentAvalonia and the full page, this preview uses more memory than that probe's bare Avalonia window. NativeAOT was not tried here.

## Differences from WPF and the mockup

- **Cards are taller.** A one-line `FASettingsExpander` is about 78 px tall, against 66 px in the WPF app and the mockup, so less fits on screen.
- **The InfoBar wraps.** `FAInfoBar` puts the message and the "Check again" button on their own lines. WPF puts the button on the right of the same row.
- **The InfoBar icon differs.** Informational severity uses the accent colour, which is red on this machine, so it looks similar to WPF's red Error icon even though the meaning differs.
- **The content area has a frame.** `FANavigationView` draws its WinUI-style content layer with a rounded top-left corner and a lighter fill. The WPF app has a flat content area.
- **A disabled expander still looks partly enabled.** On the disabled "Custom colour" expander, the chevron area keeps an enabled-looking background.
- **ToggleSwitch needed a workaround.** By default its On/Off text sits to the right of the switch. To match the Windows Settings layout, the preview uses a separate label on the left and a -12 px margin.
- **Light acrylic is too grey.** The flyout's light acrylic comes out mid-grey over a dark window behind it. Windows 11 light acrylic is closer to white. Dark acrylic looks right.
- **The title bar differs.** Avalonia 12 no longer leaves native caption buttons in place when the client area is extended; it draws its own caption buttons, including a full-screen button. The main shots therefore use the native title bar, which keeps Mica, the snap behaviour and the icon. The mockup's custom title bar (app icon plus title in client area) is only in the extended-titlebar shot, and that shot has no icon.
- **Fonts are close.** FluentAvalonia uses Segoe UI Variable on Windows, and the icons are Segoe Fluent Icons. Weights and sizes look very close to WPF. Text rendering is slightly softer (Skia).
- **The scroll position jumped.** A few captures opened scrolled to the bottom; those were retaken.

## Linux (Ubuntu 24.04 under WSLg, X11 backend)

Captured from inside Linux by window id (`import -window`), so only the app's own pixels are shown; window decorations are not included (WSLg draws its own, which is neither GNOME nor KDE).

| Image | Shows |
|---|---|
| `linux/linux-display-dark-notfound.png` | Display page, dark, monitor not found |
| `linux/linux-display-light-connected.png` | Display page, light, connected |
| `linux/linux-monitor-dark.png` | Monitor settings page, dark |
| `linux/linux-flyout-dark.png` | Quick flyout, dark |

What it shows about the port:
- Layout and controls are identical to Windows (Avalonia draws everything itself; GTK/Qt themes are not used).
- Text falls back to the Ubuntu font — fine.
- Icons render as empty boxes: Segoe Fluent Icons is a Windows font and can't be shipped. The real port must bundle an MIT icon font (Fluent UI System Icons).
- Accent is Avalonia's default purple: there is no system accent to read here; the port should read the desktop's accent (portal `accent-color`) or use the brand ember.
- Mica/acrylic don't exist on Linux; Avalonia falls back to plain *transparency* (a see-through window), so the port must switch to a solid themed background off Windows — the preview does this.
