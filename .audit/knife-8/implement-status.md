# Knife 8 implement status

Branch `feat/document-dsl` on top of `6dbf40c`. Nothing is committed yet; the parent owns the commit.

## Result

All seven gates pass. The run was on 2026-10-08.

| Gate | Result |
| --- | --- |
| `cargo fmt --all -- --check` | ok |
| `cargo clippy --all-targets --no-default-features -- -D warnings` | ok |
| `cargo clippy --all-targets --features backend-krilla -- -D warnings` | ok |
| `cargo test --no-default-features` | 111 lib + 17 integration |
| `cargo test --features backend-krilla` | 129 lib + 1 ignored, 18 integration |
| `RUSTDOCFLAGS='-D warnings' cargo doc --no-deps --no-default-features` | ok |
| `RUSTDOCFLAGS='-D warnings' cargo doc --no-deps --features backend-krilla` | ok |

The baseline at `6dbf40c` was 118 lib tests and 1 ignored with backend-krilla. Eleven lib tests are new. There are 18 new integration tests in `tests/document.rs`.

Mutation check. Swapping `behind` and `front` in `paint_pages` fails `layers_paint_behind_header_body_footer_front` and `layer_ignores_alignment_and_page_bounds`. The change was reverted afterwards.

## Diff

| File | Change |
| --- | --- |
| `src/layout/decorate.rs` | `BandContent<'a>`, `Decorations<'a>`, plus `behind`/`front` as `LayerContent<'a>` returning `LayerText`. `PageBands` gains `behind`/`front` as height-0 `PlacedFragment`s. All layers are measured before paint. A layer forces `Start` alignment (G9). New tests cover paint order, band columns on one line, a band column holding a table being rejected, a layer ignoring alignment and page bounds, and a missing glyph in a layer leaving an empty canvas. |
| `src/layout/paginate.rs` | Paint order is behind, header, body, footer, front. `Decorations<'_>`. |
| `src/layout/lower.rs` | `Columns` in `Band` is checked recursively with `Container::Band` (G1). Cells and list items still reject columns. |
| `src/layout/columns.rs` | `ColumnsNode::columns()` accessor for the band check. |
| `src/canvas.rs` | `pub(crate) fn replay(&RecordingCanvas, &mut dyn Canvas)` and a round-trip identity test (G2). |
| `src/document.rs` | `document`, `Document`, `Flow`, `Paragraph`, `Columns`, `Table`, `Cell`, `List`, `Numbering`, `Page`, `Band`, `Layer`, and `Rendered` (with `to_pdf` behind the feature). Crate tests cover a lazy `Text` node (G6) and four empty-canvas failures (G3). |
| `src/lib.rs` | Re-exports the 13 document names. No krilla types. |
| `src/backend/mod.rs` | Doc text only. It now names `to_pdf` as the production caller. |
| `tests/document.rs` | Public-API tests. |

## Choices made while implementing

- `Document::font` returns `Result<()>`, not `Result<FontId>`. `FontId` is `#[must_use]`, so `doc.font(bytes)?;` would warn at every call site. No public API takes a `FontId` yet.
- Header slots are inset by margins (synthesis default `full-bleed-slots` reversed). `Band` lowers to a `Columns` row: `[Fixed(margin.left)?, Ratio 1 left, Ratio 1 right, Fixed(margin.right)?]`, with gap 0. A zero margin drops its spacer column. A single remaining track lowers to a plain `Text`. With both slots set, each slot wraps at half the inner width.
- There is no center slot. The synthesis "Center still page-centered" refers to an API neither candidate defines, so none was added.
- Spans use `NonZeroU16` (G5). `first_builder_error_wins` therefore uses a one-column `columns` (`ColumnWidths { reason: "column-count" }`) and then an invalid table that is ignored.
- `Page` has `number`, `count`, `section_number`, `section_count`, `size`, and `margins`, and is `#[non_exhaustive]` (G4).
- Calling `header`/`footer`/`behind`/`front` again replaces the earlier closure. This is in the rustdoc and tested for `header` (G7).
- The table column count is the span sum of the first row (header first, otherwise body). An empty table fails with `ColumnWidths { reason: "column-count" }`.
- Tests use the literals `BASELINE = 13.812` and `LINE = 17.244`, measured from the fixture through the public API. The line height is ascent minus descent, which beats `12 * 1.2`.
- `backend/mod.rs` keeps `#![allow(dead_code)]`. `to_krilla_size` and `KrillaCanvas::new` are test-only, and gating them would also break `convert.rs` imports. That backend churn is outside this knife.

## Open items for the parent or user

1. `document.rs` is 774 lines, about 680 without tests. Candidate-1 suggested splitting past about 400. The synthesis module map lists one file, so it was not split.
2. `keep_together` and `page_break` in the root body: a `page_break` inside a body-level `keep_together` fails with `InvalidGeometry`, which is knife-7 behavior. `check_content` does not run on the root. A better error would need a DSL-side rule, which the design rules out.
3. Inherited from the synthesis: `p.bold`, style builders, `PageSize` enum, `image_at`, and band center slot are all deferred.
4. `Document::font` takes collection index 0 only. `.ttc` CJK collections would need an index.
