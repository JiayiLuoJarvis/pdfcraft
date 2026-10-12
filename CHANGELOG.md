# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html)
while remaining `0.x`.

## [Unreleased]

### Added

- Header and footer slots span the full page width. `Band::inset` is the only way to pull them in from the page edges.
- `Paragraph::center` and `Paragraph::end` set line alignment. The default stays start.
- `Table::tracks`, `Table::cell_padding`, and `Table::border` reach the layout engine. Defaults stay equal-ratio columns with no stroke.
- `Columns::gap`, `col_fixed`, `col_ratio`, and `col_auto` set track widths. Mixing Auto with Ratio fails with `ErrorKind::ColumnWidths { reason: ColumnWidthsReason::AutoWithRatio }`.
- `List::start`, `List::marker_width`, and `List::nest_indent` set list numbering and the marker column.
- `Cell::ul` and `Cell::ol` run when the row is written, so they can borrow locals from the table closure.

### Changed

- **Breaking.** `Result<T>` is now `Result<T, Errors>`. `Errors` keeps one to 64 failures in discovery order (`first`, `as_slice`, `iter`, `truncated`). A single untruncated item displays like `Error`. Otherwise the items are numbered, and a truncated list ends with `further errors omitted`. Equality compares items only. `From<Error> for Errors`. Any error fails the document before paint. `Document::section` returns `()`. Geometry, canvas, `Document::font`, `bold_font`, and `load_image` still return one `Error`. `to_pdf` lifts a backend error into `Errors`.
- **Breaking.** `pt_near` and `PT_ABS_TOLERANCE` are no longer on the default crate root. Enable feature `test-utils` to import them. The empty public `limits` module is removed until real limits land.
- **Breaking.** `Error` is now a struct: `kind: ErrorKind` says what failed and `at: Option<Location>` says where (section index, page number, page region, table cell). Match on `error.kind` where you matched on `Error` variants before. `Error`, `ErrorKind`, `Location`, `Region`, `CellIndex`, `GeometryKind`, `ColumnWidthsReason`, and `CellSpanReason` are exported from the crate root.
- `ErrorKind::InvalidGeometry` carries a `GeometryKind` with the rejected values. `ErrorKind::InvalidCellSpan` carries a `CellSpanReason`, and the slot is in `at.cell`. `ErrorKind::ColumnWidths` carries a `ColumnWidthsReason` instead of a string. `ErrorKind::UnsplittableTallerThanFrame` carries the measured `height` and the frame `avail`. `ErrorKind::Backend` is a struct variant with `message`.
- `ErrorKind::DecorationTallerThanMargin.height` is the measured band content height. It was the page height when the content was taller than the page.
- Bold text without a registered bold face fails with `ErrorKind::BoldFaceRequired` instead of `UnknownFont { id: 1 }`.
- `ErrorKind::MissingGlyph.font` is the raw `u32` font id.
- Font parse failures and PDF serialization failures keep the skrifa or krilla error as `Error::source()`. Equality compares `kind` and `at` only.
- `UnsupportedCanvasOperation` is removed. Nothing produced it.
- `layout` and `text` are crate-private. Empty `layout/dump.rs` is gone.
- **Breaking.** `Rendered::to_pdf()` is now the free function `quire_pdf::to_pdf(rendered)`, still behind feature `backend-krilla`. `document` lives only at the crate root; the `quire_pdf::document::document` path is gone.

### Added

- Paragraphs support mixed body and bold font runs plus a typed first-line indent.
- `FontStore` measures nominal glyph advances with skrifa 0.47.0. Missing cmap entries fail the document. The krilla adapter draws the same glyph ids through `draw_glyphs`.
- `Canvas` and `RecordingCanvas`, with an ordered `CanvasCommand` log so layout tests can assert coordinates without PDF bytes.
- Branded lengths (`Length` with `Finite` / `NonNegative` / `Positive`), page geometry (`Point`, `Size`, `Rect`, `Margins`), paint (`Color`, `Paint`, `Stroke`), and non-empty `Path` / `PathBuilder` in `geom`, plus construction `Error` variants. Package name is `quire-pdf`.
- Repository scaffold, module tree placeholders, and agent/contributor conventions.
