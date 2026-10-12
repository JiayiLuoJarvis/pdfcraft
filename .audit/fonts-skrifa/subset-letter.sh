#!/usr/bin/env bash
# Rebuild tests/fixtures/quire-letter.ttf from Noto Sans SC Regular (OFL).
# The source download stays in /tmp. Do not commit that file.
#
#   PYFTSUBSET=/path/to/pyftsubset PYTHON=/path/to/python .audit/fonts-skrifa/subset-letter.sh
#
# fontTools 4.60 provides pyftsubset. The quadratic conversion below is the
# fontTools Snippets/otf2ttf.py routine (MIT), inlined so a reviewer does not
# need a second download.
set -euo pipefail

here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../.." && pwd)
out="${1:-$root/tests/fixtures/quire-letter.ttf}"
src="${LETTER_FONT_SRC:-/tmp/NotoSansSC-Regular.otf}"
url="https://raw.githubusercontent.com/notofonts/noto-cjk/main/Sans/SubsetOTF/SC/NotoSansSC-Regular.otf"
pyftsubset="${PYFTSUBSET:-pyftsubset}"
python="${PYTHON:-python3}"

if [[ ! -f "$src" ]]; then
    echo "downloading Noto Sans SC Regular to $src" >&2
    curl -fL --retry 3 --max-time 180 -o "$src.partial" "$url"
    mv "$src.partial" "$src"
fi

text=$("$python" - <<'PY'
parts = [
    "".join(chr(c) for c in range(0x20, 0x7F)),
    "敬启者：兹就贵司与本公司于二零二六年十月所签合同事项，函请确认下列款项与交货日期。",
    "请于收函后十日内书面回复。谢谢合作。",
    "Contract C-18 / 合同编号 C-18",
    "项目",
    "金额",
    "交货日",
    "设备",
    "服务",
    "第 {} 页 / 共 {} 页",
]
print("".join(parts), end="")
PY
)

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
printf '%s' "$text" >"$work/text.txt"

"$pyftsubset" "$src" \
    --text="$text" \
    --layout-features='' \
    --no-hinting \
    --desubroutinize \
    --notdef-glyph \
    --notdef-outline \
    --recommended-glyphs \
    --output-file="$work/subset.otf"

"$python" - "$work/subset.otf" "$work/text.txt" "$out" <<'PY'
import sys

from fontTools.pens.cu2quPen import Cu2QuPen
from fontTools.pens.ttGlyphPen import TTGlyphPen
from fontTools.ttLib import TTFont, newTable

# fontTools Snippets/otf2ttf.py (MIT). Max error is 1 font unit.
MAX_ERR = 1.0
POST_FORMAT = 2.0


def glyphs_to_quadratic(glyphs, max_err=MAX_ERR, reverse_direction=True):
    quad = {}
    for gname, glyph in glyphs.items():
        tt_pen = TTGlyphPen(glyphs)
        glyph.draw(Cu2QuPen(tt_pen, max_err, reverse_direction=reverse_direction))
        quad[gname] = tt_pen.glyph()
    return quad


def otf_to_ttf(tt_font):
    if tt_font.sfntVersion != "OTTO" or "CFF " not in tt_font:
        raise SystemExit("subset is not a CFF font")
    glyph_order = tt_font.getGlyphOrder()
    tt_font["loca"] = newTable("loca")
    glyf = newTable("glyf")
    tt_font["glyf"] = glyf
    glyf.glyphOrder = glyph_order
    glyf.glyphs = glyphs_to_quadratic(tt_font.getGlyphSet())
    del tt_font["CFF "]
    glyf.compile(tt_font)
    hmtx = tt_font["hmtx"]
    for glyph_name, glyph in glyf.glyphs.items():
        if hasattr(glyph, "xMin"):
            hmtx[glyph_name] = (hmtx[glyph_name][0], glyph.xMin)
    maxp = newTable("maxp")
    tt_font["maxp"] = maxp
    maxp.tableVersion = 0x00010000
    maxp.maxZones = 1
    maxp.maxTwilightPoints = 0
    maxp.maxStorage = 0
    maxp.maxFunctionDefs = 0
    maxp.maxInstructionDefs = 0
    maxp.maxStackElements = 0
    maxp.maxSizeOfInstructions = 0
    maxp.maxComponentElements = max(
        len(g.components if hasattr(g, "components") else []) for g in glyf.glyphs.values()
    )
    maxp.compile(tt_font)
    post = tt_font["post"]
    post.formatType = POST_FORMAT
    post.extraNames = []
    post.mapping = {}
    post.glyphOrder = glyph_order
    try:
        post.compile(tt_font)
    except OverflowError:
        post.formatType = 3
    tt_font.sfntVersion = "\000\001\000\000"


def rename(tt_font):
    family = "QuireLetter"
    subfamily = "Regular"
    full = "QuireLetter Regular"
    ps = "QuireLetter-Regular"
    unique = "QuireLetter-Regular; subset of Noto Sans SC"
    mapping = {1: family, 2: subfamily, 3: unique, 4: full, 6: ps, 16: family, 17: subfamily}
    name = tt_font["name"]
    for rec in name.names:
        if rec.nameID in mapping:
            rec.string = mapping[rec.nameID]
    for name_id, value in mapping.items():
        name.setName(value, name_id, 3, 1, 0x409)
    banned = ("Noto", "Source")
    for rec in name.names:
        if rec.nameID not in (1, 4, 6, 16):
            continue
        text = rec.toUnicode()
        if any(word in text for word in banned):
            raise SystemExit(f"name id {rec.nameID} still contains a reserved family word: {text}")


def main():
    src, text_path, dest = sys.argv[1:]
    text = open(text_path, encoding="utf-8").read()
    font = TTFont(src)
    otf_to_ttf(font)
    rename(font)
    cmap = font.getBestCmap()
    missing = [ch for ch in text if ord(ch) not in cmap]
    if missing:
        raise SystemExit("cmap missing: " + "".join(missing))
    font.save(dest)
    print(f"glyphs {font['maxp'].numGlyphs} scalars {len(set(text))}")


if __name__ == "__main__":
    main()
PY

ls -l "$out"
