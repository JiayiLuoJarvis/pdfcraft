# Knife 14 implement status

**Branch.** `feat/inline-blank`
**Baseline.** `feat/style-builders` at `5deb0df`
**Date.** 2026-10-10

## Commits

- `27a6d90` adds `Paragraph::blank`, `blank_em`, and `blank_to_end`, and `Error::RunAfterFillBlank`.
- `15893e8` adds `break_spans` with blank atoms. `break_lines` stays the pure-text facade.
- `a048044` places `LinePiece` glyphs and strokes. Line height stays the body line height.
- `5b5eddf` pins stroke endpoints, the glyph after a middle blank, unchanged line height, the assemble error before any page, and `UnbreakableWiderThanFrame` with `avail` near `W'`.

## Gates

Run on `5b5eddf` before the status commit. Each command exited 0.

- `cargo fmt --all`
- `cargo fmt --all -- --check`
- `cargo clippy --all-targets -- -D warnings`
- `cargo test`. 183 library tests passed, 1 ignored. 49 tests in `tests/document.rs` passed. 0 doctests.
- `cargo test --features backend-krilla`. Same counts. `backend-krilla` is already a default feature, so this command runs the same suite.

`inline_blank_writes_a_pdf` asserts the bytes start with `%PDF`.

## Behavior pinned

- A middle blank of 30 pt moves the next glyph by that gap. The stroke ends on those x positions. Thickness 2 pt and a 4 pt baseline offset are the painted values.
- `A` plus a blank plus a newline plus `B` puts `B` on the same baseline as `A\nB` with no blank.
- `blank_to_end` after `A` on a 120 pt frame strokes to x = 120. Default thickness is 1 pt and the default offset is 1 pt below the baseline.
- `blank_em(2)` strokes 24 pt at the default 12 pt font, with only a body font registered.
- A zero-width blank and `blank_em(0)` draw no stroke and do not move the next glyph.
- A list fill starts at x = 18, the marker column, and ends at the page edge.
- A second-column fill is shifted by the column origin. The stroke ends at the page edge.
- A page that fits one line keeps the stroke on page 1. Page 2 draws `B` at x = 0 and draws no stroke.
- A fill after `text("x")`, `bold("x")`, or another blank returns `RunAfterFillBlank`. The recording has no commands.
- A 80 pt blank on a 100 pt page with a 40 pt indent returns `UnbreakableWiderThanFrame` with width near 80 and avail near 60. The recording has no commands.
- A centered fill line starts the glyph at the frame x. The stroke ends at the frame width.

## Deviations

`blank_em` is not also checked at 16 pt. No public setter changes the paragraph font size. The 12 pt check is the stroke length `2 * 12`.

Paragraph alignment is not on the public DSL. The center-plus-fill check sets `Style.align` inside the crate.

`27a6d90` stores blanks, and painting one on that commit returns `InvalidGeometry`. `a048044` is the first commit that places them.

Cell, band, and layer blanks are not implemented.
