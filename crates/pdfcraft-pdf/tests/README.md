# `pdfcraft-pdf` integration tests

Layout assertions use `RecordingCanvas` coordinates (`pt_near`) and the
checked-in golden corpus in `tests/goldens/`. There is no pixel or
viewer-based regression: goldens are stable-text serializations of
`RecordedPage` logs, never compressed PDF bytes.

## Golden corpus

| Golden | Authoring test | Covers |
| --- | --- | --- |
| `text_flow.golden` | `golden_docs.rs::golden_doc_text_flow` | Multi-run paragraph (size/color/underline/tracking), rule, `Styled` inheritance, centered paragraph |
| `table_spans.golden` | `golden_docs.rs::golden_doc_table_spans` | Header/body grid, `col_span`, `row_span`, cell fill, fixed + auto tracks, grid border (font-free) |
| `bands_layers.golden` | `golden_docs.rs::golden_doc_bands_layers` | Header/footer bands with page-time runs (`PageNumber`/`PageCount`), band link, `behind`/`front` layers, rotated group, `PageWhen::Last`, two pages |

The serializer lives in `tests/golden/` (`snapshot.rs`, `sha256.rs`) and is
test-only infrastructure, never public API. Format: `pdfcraft-golden v1`,
`skrifa` version, doc line, one `font <slot> sha256=… upem=…` line per font,
one `image <slot> <w>x<h>` line per image, then `page <n> <w> <h>` plus one
line per op. Page-space lengths print with 3 decimals, glyph advances and
affine terms with 6; `-0.0` prints as zero. Font/image ids remap to dense
slots in order of first appearance. Identity affines always print in full.

## Bless workflow

```bash
UPDATE_GOLDEN=1 cargo test -p pdfcraft-pdf --features backend-krilla,test-utils golden_docs
git diff crates/pdfcraft-pdf/tests/goldens/   # review every line as behavior, then keep
cargo test -p pdfcraft-pdf --features backend-krilla,test-utils   # must be green without the flag
```

Rules:

- Bless locally and review the diff line by line: every added line must
  correspond to an intended op (a run, a rule, a grid line, a layer mark).
  Never bless a diff you cannot explain.
- Never set `UPDATE_GOLDEN=1` in CI. Without the flag a missing or
  different golden fails naming the file, prints a unified diff capped at
  40 lines, and writes the actual log to `target/golden/<name>.actual`
  (`CARGO_TARGET_DIR` aware).
- Golden names match `[A-Za-z0-9_-]{1,64}`; path traversal is rejected.
- Keep documents small and synthetic. No real customer PDFs, no large
  documents, no orgchart/post goldens here.

## Font pin

Glyph goldens (`text_flow`, `bands_layers`) are pinned to Ubuntu 24.04
`fonts-dejavu-core` `DejaVuSans` (see `BLESSED_FONT_SHA256` in
`golden_docs.rs`); the golden header records the font SHA-256 and units per
em so a font swap shows up as a header diff. Run them with:

```bash
PDFCRAFT_PDF_FONT=/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf cargo test -p pdfcraft-pdf --features backend-krilla,test-utils golden_docs
```

A different `PDFCRAFT_PDF_FONT` skips the glyph goldens instead of failing:
a swap needs a deliberate, reviewed re-bless, not a red gate. `table_spans`
is font-free and verifies everywhere with no font configured.
