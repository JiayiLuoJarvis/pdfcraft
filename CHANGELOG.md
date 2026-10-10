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
- `Columns::gap`, `col_fixed`, `col_ratio`, and `col_auto` set track widths. Mixing Auto with Ratio fails with `Error::ColumnWidths { reason: "auto-with-ratio" }`.
- `List::start`, `List::marker_width`, and `List::nest_indent` set list numbering and the marker column.
- `Cell::ul` and `Cell::ol` run when the row is written, so they can borrow locals from the table closure.

### Changed

- `layout` and `text` are crate-private. Empty `layout/dump.rs` is gone.

### Added

- Paragraphs support mixed body and bold font runs plus a typed first-line indent.
- `FontStore` measures nominal glyph advances with skrifa 0.47.0. Missing cmap entries fail the document. The krilla adapter draws the same glyph ids through `draw_glyphs`.
- `Canvas` and `RecordingCanvas`, with an ordered `CanvasCommand` log so layout tests can assert coordinates without PDF bytes.
- Branded lengths (`Length` with `Finite` / `NonNegative` / `Positive`), page geometry (`Point`, `Size`, `Rect`, `Margins`), paint (`Color`, `Paint`, `Stroke`), and non-empty `Path` / `PathBuilder` in `geom`, plus construction `Error` variants. Package name is `quire-pdf`.
- Repository scaffold, module tree placeholders, and agent/contributor conventions.
