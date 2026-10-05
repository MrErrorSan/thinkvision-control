# Review checklist

Before reviewing, make one contact sheet per candidate:

- the app icon at 16, 24, 32, 48 and 256 px;
- the glyph at 16, 20, 24 and 32 px;
- each on `#F3F3F3` (light) and `#202020` (dark), viewed at 100% zoom with no smoothing.

## Must pass

| # | Check | How | Pass when |
| --- | --- | --- | --- |
| 1 | **16 px legibility** | Look at the 16 px glyph and icon on light and dark backgrounds from arm's length | You can name it "a monitor with a control" without a label, and the dot is still visible (not a blur) |
| 2 | **24 px legibility** | Same, at 24 px | All elements are separate, and no strokes merge |
| 3 | **Real taskbar** | Load `Tray.OnLight.ico` and `Tray.OnDark.ico` into the app, then switch Windows mode (Settings > Personalization > Colors) at 100% and 150% display scale | Sits comfortably next to Windows' own tray icons. Not brighter, bolder or blurrier than them |
| 4 | **Monochrome** | View the glyph in pure black and pure white, with no accent colour | Still reads. The concept doesn't depend on the colour of the dot |
| 5 | **Dark-background tile** | The app icon on `#202020` and on a dark macOS-style Dock mock-up | The mark and dot carry the icon even though the tile's edge is faint |
| 6 | **Not a neighbour** | Place it beside PowerToys Power Display (grey monitor), Monitorian (rays around a rectangle), Twinkle Tray (violet star; sun tray icon) and the Windows brightness sun | Nobody confuses it with any of them at 24 px |
| 7 | **No trademark confusion** | Compare it with the Lenovo logo, the red badge, the ThinkPad/ThinkVision wordmarks and Lenovo/TrackPoint red. Run a reverse image search (Google Lens or TinEye) on the 256 px icon | No Lenovo lettering, badge or exact red. A stranger wouldn't think it is an official Lenovo app |
| 8 | **Colour-blind** | Windows Settings > Accessibility > Color filters (red-green and blue-yellow filters) | The dot is still distinguishable from the mark by brightness |

Score each candidate pass or fail on every row. Any fail on rows 1, 4 or 7 rules the candidate out.

## Sign-off: files to commit

| Path | Contents |
| --- | --- |
| `assets/brand/master/app-icon.svg` | Master app icon (layers `tile`, `mark`, `dot`) |
| `assets/brand/master/glyph.svg` | Hand-tuned glyph, artboards 16 / 20 / 24 / 32 |
| `assets/brand/master/wordmark-lockup.svg` | Icon plus "ThinkVision Control" in Inter Semibold (text converted to outlines) |
| `assets/brand/previews/contact-sheet.png` | The review sheet above, for future reference |
| `src/ThinkVisionControl/Assets/ThinkVisionControl.ico` | Replaces the current single 64 px placeholder |
| `src/ThinkVisionControl/Assets/Tray/Tray.OnDark.ico`, `Tray.OnLight.ico` | Tray glyphs (+ optional `…Disconnected` / `…Error`) |
| `assets/brand/macos/`, `assets/brand/linux/` | Only when those ports start (see [exports.md](exports.md)) |

Wiring the new tray `.ico` files into the app replaces the icon currently drawn in code in `Tray/TrayIcon.cs`. That is a separate code change.
