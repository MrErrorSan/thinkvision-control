# Logo design brief: ThinkVision Control

This brief describes one minimal icon that works on every platform. Hand it to a designer as is, or paste sections into an image tool alongside [ai-image-prompts.md](ai-image-prompts.md). Export sizes are in [exports.md](exports.md) and the review checks are in [review-checklist.md](review-checklist.md).

## The product

ThinkVision Control is a small, unofficial desktop utility. It controls a Lenovo ThinkVision P32p-20 monitor from the computer over DDC/CI: brightness, contrast, input source, colour mode, Over Drive, and Day/Night presets. People reach it through hotkeys, a tray icon with a quick flyout, a jump list and a settings window. It exists because the monitor's physical buttons are failing and hard to find in the dark.

Today it runs on Windows 11 with Fluent styling. macOS and Linux versions come later.

## The goal

**One mark, used everywhere.** It must be minimal and clean, read at 16 px, and work as flat art on both light and dark backgrounds. It should *feel* like a control for a Lenovo monitor (engineered, restrained, dark with one warm accent) without copying anything Lenovo owns.

The mark exists in two forms:

| Form | What it is | Where |
| --- | --- | --- |
| **App icon** | The mark in light colour plus the accent dot, on a graphite rounded-square tile | Taskbar, Start, Dock, app grids, stores, README |
| **Glyph** | The mark alone, one colour, no tile | Tray (Windows), menu bar (macOS), panel (Linux) |

## Personality

**Calm, precise, native, quiet.** That means:

- few elements, generous whitespace, and exact geometry;
- one warm accent and nothing else loud;
- nothing that flashes, glows or shines.

## Lenovo-inspired cues (inspiration, not copies)

| Cue | How we use it | How we keep it our own |
| --- | --- | --- |
| **The single small dot** (a nod to the TrackPoint) | One accent dot that *means something* in this app: a slider knob, a status LED or a pixel | It is part of a display drawing, never a red nub on black keys. It sits where a control would sit, not in the centre of a keyboard |
| **Dual tone** | Graphite or near-black plus one warm accent. Everything else is neutral | The accent is our own hue (below), not Lenovo or ThinkPad red |
| **Engineered feel** | Angular but softened corners, straight lines, consistent stroke, nothing decorative | Our own proportions. No ThinkPad or ThinkVision industrial-design details (stand shape, bezel logo, the red "i" dot) |

**Avoid entirely:**

- the Lenovo wordmark or logotype
- the red square or tab badge
- "Think" lettering or its italic styling
- the exact Lenovo/ThinkPad red (about `#E2231A`)
- product photos or a 3D model of the P32p-20
- a sun or rays as the main symbol (they clash with the system brightness glyph and with Monitorian and Twinkle Tray)

**Watch out for:** a plain grey monitor alone is already PowerToys Power Display's icon. The dot is what makes ours distinct.

## Colour

| Token | Hex | Role | Contrast vs `#F3F3F3` (light) | vs `#202020` (dark) |
| --- | --- | --- | --- | --- |
| `graphite` | `#23272E` | Tile, or the mark on light backgrounds | 13.5 | 1.1 |
| `graphite-lift` | `#2E333B` | Top of the tile's 2-step gradient (optional) | 11.5 | 1.3 |
| `paper` | `#F7F8FA` | The mark on the tile, or on dark backgrounds | 1.0 | 15.3 |
| `steel` | `#8A919C` | Secondary line, if one is ever needed | 2.9 | 5.1 |
| **`ember`** | **`#F05A28`** | **The accent dot only** | 3.1 | 4.8 |

- **Ember is a vermilion, deliberately shifted from Lenovo red.** Its hue is about 15° (Lenovo red is about 3°), and it is lighter and a touch more orange. It still reads clearly as a warm red-orange accent. On the graphite tile it reaches 4.4:1.
  - If you want it warmer still, try `#F26B3A`.
  - Don't go back towards `#E2231A`.
- **The dot is the only coloured element.** Keep it at roughly 3–5% of the icon's area.
- **On dark backgrounds** (Windows dark taskbar, macOS dark Dock), the graphite tile edge almost disappears (1.1:1). The paper mark and the ember dot carry the icon. To keep the tile's edge visible, add a 1 px inner hairline in white at about 12% opacity, or the subtle `graphite-lift` → `graphite` top-to-bottom gradient. Neither counts as a "baked shadow".
- **Glyph colours:** white on dark taskbars and menu bars, near-black on light ones. The dot is drawn in the same colour as the rest of the glyph, because a red dot in the tray reads as an error badge.

## Three concepts

All three share one construction.

- **Mark grid:** 48 × 48 units.
- **Frame:** the screen outline, 36 × 26 units, a 3-unit stroke, a 2-unit outer corner radius and a 1-unit inner radius. That is angular, only slightly softened, as Microsoft's own icon grid uses ([app icon design](https://learn.microsoft.com/en-us/windows/apps/design/iconography/app-icon-design)).
- **Dot:** a circle of 7–8 units (about 2.5× the stroke).
- **Small sizes:** the 16 px glyph is hand-drawn as shown below, not scaled down.

Key for the 16 × 16 drawings: `#` is a solid pixel, `+` is a softened corner pixel (about 50% opacity), `o` is the dot, and `.` is empty.

### 1. Knob — recommended

The bottom edge of the screen doubles as a slider track. The dot is the knob, sitting on that edge at about two-thirds of the way along, with a one-pixel gap on each side. There is no stand. It reads as "a display you adjust", and it's the most minimal of the three.

```
  0123456789ABCDEF
0 ................
1 ................
2 ................
3 .+############+.
4 .#............#.
5 .#............#.
6 .#............#.
7 .#............#.
8 .#............#.
9 .#............#.
A .#............#.
B .#.......ooo..#.
C .+######.ooo.#+.
D .........ooo....
E ................
F ................
```

- **48 grid:** frame at x 6–42, y 10–36. The dot is centred on the bottom stroke at x ≈ 30, with a 2-unit gap either side.
- **Strengths:** reads as "monitor + control" in two elements, works at 16 px, and is unlike any neighbouring app.
- **Risk:** without a stand it could read as a generic "window" with a slider. Test it next to a media-player icon. If it's confused, add a 2-unit foot under the frame.

### 2. Status LED

A dual-tone monitor. The screen is split vertically: the left half is outline only, the right half is filled, which suggests brightness and contrast. A thicker chin carries the dot as a power or status LED at bottom right. A short neck and foot sit underneath.

```
  0123456789ABCDEF
0 ................
1 ................
2 .+############+.
3 .#......#######.
4 .#......#######.
5 .#......#######.
6 .#......#######.
7 .#......#######.
8 .#......#######.
9 .##############.
A .##########oo##.
B .+############+.
C .......##.......
D .....######.....
E ................
F ................
```

- **48 grid:** frame x 6–42, y 8–34. The fill boundary is at x 24. The chin is 6 units tall. The LED is a 5 × 3-unit pill centred at x 36. The neck is 4 × 3 units and the foot 16 × 3 units.
- **In the glyph**, the LED is a knock-out (transparent) in the solid chin.
- **Strengths:** the most "hardware" and Lenovo-feeling of the three, and the dual tone shows directly.
- **Risk:** the half-filled screen resembles a "contrast" glyph or a split-view icon, and the LED is tiny at 16 px (2 × 1 px). Check at 16 px on a light taskbar.

### 3. Point

An empty screen frame on a short stand, with the dot as a single point near the centre. It is the most direct TrackPoint nod, re-read as "the point you control" or "the one lit pixel".

```
  0123456789ABCDEF
0 ................
1 ................
2 .+############+.
3 .#............#.
4 .#............#.
5 .#.....oo.....#.
6 .#....oooo....#.
7 .#....oooo....#.
8 .#.....oo.....#.
9 .#............#.
A .#............#.
B .+############+.
C .......##.......
D .....######.....
E ................
F ................
```

- **48 grid:** frame x 6–42, y 8–34, with the dot (8 units) at the frame centre. The neck and foot are as in concept 2.
- **Strengths:** iconic, symmetrical, the calmest of the three, and the strongest Lenovo association.
- **Risk:** a centred dot in a frame also reads as "screen recording", "camera" or "cast". As a mitigation, place the dot off-centre (lower right, about 60/60%) so it reads as a pixel or cursor rather than a record button.

**Recommendation:** take **Knob** forward as the lead, and keep **Point** (off-centre) as the challenger. Compare both at 16 and 24 px before deciding.

## App icon composition (all concepts)

- **Canvas:** 1024 × 1024. The graphite tile is 864 × 864, centred (an 80 px margin all round), with a corner radius of about 160 px.
- **The mark** is in `paper`, about 62% of the tile's width, and optically centred. Nudge it up 2–3% if it has a stand.
- **The dot** is in `ember`.
- **Flat colour only.** The single permitted extras are the 2-step tile gradient and the edge hairline. No glow, no reflections, no drop shadow under the tile. macOS adds its own glass effects, and Linux stores add their own shadow.

## Wordmark

- **"ThinkVision Control" set in Inter Semibold.** Inter is SIL Open Font License, free to use anywhere ([licence](https://github.com/rsms/inter/blob/master/LICENSE.txt)). GNOME's UI font, Adwaita Sans, is based on it.
- **Don't use Segoe UI Variable.** Microsoft states it "is not available for licensing or use outside of Microsoft products or on non-Windows platforms" ([Font redistribution FAQ](https://learn.microsoft.com/en-us/typography/fonts/font-faq)).
- **Plain upright letters:** no italic, no red, no special treatment of "Think".
- **Lockup:** the icon on the left, a gap of half the icon's width, then the wordmark at cap height ≈ 0.6 × icon height.

## Trademark note

- "ThinkVision" is a registered trademark of Lenovo ([Lenovo trademarks](https://lenovo.com/ca/en/legal/copytrade)). Describing compatibility is normally fine; looking like an official product is not.
- Put this line in the About section, the README and any store listing: "Unofficial. Not affiliated with or endorsed by Lenovo. ThinkVision is a trademark of Lenovo."
- Keep the mark free of letters. If the app is ever renamed, only the wordmark changes.
- This is not legal advice. Get a quick trademark opinion before any public store release under this name.
