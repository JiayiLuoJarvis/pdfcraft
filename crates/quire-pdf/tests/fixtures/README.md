# Test fixtures

`quire-measure.ttf` is an OFL subset of Adobe Source Han Serif ExtraLight.
It covers printable ASCII, digits, U+2022, and 第 / 页 / 共.
Family name is QuireMeasure so it does not use Adobe's reserved name Source.
See `LICENSE-OFL.txt`.

Rebuild with `.audit/fonts-skrifa/subset-fixture.sh` plus a local rename step.
Do not vendor a full CJK font.

`quire-letter.ttf` is an OFL subset of Noto Sans SC Regular, family name QuireLetter.
The source file is `Sans/SubsetOTF/SC/NotoSansSC-Regular.otf` from
<https://github.com/notofonts/noto-cjk>.
It covers printable ASCII and the Simplified Chinese in `tests/letter_layout.rs`
(two body paragraphs, the mixed line, table headers and cells, and the footer
`第 {} 页 / 共 {} 页`).
Download the source into `/tmp` only.

Rebuild with fontTools 4.60 (`pyftsubset`) on `PATH`:

```bash
PYFTSUBSET=pyftsubset PYTHON=python3 .audit/fonts-skrifa/subset-letter.sh
```

The script subsets with `pyftsubset`, converts the CFF outlines to quadratic
TrueType, and renames the family to QuireLetter.
See `LICENSE-OFL.txt`.

`one-pixel.png` is a 1 by 1 opaque red PNG for image tests.

Integration tests load fixtures by path from `tests/`, not from `src/`.
