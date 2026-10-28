# Regenerates crates/tvc-app/ui/fonts/FluentSystemIcons-Regular.ttf: the icon font cut down to the glyphs the app
# uses, so the release exe stays small. The full font stays in ui/fonts/source/ (not embedded by the build).
#
#   python -I scripts/subset-icon-font.py
#
# Needs fontTools (pip install fonttools). Keeps every glyph written as \u{XXXX} in crates/tvc-app/ui/**/*.slint,
# plus EXTRA below. Run it again after adding an icon.
import pathlib
import re

from fontTools import subset

# Glyphs the Rust code passes to the UI at runtime (not visible in the .slint files).
EXTRA = """
e1f8 f87e f2be f831 f33c e685 edd0 f4f4 f1a0 f2f6 e99a f35a f60a f3f2 f295 f191
f299 f86a f4a4 f8c1 f124 f690 f32c f36a f3de f2b1 f2a4 f2b7 f588 f6aa f4b9 f15c
"""

root = pathlib.Path(__file__).resolve().parent.parent
ui = root / "crates" / "tvc-app" / "ui"
source = ui / "fonts" / "source" / "FluentSystemIcons-Regular.ttf"
target = ui / "fonts" / "FluentSystemIcons-Regular.ttf"

codepoints = {int(c, 16) for c in EXTRA.split()}
for path in ui.rglob("*.slint"):
    codepoints |= {int(c, 16) for c in re.findall(r"\\u\{([0-9a-fA-F]+)\}", path.read_text(encoding="utf-8"))}

options = subset.Options()
options.layout_features = []  # the icons are addressed by code point; the ligature table isn't needed
options.name_IDs = ["*"]  # keep the family name "FluentSystemIcons-Regular" that theme.slint asks for
options.name_languages = ["*"]
options.notdef_outline = True

font = subset.load_font(str(source), options)
subsetter = subset.Subsetter(options)
subsetter.populate(unicodes=codepoints)
subsetter.subset(font)
missing = sorted(c for c in codepoints if c not in font.getBestCmap())
subset.save_font(font, str(target), options)

print(f"{len(codepoints)} code points, {source.stat().st_size:,} -> {target.stat().st_size:,} bytes")
if missing:
    raise SystemExit("not in the font: " + " ".join(f"{c:x}" for c in missing))
