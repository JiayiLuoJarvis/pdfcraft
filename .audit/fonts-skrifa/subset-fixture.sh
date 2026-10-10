#!/usr/bin/env bash
# Rebuild tests/fixtures/source-han-serif-quire.ttf from a full Source Han Serif TTF.
# The committed fixture is the 13K subset. Do not vendor the 30M source.
set -euo pipefail
src="${1:?usage: subset-fixture.sh /path/to/SourceHanSerif*.ttf}"
out="${2:-tests/fixtures/source-han-serif-quire.ttf}"
pyftsubset="${PYFTSUBSET:-pyftsubset}"
text=$(python3 -c 'print("".join(chr(c) for c in range(0x20, 0x7F)) + "第页共\u2022")')
"$pyftsubset" "$src" \
  --text="$text" \
  --layout-features='' \
  --no-hinting \
  --desubroutinize \
  --glyph-names \
  --notdef-glyph \
  --notdef-outline \
  --recommended-glyphs \
  --output-file="$out"
ls -l "$out"
