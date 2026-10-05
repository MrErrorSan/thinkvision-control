# Windows 11 design foundations (Fluent for Windows 11 / WinUI 3) — value-level reference

Research date: 2026-10-05. Sources are Microsoft Learn pages unless noted. Each Learn page's `updated_at` stamp, as fetched, is given where it matters, so freshness can be judged. Units: Microsoft uses epx (effective pixels). Its docs say "epx, ep, and px are used interchangeably" ([Screen sizes and breakpoints](https://learn.microsoft.com/en-us/windows/apps/design/layout/screen-sizes-and-breakpoints-for-responsive-design)). For WPF, 1 epx = 1 WPF device-independent unit (inference).

## Design principles: what Microsoft states, and what they mean in practice

### Takeaway
Microsoft lists five Windows 11 principles: Effortless, Calm, Personal, Familiar and Complete + Coherent. Seven "signature experiences" put them into practice: color, elevation and layering, iconography, materials, geometry, typography and motion. In practice this means quiet neutral surfaces, accent colour used sparingly, soft rounded geometry, materials that follow the user's wallpaper and theme, and stock controls and motion instead of custom ones.

### Cited Findings
- Framing: "We have evolved our design language alongside of Fluent to create a design that is human, universal, and truly feels like Windows." (page ms.date 2025-12-11, updated 2026-07-14) — [Design principles](https://learn.microsoft.com/en-us/windows/apps/design/design-principles)
- **Effortless**: "Windows 11 is faster and more intuitive. It's easy to do what I want, with focus and precision." — [Design principles](https://learn.microsoft.com/en-us/windows/apps/design/design-principles)
- **Calm**: "softer and decluttered; it fades into the background to help me stay calm and focused. The experience feels warm, ethereal, and approachable." — [Design principles](https://learn.microsoft.com/en-us/windows/apps/design/design-principles)
- **Personal**: "adapts seamlessly to the way I use my device. It bends and flexes to my individual needs and preferences so that I can truly express myself." — [Design principles](https://learn.microsoft.com/en-us/windows/apps/design/design-principles)
- **Familiar**: "balances a new, refreshed look and feel with the familiarity of the Windows I already know. There is no learning curve; I can pick it up and go." — [Design principles](https://learn.microsoft.com/en-us/windows/apps/design/design-principles)
- **Complete + Coherent**: "offers a visually seamless experience across platforms… a consistent Windows experience." — [Design principles](https://learn.microsoft.com/en-us/windows/apps/design/design-principles)
- The signature experiences are listed as Color, Elevation and layering, Iconography, Materials, Shapes and geometry, Typography and Motion. Color "provide[s] a calming foundation… emphasizing significant items only when necessary". Materials "help us distinguish between focused and unfocused applications". Motion is "reactive, direct, and context appropriate". — [Design principles](https://learn.microsoft.com/en-us/windows/apps/design/design-principles)
- Geometry rationale: "Progressively rounded corners, nested elements, and consistent gutters combine to create a soft, calm, and approachable effect that emphasizes unity of purpose and ease of use." — [Geometry](https://learn.microsoft.com/en-us/windows/apps/design/signature-experiences/geometry)
- Each signature-experience page notes that it describes how the Fluent Design language (fluent2.microsoft.design) is *applied to Windows apps*. The Windows pages are the platform-specific layer on top of Fluent 2. — [Typography](https://learn.microsoft.com/en-us/windows/apps/design/signature-experiences/typography)

### Inferences
- **Calm** shows up as neutral greys, low-opacity fills, 1 px hairline strokes in place of heavy shadows, and accent colour kept for primary actions and selection state.
- **Personal** shows up as following the system light/dark theme, using `SystemAccentColor` and using Mica, which is tinted by the wallpaper.
- **Familiar** and **Complete + Coherent** argue for stock control shapes and sizes, the stock type ramp and stock motion, rather than a custom visual language.

### Gaps
- Microsoft gives no worked "principle → rule" mapping. The practical meanings above are my synthesis from the signature-experience pages.

---

## Typography: the Segoe UI Variable type ramp

### Takeaway
Use Segoe UI Variable throughout, with Regular weight for text and Semibold for titles, in sentence case and aligned left. The ramp in epx (size/line height) is: Caption 12/16, Body 14/20, Body Strong 14/20 Semibold, Body Large 18/24, Subtitle 20/28, Title 28/36, Title Large 40/52, Display 68/92. Minimum legible sizes are 12 px Regular and 14 px Semibold. Bold and Italic are not part of the ramp.

### Cited Findings
- Segoe UI Variable "is the new system font for Windows… uses variable font technology to dynamically provide great legibility at very small sizes, and improved outlines at display sizes." (page updated 2026-07-14) — [Typography](https://learn.microsoft.com/en-us/windows/apps/design/signature-experiences/typography)
- Axes: weight (`wght`) runs from Thin (100) to Bold (700). Optical size (`opsz`) is "automatic and on by default… for optical scaling from 8pt to 36pt". XAML common controls pick Segoe UI Variable by default and match the optical size to the requested font size automatically. — [Typography](https://learn.microsoft.com/en-us/windows/apps/design/signature-experiences/typography)
- Named weights: Light 300, Semilight 350, Regular 400, Semibold 600, Bold 700. — [Typography](https://learn.microsoft.com/en-us/windows/apps/design/signature-experiences/typography)
- **Best-practice table**:

  | Attribute | Value |
  | --- | --- |
  | Weight | Regular for most text, Semibold for titles |
  | Alignment | Left by default; centre only in rare cases such as text below icons |
  | Minimum values | **14px Semibold, 12px Regular**; "Text smaller than these sizes and weights are illegible in some languages" |
  | Casing | Sentence case "for all UI text, including titles" |
  | Truncation | "Use ellipses in most cases; clipping is only used in rare cases" |

  — [Typography](https://learn.microsoft.com/en-us/windows/apps/design/signature-experiences/typography)
- **Type ramp** (Windows 11). The "Weight" column on the page names the Segoe UI Variable optical variant: Small, Text or Display.

  | Style | Variant / weight | Size / line height (epx) | XAML style key |
  | --- | --- | --- | --- |
  | Caption | Small, Regular | 12 / 16 | `CaptionTextBlockStyle` |
  | Body | Text, Regular | 14 / 20 | `BodyTextBlockStyle` |
  | Body Strong | Text, Semibold | 14 / 20 | `BodyStrongTextBlockStyle` |
  | Body Large | Text, Regular | 18 / 24 | `BodyLargeTextBlockStyle` |
  | Body Large Strong | Text, Semibold | 18 / 24 | `BodyLargeStrongTextBlockStyle` |
  | Subtitle | Display, Semibold | 20 / 28 | `SubtitleTextBlockStyle` |
  | Title | Display, Semibold | 28 / 36 | `TitleTextBlockStyle` |
  | Title Large | Display, Semibold | 40 / 52 | `TitleLargeTextBlockStyle` |
  | Display | Display, Semibold | 68 / 92 | `DisplayTextBlockStyle` |

  — [Typography](https://learn.microsoft.com/en-us/windows/apps/design/signature-experiences/typography); style keys and weights from [XAML theme resources – type ramp](https://learn.microsoft.com/en-us/windows/apps/develop/platform/xaml/xaml-theme-resources) (updated 2026-08-19)
- "Bold and Italic styles are not part of the Windows type ramp. Use Semibold instead of Bold for emphasis. Italic is excluded because it can reduce readability and legibility, particularly for people with dyslexia." — [Typography](https://learn.microsoft.com/en-us/windows/apps/design/signature-experiences/typography)
- Use one font throughout the app UI. The default is Segoe UI Variable. — [Typography](https://learn.microsoft.com/en-us/windows/apps/design/signature-experiences/typography)
- Line length: "Keep to 50–60 letters per line for ease of reading." "Don't use fewer than 20 characters or more than 60 characters per line." — [Typography](https://learn.microsoft.com/en-us/windows/apps/design/signature-experiences/typography)
- When to use each style:
  - Title, Subtitle and Body go together "with 12epx spacing".
  - "When differentiating a title in a confined UI space, use Body Strong for the title without any additional spacing between text blocks."
  - "Use caption size for very confined spaces where text is needed, such as command buttons."
  - Multi-line lists use Body + Caption with 32 epx icons, and Body Strong for section headers.
  - Grid items with icons or person pictures use centre-aligned Caption.

  — [Content layout and spacing](https://learn.microsoft.com/en-us/windows/apps/design/basics/content-basics) (updated 2026-09-11)
- The window title in the title bar uses Caption style, in Segoe UI Variable (or Segoe UI as a fallback). — [Title bar](https://learn.microsoft.com/en-us/windows/apps/design/basics/titlebar-design)
- Segoe UI Variable covers English, European languages, Greek and Russian. Other scripts have their own recommended UI fonts: Yu Gothic UI (Japanese), Microsoft YaHei UI (Simplified Chinese), Microsoft JhengHei UI (Traditional Chinese), Malgun Gothic (Korean), Nirmala UI (South Asian), Leelawadee UI (Southeast Asian), and Segoe UI (Arabic, Armenian, Georgian, Hebrew). Selawik is the open-source, metric-compatible substitute for Segoe UI on other platforms. — [Typography](https://learn.microsoft.com/en-us/windows/apps/design/signature-experiences/typography)
- Font sizes are in epx and scale automatically, so you "shouldn't have to alter font sizes for different screens sizes or resolutions". — [Typography](https://learn.microsoft.com/en-us/windows/apps/design/signature-experiences/typography)
- Base rich-text style: `FontFamily` Segoe UI Variable, `FontSize` 14, `LineStackingStrategy` MaxHeight, `TextLineBounds` Full, `OpticalMarginAlignment` TrimSideBearings. — [XAML theme resources](https://learn.microsoft.com/en-us/windows/apps/develop/platform/xaml/xaml-theme-resources)

### Inferences
- In WPF, the optical axis is probably not automatic. WPF's text stack does not expose variable-font axes the way WinUI's DirectWrite path does. Using the named faces ("Segoe UI Variable Small", "Segoe UI Variable Text", "Segoe UI Variable Display") at the matching ramp sizes probably approximates the ramp. This needs verification in a WPF prototype.
- Line height maps to `LineHeight` with `LineStackingStrategy="BlockLineHeight"` in WPF. The usual Windows 11 default UI size is Body, 14 epx.

### Gaps
- **Internal contradiction on the Typography page:** the best-practice table says "Use ellipses in most cases". The later "Clipping and ellipses" section says "Clip text… Don't use ellipses to avoid visual clutter", except where containers are not well defined or a "see more" link exists. The page itself recommends `TextTrimming="Clip"` in its sample. Both statements are live on the same page as of 2026-07-14.
- The page gives no letter-spacing (tracking) values for the ramp.

---

## Spacing and layout: the 4 epx grid, gutters, control sizes and breakpoints

### Takeaway
Make every size, margin and position a multiple of 4 epx. Standard gaps are:

| Between | Gap (epx) |
| --- | --- |
| Buttons, button and flyout, control and its header | 8 |
| Control and label, content areas or cards | 12 |
| Surface edge and text | 16 |
| Control and expander chevron | 16 |
| Expander child indent | 48 |

The title bar is 32 epx tall, or 48 epx when it holds a search box or person picture. Touch targets are about 40×40 epx (7.5 mm); Compact density uses 32×32 epx. Breakpoints are Small up to 640 epx, Medium 641–1007 epx and Large 1008 epx and up.

### Cited Findings
- "The sizes, margins, and positions of UI elements should always be in multiples of 4 epx". Scaling plateaus are 100, 125, 150, 175, 200, 225, 250, 300, 350 and 400%. "The base unit is 4 because it scales to these plateaus as a whole number (for example, 4 × 125% = 5…)". "Text does not have this requirement". (updated 2026-07-14) — [Screen sizes and breakpoints](https://learn.microsoft.com/en-us/windows/apps/design/layout/screen-sizes-and-breakpoints-for-responsive-design)
- Breakpoints, measured on the app window rather than the screen:
  - Small: up to 640 px (TVs; 480×854, 540×960)
  - Medium: 641–1007 px (tablets; 960×540)
  - Large: 1008 px and up (PCs and laptops; 1024×640, 1366×768, 1920×1080)

  For mockups, "set the DPI to 72 and set the image dimensions to the effective resolution". — [Screen sizes and breakpoints](https://learn.microsoft.com/en-us/windows/apps/design/layout/screen-sizes-and-breakpoints-for-responsive-design)
- Spacing and gutters, with the rationale that they "semantically group an experience into separate components" and "map to our rounded corner logic":

  | Between | Gap (epx) |
  | --- | --- |
  | Buttons | 8 |
  | Buttons and flyouts | 8 |
  | Control and header | 8 |
  | Control and label | 12 |
  | Content areas | 12 |
  | Surface and edge text | 16 |

  — [Content layout and spacing](https://learn.microsoft.com/en-us/windows/apps/design/basics/content-basics) (updated 2026-09-11)
- Expander pattern: "Controls should be right-aligned with 16epx between the control and expander button". Controls inside an expander are indented 48 epx. — [Content layout and spacing](https://learn.microsoft.com/en-us/windows/apps/design/basics/content-basics)
- Title bar:
  - Height is 32 px; increase it to 48 px when a search box or person picture is in it.
  - The window icon is 16×16 px, placed 16 px from the left edge and vertically centred (8 px top and bottom margins in a 32 px bar).
  - The title sits 16 px from the icon or back button and uses the Caption style.
  - The back button glyph is E830 ChromeBack, 16×16, 16 px from the edge.
  - Caption glyphs: E921 Minimize, E922 Maximize, E923 Restore, E8BB Close. They have full-bleed backplates.
  - "All title bar elements should be semi-transparent when the window is inactive."
  - The default background is Mica.

  — [Title bar](https://learn.microsoft.com/en-us/windows/apps/design/basics/titlebar-design) (updated 2026-07-14)
- Touch target: "set your touch target size to 7.5mm square range (40x40 pixels on a 135 PPI display at a 1.0x scaling plateau)". Frequently pressed or high-consequence targets should be larger or better padded. (updated 2026-09-28) — [Targeting](https://learn.microsoft.com/en-us/windows/apps/develop/input/guidelines-for-targeting)
- Density: "Fluent Standard sizing aligns all items on the screen to a 40x40 effective pixels (epx) target". "Fluent Compact sizing… aligns UI elements to a 32x32 epx target". Compact is supported "beginning with WinUI 2.1". These come from the older Spacing page, as shown in search snippets of [learn.microsoft.com/…/design/style/spacing](https://learn.microsoft.com/en-us/windows/apps/design/style/spacing). That URL now redirects to the Content layout and spacing page, which no longer has the density section (see Gaps).
- Navigation back-button styles: `NavigationBackButtonNormalStyle` is 40×40 px and `NavigationBackButtonSmallStyle` is 30×30 px. — [XAML theme resources](https://learn.microsoft.com/en-us/windows/apps/develop/platform/xaml/xaml-theme-resources)
- Line length of 50–60 characters, which bounds readable text-column width. — [Typography](https://learn.microsoft.com/en-us/windows/apps/design/signature-experiences/typography)

### Inferences
- Default WinUI control heights (Button, TextBox, ComboBox) are commonly 32 epx in standard density. That value is in the WinUI control templates (`*_themeresources.xaml`), not in the design pages I fetched. Verify it against the WinUI source before hard-coding it.
- Settings-style pages: the Windows Settings app and the WinUI Gallery use a scrollable column of `SettingsCard` items stacked with `Spacing="4"` and a max content width of about 1000 epx. That description came from a search-engine summary of Community Toolkit material ([SettingsCard docs](https://learn.microsoft.com/en-us/dotnet/communitytoolkit/windows/settingscontrols/settingscard)), not from a fetched primary page. Treat it as unverified.

### Gaps
- **Recent doc change:** the en-us `design/style/spacing` URL now redirects to `design/basics/content-basics` (updated 2026-09-11). That page no longer carries the "Control size and density" section (Standard 40×40 / Compact 32×32). Localized copies show the same trimmed content, so this looks like a deliberate restructure. I found no current en-us page with control heights.
- I found no official Microsoft page-margin specification for settings-style pages (for example 36/56 epx side padding or a max width). Values circulating in the community, such as 56 epx left padding and 1000 epx max width, are not confirmed by a primary Microsoft design page I could fetch.
- No official "recommended content width" beyond the 50–60 character line length.

---

## Geometry: corner radius and strokes

### Takeaway
Corner radii:

| Radius | Applies to |
| --- | --- |
| 8 px | Top-level and overlay surfaces: windows, dialogs, flyouts, menus, TeachingTip |
| 4 px | In-page controls and bar shapes: Button, CheckBox, ComboBox, TextBox, ListView backplates, ProgressBar, ScrollBar, Slider, ToolTip |
| 0 | Where straight edges meet, on split or attached parts, and on snapped or maximized windows |

Every elevation level also carries a 1 px stroke (contour).

### Cited Findings
- Corner radius table (updated 2026-02-19) — [Geometry](https://learn.microsoft.com/en-us/windows/apps/design/signature-experiences/geometry):

  | Radius | Usage |
  | --- | --- |
  | 8px | Top-level containers such as app windows, flyouts and dialogs |
  | 4px | In-page elements such as buttons and list backplates |
  | 0px | Straight edges that intersect with other straight edges |
  | 0px | Window corners when windows are snapped or maximized |

- Specifics:
  - Standard always-visible controls (Button, CheckBox, ComboBox, TextBox, ListView) use **4px**.
  - Transient or overlay UI (ContentDialog, Flyout, MenuFlyout, TeachingTip) uses **8px**. "ToolTip is an exception, using 4px due to its small size."
  - Bars (ProgressBar, ScrollBar, Slider) use **4px**.

  — [Geometry](https://learn.microsoft.com/en-us/windows/apps/design/signature-experiences/geometry)
- Don't round where elements inside a container touch each other, such as the two halves of a SplitButton. Don't round the side of a flyout that is attached to its invoking UI, such as the AutoSuggest dropdown. — [Geometry](https://learn.microsoft.com/en-us/windows/apps/design/signature-experiences/geometry)
- Global overrides: `ControlCornerRadius` (default 4px) and `OverlayCornerRadius` (default 8px). Win32 windows can opt in to rounded corners through the DWM API. — [Geometry](https://learn.microsoft.com/en-us/windows/apps/design/signature-experiences/geometry); [Apply rounded corners in desktop apps](https://learn.microsoft.com/en-us/windows/apps/desktop/modernize/ui/apply-rounded-corners)
- Strokes: every elevation level (Window, Dialog, Flyout, Tooltip, Card, Control, Layer) is listed with "Stroke width: 1". — [Layering and elevation](https://learn.microsoft.com/en-us/windows/apps/design/signature-experiences/layering)
- Fluent 2: "Windows uses strokes instead of key shadows to outline an object." — [Fluent 2 – Elevation](https://fluent2.microsoft.design/elevation)
- Stroke colours from WinUI resources:

  | Token | Light | Dark |
  | --- | --- | --- |
  | ControlStrokeColorDefault | `#0F000000` | `#12FFFFFF` |
  | ControlStrokeColorSecondary | `#29000000` | `#18FFFFFF` |
  | CardStrokeColorDefault | `#0F000000` | `#19000000` |
  | SurfaceStrokeColorFlyout | `#0F000000` | `#33000000` |
  | DividerStrokeColorDefault | `#0F000000` | `#15FFFFFF` |
  | SurfaceStrokeColorDefault (both themes) | `#66757575` | `#66757575` |

  — [WinUI Common_themeresources_any.xaml (GitHub)](https://raw.githubusercontent.com/microsoft/microsoft-ui-xaml/main/controls/dev/CommonStyles/Common_themeresources_any.xaml)

### Inferences
- In WPF, a custom window chrome should drop its corner radius to 0 when `WindowState == Maximized`. On Windows 11, DWM rounds and un-rounds top-level windows itself when `DWMWA_WINDOW_CORNER_PREFERENCE` is set.
- Nested radii: an inner element inside an 8 px container uses 4 px. This is consistent with the "progressively rounded corners, nested elements" rationale, but no explicit formula is given.

### Gaps
- No explicit nested-radius formula (outer = inner + padding) is published on the Windows pages.

---

## Color and theming

### Takeaway
Windows has Light, Dark and HighContrast themes. Darker surfaces are less important and lighter or brighter surfaces are more important. Use accent colour sparingly, for important elements and interactive state; never put accent text on acrylic. Text has four levels: Primary, Secondary, Tertiary and Disabled. Dark text levels are white at about 100/77/53/36% opacity; light text levels are black at about 89/62/45/36%. Always honour the system accent colour and the user's contrast-theme colours.

### Cited Findings
- "Windows supports two color modes, or themes: light and dark." "In both light and dark color modes, darker colors indicate background surfaces of less importance. Important surfaces are highlighted with lighter and brighter colors." By default an app follows the user's theme. (updated 2026-07-14) — [Color](https://learn.microsoft.com/en-us/windows/apps/design/signature-experiences/color)
- Accent: "used to emphasize important elements in the user interface and to indicate the state of an interactive object or control. Accent color values are generated automatically and optimized for contrast in both light and dark modes. Accent colors are used sparingly…" — [Color](https://learn.microsoft.com/en-us/windows/apps/design/signature-experiences/color)
- Color principles:
  - "Use color meaningfully."
  - "Use color to indicate interactivity" (choose one colour for interactive elements).
  - "Color is personal."
  - "Color is cultural."
  - About 8% of men and 0.5% of women are red-green colourblind, so don't make red/green the sole differentiator.
  - Consider ambient lighting.

  — [Color](https://learn.microsoft.com/en-us/windows/apps/design/signature-experiences/color)
- `SystemAccentColor` "gets the color that the user has specified as the accent color in the Windows personalization settings". "It's a best practice to respect the user's color choices, especially for contrast theme settings." — [XAML theme resources](https://learn.microsoft.com/en-us/windows/apps/develop/platform/xaml/xaml-theme-resources)
- Theme dictionaries: define "Light", "Dark" and "HighContrast" explicitly rather than "Default". In HighContrast, use the `SystemColor*Color` resources; for example, `TextFillColorPrimaryBrush` maps to `SystemColorWindowTextColor`. — [XAML theme resources](https://learn.microsoft.com/en-us/windows/apps/develop/platform/xaml/xaml-theme-resources)
- `TextFillColorPrimary` runtime values: Light `#E4000000`, Dark `#FFFFFFFF`, HighContrast = the user's Text colour. — [XAML theme resources](https://learn.microsoft.com/en-us/windows/apps/develop/platform/xaml/xaml-theme-resources)
- **Token values** from the WinUI source ([Common_themeresources_any.xaml](https://raw.githubusercontent.com/microsoft/microsoft-ui-xaml/main/controls/dev/CommonStyles/Common_themeresources_any.xaml)). ARGB hex: the first byte is alpha.

  | Token | Light | Dark |
  | --- | --- | --- |
  | TextFillColorPrimary | `#E4000000` | `#FFFFFF` |
  | TextFillColorSecondary | `#9E000000` | `#C5FFFFFF` |
  | TextFillColorTertiary | `#72000000` | `#87FFFFFF` |
  | TextFillColorDisabled | `#5C000000` | `#5DFFFFFF` |
  | TextOnAccentFillColorPrimary | `#FFFFFF` | `#000000` |
  | AccentTextFillColorPrimary | SystemAccentColorDark2 | SystemAccentColorLight3 |
  | AccentFillColorDefault | SystemAccentColorDark1 | SystemAccentColorLight2 |
  | AccentFillColorSecondary / Tertiary | accent at 0.9 / 0.8 opacity | accent at 0.9 / 0.8 opacity |
  | ControlFillColorDefault | `#B3FFFFFF` | `#0FFFFFFF` |
  | ControlFillColorSecondary (hover) | `#80F9F9F9` | `#15FFFFFF` |
  | ControlFillColorTertiary (pressed) | `#4DF9F9F9` | `#08FFFFFF` |
  | ControlFillColorDisabled | `#4DF9F9F9` | `#0BFFFFFF` |
  | SubtleFillColorSecondary / Tertiary | `#09000000` / `#06000000` | `#0FFFFFFF` / `#0AFFFFFF` |
  | SolidBackgroundFillColorBase (Mica fallback) | `#F3F3F3` | `#202020` |
  | SolidBackgroundFillColorBaseAlt (Mica Alt fallback) | `#DADADA` | `#0A0A0A` |
  | SolidBackgroundFillColorSecondary | `#EEEEEE` | `#1C1C1C` |
  | SolidBackgroundFillColorTertiary | `#F9F9F9` | `#282828` |
  | SolidBackgroundFillColorQuarternary | `#FFFFFF` | `#2C2C2C` |
  | LayerFillColorDefault (content layer on Mica) | `#80FFFFFF` | `#4C3A3A3A` |
  | LayerFillColorAlt | `#FFFFFF` | `#0DFFFFFF` |
  | LayerOnMicaBaseAltFillColorDefault | `#B3FFFFFF` | `#733A3A3A` |
  | CardBackgroundFillColorDefault | `#B3FFFFFF` | `#0DFFFFFF` |
  | CardBackgroundFillColorSecondary | `#80F6F6F6` | `#08FFFFFF` |
  | SmokeFillColorDefault | `#4D000000` | `#4D000000` |
  | FocusStrokeColorOuter / Inner | `#E4000000` / `#B3FFFFFF` | `#FFFFFF` / `#B3000000` |
  | SystemFillColorCritical | `#C42B1C` | `#FF99A4` |
  | SystemFillColorSuccess | `#0F7B0F` | `#6CCB5F` |
  | SystemFillColorCaution | `#9D5D00` | `#FCE100` |
  | SystemFillColorAttention | SystemAccentColor | SystemAccentColorLight2 |

- Where *not* to use accent: "We don't recommend placing accent-colored text on your acrylic surfaces because these combinations are likely to not pass minimum contrast ratio requirements at the default 14px font size. Try to avoid placing hyperlinks over acrylic elements." — [Acrylic](https://learn.microsoft.com/en-us/windows/apps/design/style/acrylic) (updated 2026-10-02)
- High contrast: Mica and acrylic are replaced by the user's chosen background colour in High Contrast mode. Title bars should use the `SystemColors` class in high-contrast themes. — [Mica](https://learn.microsoft.com/en-us/windows/apps/design/style/mica); [Title bar](https://learn.microsoft.com/en-us/windows/apps/design/basics/titlebar-design)

### Inferences
- Accent shade choice: in light theme, accent fills use the darker accent (Dark1) and accent text uses Dark2. In dark theme, fills use Light2 and text uses Light3. Text on an accent fill is white in light theme and black in dark theme. A WPF app can read the shades from `Windows.UI.ViewManagement.UISettings.GetColorValue(UIColorType.AccentDark1…AccentLight3)`, which is a known API not fetched here.
- Most fills are semi-transparent overlays rather than opaque greys. That is why they look right over Mica. In WPF without Mica, compose them over `SolidBackgroundFillColorBase`.

### Gaps
- The Color page gives no numeric list of where accent must not be used, beyond the acrylic, hyperlink and "sparingly" guidance.
- I did not fetch the HighContrast dictionary mappings token by token.

---

## Materials and elevation

### Takeaway
- **Mica** is an opaque, wallpaper-tinted backdrop for the base layer of long-lived windows, visible through the title bar. **Mica Alt** is a stronger-tinted variant for tabbed title bars or extra hierarchy.
- **Acrylic** is frosted glass for transient, light-dismiss surfaces such as flyouts, menus and dropdowns.
- **Smoke** is a 30% black (`#4D000000`) scrim behind modal dialogs.
- Elevation values are Window/Dialog 128, Flyout 32, Tooltip 16, Card 8, Control 2 (1 when pressed) and Layer 1. Every level has a 1 px stroke.

### Cited Findings
- Two families: occluding materials (acrylic, Mica) "used as base layers beneath interactive UI controls", and transparent materials (smoke) "used to highlight immersive surfaces". (Materials page updated 2024-11-05) — [Materials](https://learn.microsoft.com/en-us/windows/apps/design/signature-experiences/materials)
- **Mica**: "an opaque, dynamic material that incorporates theme and desktop wallpaper to paint the background of long-lived windows such as apps and settings… only samples the desktop wallpaper once". It falls back to a neutral colour when the app is inactive. Apply it "as the base layer of your app, and prioritize visibility in the title bar area." (updated 2026-07-17) — [Mica](https://learn.microsoft.com/en-us/windows/apps/design/style/mica)
- **Mica Alt**: "stronger tinting of the user's desktop background color… especially when creating an app with a tabbed title bar". It requires Windows App SDK 1.1+ and Windows 11 build 22000+. — [Mica](https://learn.microsoft.com/en-us/windows/apps/design/style/mica)
- Mica falls back to a solid colour (`SolidBackgroundFillColorBase`, or `…BaseAlt` for Mica Alt) when:
  - transparency is turned off in Settings,
  - Battery Saver is on,
  - the hardware is low-end,
  - the window is inactive, or
  - the OS is older than build 22000.

  — [Mica](https://learn.microsoft.com/en-us/windows/apps/design/style/mica)
- Layering on Mica:
  - Content layer: `LayerFillColorDefaultBrush`, a low-opacity solid. It comes in a "Standard pattern" (one contiguous area) or a "Card pattern" (segmented cards).
  - With Mica Alt there is a commanding layer (`LayerOnMicaBaseAltFillColorDefaultBrush`) and a content layer (`LayerFillColorDefaultBrush`).
  - Do set layer backgrounds to transparent where Mica should show.
  - Don't apply the backdrop more than once.
  - Don't apply the backdrop to a UI element.

  — [Mica](https://learn.microsoft.com/en-us/windows/apps/design/style/mica)
- Two-layer app model: the **base** layer holds "app menus, commands, and navigation". The **content** layer "focuses the user on the app's central experience… may be one contiguous element, or separated into cards". — [Layering and elevation](https://learn.microsoft.com/en-us/windows/apps/design/signature-experiences/layering) (updated 2026-02-19)
- **Acrylic** (Materials page): "brighter and more translucent" in Windows 11 and "used only for transient, light-dismiss surfaces such as flyouts and context menus." — [Materials](https://learn.microsoft.com/en-us/windows/apps/design/signature-experiences/materials)
- **Acrylic** detail page (updated 2026-10-02):
  - *Background acrylic* shows the desktop and other windows. Use it for context menus, flyouts, non-modal popups and light-dismiss panes. MenuFlyout, AutoSuggestBox and ComboBox use it by default.
  - *In-app acrylic* is for supporting UI that overlaps content, such as NavigationView in Compact or Minimal mode. Vertical panes that only section off content should be opaque.
  - Do: use it on transient surfaces, and extend it to at least one window edge.
  - Don't: put desktop acrylic on large backgrounds, place acrylic panes side by side (visible seam), or put accent text on acrylic.
  - Don't layer multiple acrylic surfaces.
  - The recipe is background, blur, exclusion blend, colour/tint overlay and noise.
  - Acrylic falls back to a solid colour when transparency is off, Battery Saver is on, or the hardware is low-end. Background acrylic also goes solid when the window is inactive.
  - Acrylic is GPU-intensive.

  — [Acrylic](https://learn.microsoft.com/en-us/windows/apps/design/style/acrylic)
- **Smoke**: "dimming the surfaces beneath… used to signal blocking interaction below a modal UI such as a dialog". It is "not mode aware; it is always translucent black". The token is `SmokeFillColorDefault` = `#4D000000` (about 30% black) in both themes. — [Materials](https://learn.microsoft.com/en-us/windows/apps/design/signature-experiences/materials); [WinUI source](https://raw.githubusercontent.com/microsoft/microsoft-ui-xaml/main/controls/dev/CommonStyles/Common_themeresources_any.xaml)
- **Elevation values** (each with stroke width 1):

  | Surface | Elevation |
  | --- | --- |
  | Window | 128 |
  | Dialog | 128 |
  | Flyout | 32 |
  | Tooltip | 16 |
  | Card | 8 |
  | Control | 2 |
  | Layer | 1 |

  Control states: Rest 2, Hover 2, Pressed 1. "The intensity of the rendered shadow changes depending on the theme at parity of value." — [Layering and elevation](https://learn.microsoft.com/en-us/windows/apps/design/signature-experiences/layering)
- Shadow rationale: "The higher the elevation, the larger and softer the shadow becomes… Using shadows purposefully — rather than decoratively… Overusing shadows, however, can diminish their impact and create visual noise." Use ThemeShadow or DropShadow. — [Layering and elevation](https://learn.microsoft.com/en-us/windows/apps/design/signature-experiences/layering)
- Fluent 2 shadow formula (cross-platform): the low ramp (shadow 2/4/8/16) key shadow has blur = n and y-offset = n/2, at about 14% opacity in light theme and higher in dark. The high ramp (28, 64) uses 24% key opacity in light and 28% in dark. Ambient components are around 20%. "Windows uses strokes instead of key shadows to outline an object." The fetched page content was partially garbled, so check exact ambient values directly. — [Fluent 2 – Elevation](https://fluent2.microsoft.design/elevation)
- Cards: background `CardBackgroundFillColorDefault` (light `#B3FFFFFF`, dark `#0DFFFFFF`), stroke `CardStrokeColorDefault`, elevation 8. — [WinUI source](https://raw.githubusercontent.com/microsoft/microsoft-ui-xaml/main/controls/dev/CommonStyles/Common_themeresources_any.xaml); [Layering](https://learn.microsoft.com/en-us/windows/apps/design/signature-experiences/layering)

### Inferences
- A WPF app on Windows 11 can request Mica through `DwmSetWindowAttribute(DWMWA_SYSTEMBACKDROP_TYPE)` ([Apply Mica in Win32](https://learn.microsoft.com/en-us/windows/apps/desktop/modernize/ui/apply-mica-win32), not fetched here). It must then keep the window background transparent and paint content layers with the low-opacity layer brushes. Otherwise it should paint `SolidBackgroundFillColorBase`, which is the documented fallback anyway.
- For a small utility or tray-style app, Mica as the base layer with cards for groups matches the documented "Card pattern".

### Gaps
- **Tension in Microsoft's docs:** the Materials page says acrylic is "used only for transient, light-dismiss surfaces". The Acrylic page also endorses in-app acrylic for overlapping supporting panes. Treat transient surfaces as the primary use and overlapping panes as the secondary one.
- Exact pixel blur, offset and opacity values for Windows elevation 2/8/16/32/128 are not on the Windows pages. Only the abstract "elevation value" is given.

---

## Iconography

### Takeaway
Use Segoe Fluent Icons, which is built into Windows 11 and replaces the deprecated Segoe MDL2 Assets. The glyphs are monoline with a 1 epx stroke. Use only the sizes 16, 20, 24, 32, 40, 48 or 64 epx; a 16 epx font size is a 16×16 epx icon. Glyphs share a fixed width and origin, so they can be layered. They are not meant to sit inline within text.

### Cited Findings
- "All glyphs in Segoe Fluent Icons are drawn in a monoline style… created through a single stroke of 1 epx." The aesthetic principles are Minimal, Harmonious and Evolved. (updated 2026-07-14) — [Iconography](https://learn.microsoft.com/en-us/windows/apps/design/iconography/)
- "Each font glyph is designed so that the footprint of the icon area is a square em. An icon with a 16-epx font size is the equivalent of a 16x16-epx icon." — [Iconography](https://learn.microsoft.com/en-us/windows/apps/design/iconography/)
- Use system icons "for items like command bars, navigation, or status indicators". — [Iconography](https://learn.microsoft.com/en-us/windows/apps/design/iconography/)
- Modifiers: the base icon fills the footprint, and a modifier sits in one of the bottom quadrants. Layering two glyphs is recommended for a different state of the same icon, such as active or selected. — [Iconography](https://learn.microsoft.com/en-us/windows/apps/design/iconography/)
- Recommended sizes: **16, 20, 24, 32, 40, 48, 64**. Other sizes give less crisp results. All glyphs have an identical fixed width, consistent height and left origin, which enables layering and colourisation. Many glyphs have RTL-mirrored variants. Glyphs are in the Unicode PUA ranges E700–F8CC. Segoe MDL2 Assets is deprecated. The font ships with Windows 11; on Windows 10 it is a separate download that may lag behind. "Not intended for inline use with text." (updated 2026-09-03) — [Segoe Fluent Icons font](https://learn.microsoft.com/en-us/windows/apps/design/iconography/segoe-fluent-icons-font)
- In lists: multi-line lists use 32 epx icons; grid items with icons use a centred Caption label. — [Content layout and spacing](https://learn.microsoft.com/en-us/windows/apps/design/basics/content-basics)
- The title-bar app icon is 16×16 px. — [Title bar](https://learn.microsoft.com/en-us/windows/apps/design/basics/titlebar-design)

### Inferences
- The typical pairings are a 16 epx glyph beside 14 epx Body text in buttons and menus, and 20 epx in NavigationView items. These match observed WinUI defaults but are not stated on the pages fetched.
- Pairing icons with text labels: the docs say glyphs are not for inline text use, and the writing guidance favours clear labels. For an icon-only button, provide an `AutomationProperties.Name` and a tooltip (see Accessibility).

### Gaps
- No explicit Microsoft rule on *when* an icon must have a visible text label. That guidance sits in per-control pages (CommandBar, AppBarButton), which I did not fetch.

---

## Motion

### Takeaway
Motion is fast and direct. Standard durations are 83, 167, 250 and 333 ms. Entrances use cubic-bezier(0,0,0,1). Exits are 167 ms and always fade, and the bare-minimum transition is an 83 ms linear opacity fade. Prefer the stock control animations (page transitions, connected animations, animated icons) over custom ones.

### Cited Findings
- Motion principles (updated 2026-07-14) — [Motion](https://learn.microsoft.com/en-us/windows/apps/design/signature-experiences/motion):
  - **Connected**: elements that change position or size visually connect between states.
  - **Consistent**: surfaces that share an entry point invoke and dismiss the same way; for example, taskbar flyouts slide up and down.
  - **Responsive**: motion adapts to input, posture and orientation.
  - **Delightful**: "always brief and fleeting".
  - **Resourceful**: "Avoid custom animations where possible… If you can't use WinUI controls, mimic existing OS behaviors."
- **Animation properties table** — [Motion](https://learn.microsoft.com/en-us/windows/apps/design/signature-experiences/motion):

  | Purpose | Definition | Easing | Timing (ms) | Used for |
  | --- | --- | --- | --- | --- |
  | Direct Entrance | Fast – In | cubic-bezier(0,0,0,1) | 167, 250, 333 | Position, Scale, Rotation |
  | Existing Elements | Point to Point | cubic-bezier(0.55,0.55,0,1) | 167, 250, 333 | Position, Scale, Rotation |
  | Direct Exit | Fast – Out | cubic-bezier(0,0,0,1) | 167 | Position, Scale, Rotation ("ALWAYS combine with fade out") |
  | Gentle Exit | Soft – Out | cubic-bezier(1,0,1,1) | 167 | Position, Scale |
  | Bare Minimum | Fade – In + Out | Linear | 83 | Opacity |
  | Strong Entrance | Elastic In, 3 keyframes | KF1 (0.85,0,0,1) 167; KF2 (0.85,0,0.75,1) 167; KF3 (0.85,0,0,1) 333 | as listed per keyframe | Position, Scale |

- Recommended built-in motion: page transitions for page-to-page changes (in Settings, top-level pages slide up from the bottom and sub-pages slide left/right); connected animation for layer-to-layer changes within a page; AnimatedIcon (Lottie) for micro-interactions. — [Motion](https://learn.microsoft.com/en-us/windows/apps/design/signature-experiences/motion)
- The docs use still images by default "to improve accessibility and readability", with the animated version on click. — [Motion](https://learn.microsoft.com/en-us/windows/apps/design/signature-experiences/motion)

### Inferences
- In WPF, `KeySpline="0,0,0,1"` on spline keyframes approximates the Direct Entrance curve. Durations should come from the 83/167/250/333 ms set.
- Animations should respect the system "Animation effects" setting (SPI_GETCLIENTAREAANIMATION / `UISettings.AnimationsEnabled`). This is an accessibility best practice but is not stated on the Motion page itself.

### Gaps
- The Motion page does not state a reduced-motion requirement explicitly.
- It gives no timing for hover or pressed colour transitions. Stock WinUI brush transitions are commonly 83 ms, but I did not confirm that here.

---

## Writing style and voice for UI text

### Takeaway
Be warm and relaxed, ready to lend a hand, and crisp and clear. Use sentence case for all UI text and address the user as "you" (the app is "we", never "I"). Use active voice and contractions. Use no periods on buttons, labels or checkboxes; use periods for full sentences in tooltips, errors and dialogs. Error messages must not blame the user, should say what happened and what happens next, and should give a realistic fix.

### Cited Findings
- Three voice principles: **Be warm and relaxed** ("don't blame the user… Your app should take responsibility"), **Be ready to lend a hand** ("always provide a solution when there's a problem"), **Be crisp and clear** ("don't assume that users will read every word"). (updated 2026-06-27) — [Writing style](https://learn.microsoft.com/en-us/windows/apps/design/style/writing-style)
- "Lead with what's important". Do: "Select filters to add effects to your image." — [Writing style](https://learn.microsoft.com/en-us/windows/apps/design/style/writing-style)
- Active voice. Do: "Restart the app to see your changes." Don't: "The changes will be applied when the app is restarted." — [Writing style](https://learn.microsoft.com/en-us/windows/apps/design/style/writing-style)
- Addressing the user: "Always address the user as 'you.'" "Use 'we' to refer to your own perspective." "Don't use 'I' or 'me' to refer to the app's perspective." — [Writing style](https://learn.microsoft.com/en-us/windows/apps/design/style/writing-style)
- Abbreviations: define each on first use, and avoid them if the app is localized. Use contractions when they are natural. — [Writing style](https://learn.microsoft.com/en-us/windows/apps/design/style/writing-style)
- Periods: "Use periods to end full sentences in tooltips, error messages, and dialogs. Don't end text for buttons, radio buttons, labels, or checkboxes with a period." — [Writing style](https://learn.microsoft.com/en-us/windows/apps/design/style/writing-style)
- Capitalization: capitalize proper nouns and the start of every string. The Typography page adds: "Use sentence casing for all UI text, including titles." — [Writing style](https://learn.microsoft.com/en-us/windows/apps/design/style/writing-style); [Typography](https://learn.microsoft.com/en-us/windows/apps/design/signature-experiences/typography)
- Error messages: be conversational without jargon, say what went wrong and what will happen next, give a realistic solution, and cut extraneous information. Example: "You're not connected." followed by actionable bullet steps. Do: "We couldn't upload the picture. If this happens again, try restarting the app…" — [Writing style](https://learn.microsoft.com/en-us/windows/apps/design/style/writing-style)
- Dialogs: "call and response". "Make sure that your buttons are clear answers to the question posed by the title." — [Writing style](https://learn.microsoft.com/en-us/windows/apps/design/style/writing-style)
- Buttons: "The longest the text on a button should ever be is a couple short words". Use active verbs, for example "Install now" or "Share". — [Writing style](https://learn.microsoft.com/en-us/windows/apps/design/style/writing-style)
- The deeper reference is the [Microsoft Style Guide](https://learn.microsoft.com/en-us/style-guide/welcome/). — [Writing style](https://learn.microsoft.com/en-us/windows/apps/design/style/writing-style)

### Inferences
- Under these rules, a status line for a device that does not respond should read something like "We can't reach your monitor. Check that the cable is connected, then try again." rather than "Error: DDC/CI timeout".

### Gaps
- The Writing style page's ms.date is 2020-09-24 (content updated 2026-06-27). It has not been rewritten specifically for Windows 11, but nothing in it conflicts with Windows 11 guidance.

---

## Accessibility requirements

### Takeaway
Text contrast must be at least 4.5:1, designed into the default themes rather than left to high-contrast mode. Every pointer action needs a keyboard equivalent (Space/Enter), with a logical tab order and F6 to move between panes. Show focus visuals: a 2 px outer stroke plus a 1 px inner stroke, 1 px outside the control. Touch targets should be about 40×40 epx (7.5 mm). Image and icon-only elements need an `AutomationProperties.Name`. Respect text scaling (up to 225%) and contrast themes.

### Cited Findings
- "Visible text must have a minimum luminance contrast ratio of 4.5:1 against its background", aligned with WCAG G18. Logos, incidental text in inactive UI, and purely decorative text are exceptions. "Do not treat high-contrast mode as the primary mitigation for low readability." (updated 2026-08-26) — [Accessible text requirements](https://learn.microsoft.com/en-us/windows/apps/design/accessibility/accessible-text-requirements)
- Text scaling: `IsTextScaleFactorEnabled` defaults to true; disable it "only when necessary". `TextScaleFactor` ranges over [1, 2.25]. Smaller text scales more than larger text. Test with Magnifier, display scale, and the "Make text bigger" setting. — [Accessible text requirements](https://learn.microsoft.com/en-us/windows/apps/design/accessibility/accessible-text-requirements)
- Static text belongs in TextBlock, not a read-only TextBox, which misreports the Edit role. Don't put static text in tab order. Text inside images needs `AutomationProperties.Name`. — [Accessible text requirements](https://learn.microsoft.com/en-us/windows/apps/design/accessibility/accessible-text-requirements)
- Keyboard (updated 2026-09-28) — [Keyboard accessibility](https://learn.microsoft.com/en-us/windows/apps/design/accessibility/keyboard-accessibility):
  - "Keyboard accessibility should be treated as a primary interaction model."
  - Tab order follows declaration order unless `TabIndex` is set.
  - Disabled controls leave the tab order; use `IsTabStop=false` to exclude other elements.
  - Implement Space and Enter for invokable controls.
  - Implement **F6 / Shift+F6** to move between panes; this must be done explicitly.
  - Provide accelerators and access keys, and expose them through `AutomationProperties.AcceleratorKey` and `AccessKey`.
  - Wrap clickable images in a Button with an `AutomationProperties.Name` (example: "Open profile photo").
- Focus visuals: "The primary border is 2px thick, and runs around the outside of the secondary border. The secondary border is 1px thick". "The default margin is 1px away from the control bounds." They are configurable through `FocusVisualPrimaryThickness`, `FocusVisualSecondaryThickness`, `FocusVisualMargin` and `UseSystemFocusVisuals`, with app-wide brushes `SystemControlFocusVisualPrimaryBrush` and `SystemControlFocusVisualSecondaryBrush`. (updated 2026-09-28) — [Visual feedback](https://learn.microsoft.com/en-us/windows/apps/develop/input/guidelines-for-visualfeedback)
- Focus stroke colours: outer `#E4000000` (light) / `#FFFFFF` (dark); inner `#B3FFFFFF` (light) / `#B3000000` (dark). — [WinUI source](https://raw.githubusercontent.com/microsoft/microsoft-ui-xaml/main/controls/dev/CommonStyles/Common_themeresources_any.xaml)
- Touch targets: 7.5 mm, or 40×40 px at 135 PPI and 1.0× scale. Make frequently used and high-consequence targets larger or better padded. — [Targeting](https://learn.microsoft.com/en-us/windows/apps/develop/input/guidelines-for-targeting)
- Colourblindness: don't rely on red and green alone. — [Color](https://learn.microsoft.com/en-us/windows/apps/design/signature-experiences/color)
- Contrast themes: provide an explicit HighContrast theme dictionary that uses the `SystemColor*` resources, and respect the user's choices. — [XAML theme resources](https://learn.microsoft.com/en-us/windows/apps/develop/platform/xaml/xaml-theme-resources)

### Inferences
- Secondary text (`#9E000000` on `#F3F3F3`, or `#C5FFFFFF` on `#202020`) is designed to pass 4.5:1. Tertiary and disabled text may not; disabled UI is exempt as "inactive UI".
- In WPF, the equivalent of `AutomationProperties.Name` is the same attached property in `System.Windows.Automation`. Focus visuals are done with `FocusVisualStyle`. Replicating the 2 px + 1 px strokes requires a custom `FocusVisualStyle`, because the default WPF dotted rectangle does not match.

### Gaps
- No Windows design page states a non-text (UI component) contrast ratio, such as WCAG 1.4.11's 3:1 for control boundaries.
- A minimum compact-mode touch target is not stated on the current Targeting page.

---

## Recent Windows 11 changes (24H2 / 25H2) relevant to app styling

### Takeaway
Nothing in Microsoft's Windows 11 design foundations changed in substance for 24H2 or 25H2. The ramp, radii, spacing, materials and motion values are the same as in the 2021 originals, and most signature-experience pages keep ms.date 2021–2024 with only maintenance updates in 2026. The visible OS-level change is broader, more consistent dark mode in legacy system surfaces (File Explorer copy, move and delete dialogs, among others) in 25H2-era builds. The doc-side change is a restructure of the Spacing page.

### Cited Findings
- Page dates as fetched:

  | Page | ms.date | Updated |
  | --- | --- | --- |
  | Typography | 2021-06-24 | 2026-07-14 |
  | Geometry | 2021-06-24 | 2026-02-19 |
  | Layering | 2021-06-24 | 2026-02-19 |
  | Color | 2024-09-19 | 2026-07-14 |
  | Materials | 2024-09-16 | 2024-11-05 |
  | Motion | 2024-07-24 | — |
  | Mica | 2026-07-22 | — |
  | Acrylic | 2026-10-02 | — |
  | Design principles | 2025-12-11 | — |

  — pages linked above
- The Typography ramp now includes **Body Large (18/24)** and **Body Large Strong (18/24)**, which are absent from the original 2021 Windows 11 ramp. The XAML styles `BodyLargeTextBlockStyle` and `BodyLargeStrongTextBlockStyle` exist. — [Typography](https://learn.microsoft.com/en-us/windows/apps/design/signature-experiences/typography); [XAML theme resources](https://learn.microsoft.com/en-us/windows/apps/develop/platform/xaml/xaml-theme-resources)
- The `design/style/spacing` URL now redirects to "Content layout and spacing" (`design/basics/content-basics`, updated 2026-09-11). The Standard/Compact density section seen in older copies is gone. — [Content layout and spacing](https://learn.microsoft.com/en-us/windows/apps/design/basics/content-basics)
- 25H2-era builds extend dark mode to File Explorer copy, move and delete dialogs, progress bars and confirmation dialogs. — [Neowin](https://www.neowin.net/news/windows-11-25h2-finally-gets-long-requested-dark-mode-improvements-and-more-in-new-build/) (secondary press source)
- Accessibility pages (keyboard, targeting, visual feedback, accessible text) were substantially rewritten in August–September 2026, adding explicit F6 pane navigation and "keyboard as primary interaction model" language. — [Keyboard accessibility](https://learn.microsoft.com/en-us/windows/apps/design/accessibility/keyboard-accessibility)

### Inferences
- The Body Large styles were probably added to the docs between 2022 and 2024 (inferred from their absence in early Windows 11 material); I did not confirm the date. Body Large is the step between Body and Subtitle, and is useful for prominent values such as a large brightness readout.
- The 25H2 dark-mode work means a WPF app's own dialogs and message boxes will look more out of place if they stay light while the OS is dark. That raises the value of fully theming every window, including WPF `MessageBox` replacements.

### Gaps
- I found no Microsoft primary source (release notes or design blog) describing design-token or guideline changes specific to 24H2 or 25H2. The OS-change evidence is press coverage only.
- I did not verify whether WPF's built-in Fluent theme (.NET 9+) tracks these exact tokens. That is out of this note's scope and may be covered by another researcher.
