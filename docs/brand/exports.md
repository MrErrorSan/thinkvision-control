# Exports: one master, every platform

Draw the icon once as vector, then export it per platform from two drawings.

## The two masters

| File | What it is |
| --- | --- |
| `assets/brand/master/app-icon.svg` | 1024 × 1024 canvas: graphite tile (864 px, about 160 px radius), mark in `paper`, dot in `ember`. Each is on its own named layer: `tile`, `mark`, `dot` |
| `assets/brand/master/glyph.svg` | 16 × 16 canvas, one colour (black), no tile: the hand-tuned small glyph from the brief. Add 20, 24 and 32 px artboards, each nudged onto whole pixels |

Rules:

- **Up to 32 px, export from `glyph.svg`.** The app icon at 16–32 px also uses the hand-tuned mark (on the tile).
- **From 48 px up, export from `app-icon.svg`.**
- **Never upscale a bitmap.**

## Export table

| OS | File | Sizes / format | Variants |
| --- | --- | --- | --- |
| **Windows** | `ThinkVisionControl.ico` (app) | 16, 20, 24, 30, 32, 36, 40, 48, 60, 64, 72, 80, 96, 256 px, 32-bit RGBA. The 256 image is PNG-compressed. Minimum set: 16, 24, 32, 48, 256 | One file, used on both themes |
| | `Tray.OnDark.ico`, `Tray.OnLight.ico` | 16, 20, 24, 32 (+40, 48, 64 for 250–400% scale) | White glyph / near-black glyph. Optional `…Disconnected` and `…Error` versions with a bottom-right badge |
| **macOS** | `AppIcon.icon` (Icon Composer, macOS 26+) | Layers as SVG: `mark`, `dot`. Background = solid `graphite` (the system draws the rounded tile and glass) | Default, dark and mono previews in Icon Composer |
| | `AppIcon.icns` (older macOS / non-Xcode builds) | 16, 32, 128, 256, 512 pt at @1x and @2x (16–1024 px) | — |
| | `MenuBarTemplate.pdf` (or `.svg`) | The glyph about 16–18 pt tall on the 24 pt menu bar. Black + transparent only, marked as a template image | The system recolours it for light, dark and selected |
| **Linux** | `<app-id>.png` in `hicolor/<n>x<n>/apps/` | 16, 22, 24, 32, 48, 64, 128, 256, 512 px | — |
| | `<app-id>.svg` in `hicolor/scalable/apps/` | The SVG app icon | — |
| | `<app-id>-symbolic.svg` in `hicolor/symbolic/apps/` | The glyph, 16 × 16, one solid colour | Recoloured by the desktop, used in panels and the tray |
| *Optional mobile* | Android adaptive icon / iOS 1024 | Android: 108 dp layers, keep the mark inside the 66 dp safe zone, plus a monochrome layer. iOS: the same `AppIcon.icon` | — |

`<app-id>` is a reverse-DNS ID you own, for example `io.github.<owner>.ThinkVisionControl`. Never use `com.lenovo.*`.

## Platform rules worth knowing (short)

- **Windows:**
  - Picks the nearest larger size and scales down, so include 256.
  - Tray icons are your own art. Windows does not recolour them, which is why there are two tray files.
  - Status badges go bottom right, one at a time.
- **macOS 26:**
  - Icons that don't fill the rounded-square shape get put on a grey plate.
  - Let Icon Composer draw the tile, and don't bake in highlights or shadows.
- **Linux / Flathub:**
  - SVG, or a PNG of at least 256 px, square.
  - Don't let the art touch the canvas edges.
  - No baked-in shadows or glows.
  - The icon must read on light and dark backgrounds. The graphite tile with paper mark does.

## Commands (reference)

```sh
# PNGs from SVG (Inkscape 1.x)
inkscape app-icon.svg --export-type=png -w 256 -h 256 --export-filename=icon-256.png

# Windows .ico from per-size PNGs (ImageMagick 7)
magick icon-16.png icon-20.png icon-24.png icon-32.png icon-48.png icon-64.png icon-256.png ThinkVisionControl.ico

# macOS .icns from an .iconset folder (on a Mac)
iconutil -c icns AppIcon.iconset
```

## Sources

- [Construct your Windows app's icon](https://learn.microsoft.com/en-us/windows/apps/design/iconography/app-icon-construction): scale table and minimum sizes
- [Notification area guidelines](https://learn.microsoft.com/en-us/windows/win32/uxguide/winenv-notification): tray sizes and overlays
- [Apple HIG: App icons](https://developer.apple.com/design/human-interface-guidelines/app-icons) and [Creating your app icon using Icon Composer](https://developer.apple.com/documentation/xcode/creating-your-app-icon-using-icon-composer)
- [Apple HIG: The menu bar](https://developer.apple.com/design/human-interface-guidelines/the-menu-bar) (24 pt bar, black-and-clear icons)
- [Icon Theme Specification](https://specifications.freedesktop.org/icon-theme/latest/) and [Flatpak conventions: application icons](https://docs.flatpak.org/en/latest/conventions.html)
- [Flathub quality guidelines](https://docs.flathub.org/docs/for-app-authors/metainfo-guidelines/quality-guidelines)
- [Android adaptive icons](https://developer.android.com/develop/ui/views/launch/icon_design_adaptive)

## Not verified

- **The Windows .ico list.** It is derived from Microsoft's scale table and the MSIX target-size list. The page's own "Icon sizes (Win32)" table renders empty.
- **The 16–18 pt menu-bar glyph height.** This is a common convention. Apple only states the 24 pt bar height.
- **The macOS 26 grey-plate behaviour.** This comes from press reports (heise, Daring Fireball), not from Apple's documentation.
- **Near-black for the light-taskbar tray glyph.** Microsoft publishes no exact value. Match it by eye against Windows' own tray icons.
