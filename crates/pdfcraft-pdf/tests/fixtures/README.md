# Test fixtures

Font files are **not** stored in this repository. Integration and unit tests that
need a face read:

| Variable | Used for |
| --- | --- |
| `PDFCRAFT_PDF_FONT` | General layout / measure tests (Latin is enough for most) |
| `PDFCRAFT_CJK_FONT` | Letter layout and samples that need Chinese glyphs |

If unset, those tests return early (skip). Examples require the same variables.

`one-pixel.png` is a 1 by 1 opaque red PNG for image tests.
`bad-idat.png` is a corrupt PNG for decode-error paths.

`LICENSE-OFL.txt` records the OFL terms that applied to the former subset
fixtures (`quire-measure` / `quire-letter`) when they lived in-tree. Rebuild
subsets locally with `.audit/fonts-skrifa/subset-fixture.sh` and
`subset-letter.sh` if you want pinned glyph metrics again.
