# AI image prompts

Use these prompts to explore the three concepts in [logo-design-brief.md](logo-design-brief.md). AI output is a **sketch, not the final icon**. The final icon is redrawn as clean vector (see the last section).

## Settings per tool

| Tool | Settings |
| --- | --- |
| Midjourney | Append `--ar 1:1 --style raw --stylize 50`. Add the negative list after `--no` |
| ChatGPT / GPT Image | Ask for "square 1024×1024 image". Paste the negative list as "Avoid: …" |
| Adobe Firefly | Content type: Graphic. Aspect ratio: Square. Paste the negative list into the exclude field |
| Ideogram | Style: Design. Magic Prompt: off. Negative prompt field available |

Run each prompt 2–3 times. Then reuse the best result as an image reference ("same style as this, but…") for the other variants.

## Shared blocks

**Style (append to every prompt):**

> minimal flat vector app icon, front-facing, precise geometry, angular shapes with slightly softened corners, consistent stroke weight, generous negative space, dual-tone graphite (#23272E) and off-white (#F7F8FA) with a single small vermilion (#F05A28) accent dot, calm, engineered, restrained, Windows 11 Fluent and macOS style, centered, crisp edges

**Negative list (exclude in every prompt):**

> text, letters, words, numbers, logo lettering, Lenovo, ThinkPad, Think wordmark, red square badge, keyboard, laptop, sun, sun rays, light bulb, moon, photorealistic, 3D render, isometric, perspective, glossy reflection, lens flare, glow, neon, gradients mesh, drop shadow, bevel, texture, noise, clutter, multiple objects, purple

## Concept 1: Knob (lead)

**App icon, light background:**

> app icon of a dark graphite rounded-square tile on a plain light grey background; inside the tile, a thin off-white outline of a wide computer screen (16:10), no stand; the bottom edge of the screen outline doubles as a slider track, with a small solid vermilion circle sitting on that bottom edge two-thirds of the way along like a slider knob, with a tiny gap on each side of the knob; nothing else. [style]

**App icon, dark background:** use the same prompt with "plain near-black background (#202020)" instead.

**Monochrome glyph:**

> a single-color black line icon on pure white, 16 by 16 pixel grid style, a rectangle screen outline with 1-pixel stroke and square-ish corners, bottom edge interrupted by a small solid circle like a slider knob two-thirds along, no stand, no shading, pixel-perfect, like a Windows system tray icon. [negative list]

## Concept 2: Status LED

**App icon, light background:**

> app icon of a dark graphite rounded-square tile on a plain light grey background; inside it, an off-white front-facing computer monitor with a short neck and wide flat foot; the screen is split vertically in two tones, left half an empty outline and right half solid off-white; the monitor has a slightly thicker bottom bezel with one tiny vermilion pill-shaped status LED at the bottom right of the bezel; nothing else. [style]

**App icon, dark background:** use the same prompt with "plain near-black background (#202020)".

**Monochrome glyph:**

> a single-color black icon on pure white, 16 by 16 pixel grid style, front-facing monitor with short stand, screen left half outlined and right half solid black, thick bottom bezel with a tiny cut-out rectangle as a status light at bottom right, no shading, pixel-perfect, like a Windows system tray icon. [negative list]

## Concept 3: Point

**App icon, light background:**

> app icon of a dark graphite rounded-square tile on a plain light grey background; inside it, a thin off-white outline of a front-facing computer monitor on a short neck and flat foot; the screen is empty except for one small solid vermilion dot placed slightly below and right of center, like a single lit pixel or a pointing nub; nothing else. [style]

**App icon, dark background:** use the same prompt with "plain near-black background (#202020)".

**Monochrome glyph:**

> a single-color black line icon on pure white, 16 by 16 pixel grid style, monitor outline with 1-pixel stroke on a short stand, one small solid dot inside the screen slightly right of and below center, no shading, pixel-perfect, like a Windows system tray icon. [negative list]

## Wordmark lockup (layout exploration only)

> horizontal logo lockup on white: on the left the [concept name] app icon described above, on the right the words "ThinkVision Control" in a clean neutral grotesque sans-serif similar to Inter, semibold, dark graphite, upright, no italics, generous spacing, minimal, corporate-calm. [style, minus "app icon"]

AI tools distort letters. Use the output only to judge proportion and spacing, then set the real text in Inter Semibold.

## From AI sketch to clean vector

1. **Redraw; don't trace.** Place the best image as a locked, 30%-opacity reference in Figma, Illustrator or Inkscape, and rebuild it with rectangles, circles and strokes on the 48-unit grid from the brief. Auto-trace gives wobbly curves and too many nodes. A hand-built redraw is also clearly your own work, which matters because purely AI-generated images may not be protectable by copyright ([US Copyright Office, AI report part 2](https://www.copyright.gov/ai/)).
2. **Snap to whole pixels at each small size.** Make 16, 20, 24 and 32 px artboards, turn on the pixel grid with snap to pixel, and set strokes to whole pixels (1 px at 16). Edges should land on pixel boundaries.
3. **Hint by hand.** At 16 px, copy the brief's ASCII drawing exactly. Fix any blurry half-pixel edges, and keep at least 1 px of space between the dot and the line.
4. **Lock the colours** to the brief's tokens (`graphite`, `paper`, `ember`), and remove any gradients the AI added.
5. **Export** following [exports.md](exports.md), then run [review-checklist.md](review-checklist.md).
