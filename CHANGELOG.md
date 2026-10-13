# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html)
while remaining `0.x`.

## [Unreleased]

### Documentation

- `docs/quire-orgchart-spec.md` is the behavior contract for equity charts.

- `quire-pdf` ships a self-contained crates.io-style README, `docs/guide.md` usage guide, crate/module rustdoc, dual license files in the package, and `package.metadata.docs.rs`. Caller docs do not require reading internal proposal notes.

### Added

- `quire-orgchart` checks an equity chart with `validate_request` and builds a PDF info title with `document_title`. `top_n` of `None` or `Some(0)` leaves the request unchanged. A larger `top_n` folds extra shareholder children into one `其他股东(k家)` node. This step does not check the folded node count against `max_nodes`, and it does not draw a PDF.
- `quire-orgchart` builds in-box lines and page-chrome strings. A ratio stays outside the box. Shareholder and subject lines are the name and the legal person. Investment lines are the name, paid-in amount, location, and approval date. Page titles use `股权架构图`, `对外投资`, `续页`, and `附件`. The footer keeps `頁`. This step does not measure text or draw a PDF.
- `quire-orgchart` measures node boxes from [`FontBytes`] with skrifa `0.48.0`. A `ttcf` collection is `FONT_ERROR`. A missing glyph is `MISSING_GLYPH`. A box taller than the portrait limit is `NODE_TOO_TALL`. This step does not draw a PDF.

- Repository is a Cargo workspace: `crates/quire-pdf` (existing layout library), plus empty shells `quire-forms` and `quire-orgchart`. Boundaries are in `docs/workspace-crates-spec.md`. Callers depend on the crate they need; forms must not depend on `quire-pdf`.
- `Ratio::new` builds a column weight. A weight that is not finite, or is not above zero, fails with `ErrorKind::ColumnWidths` before a table or a column set is assembled.
- `tests/fixtures/quire-letter.ttf` is an OFL subset of Noto Sans SC Regular with family name QuireLetter. `tests/letter_layout.rs` lays out a confirmation letter through `layout_spec` and checks the recorded canvas. `to_pdf` only checks that the bytes start with `%PDF`.
- `Limits` caps font bytes, image bytes, image pixels, image edge, pages, blocks, table cells, and paragraph bytes. `DocumentSpec::limits` supplies the caps. Exceeding a cap fails the document with `ErrorKind::LimitExceeded`. Setters reject 0 and any value above four times the default.
- Header and footer slots span the full page width. `Band::inset` is the only way to pull them in from the page edges.
- `Paragraph::center` and `Paragraph::end` set line alignment. The default stays start.
- `Table::tracks`, `Table::cell_padding`, and `Table::border` reach the layout engine. Defaults stay equal-ratio columns with no stroke.
- `Columns::gap`, `col_fixed`, `col_ratio`, and `col_auto` set track widths. Mixing Auto with Ratio fails with `ErrorKind::ColumnWidths { reason: ColumnWidthsReason::AutoWithRatio }`.
- `List::start`, `List::marker_width`, and `List::nest_indent` set list numbering and the marker column.
- `Cell::ul` and `Cell::ol` run when the row is written, so they can borrow locals from the table closure.
- `Cell::empty` and `Cell::add` build a cell from the same values the body adds: `Paragraph`, `HLine`, `PlacedImage`, `Spacer`, and `ListBlock` (sealed `IntoCellItem`). A cell may hold several children, stacked top to bottom; it is still one row-high slot that does not split. An empty cell is a zero-height slot. `Table` and `Columns` cannot be added to a cell.
- `Spacer` and `ListBlock` are public values. `FlowWriter::spacer`, `ul`, and `ol` now add them, and `FlowWriter::add` accepts them. Behavior of those sugars is unchanged.
- The `Cell::paragraph` / `ul` / `ol` / `image` / `hline` / `spacer` sugars keep their signatures and are now `Cell::empty().add(…)` of the shared value. `Cell::new` and string cells still use the table's text style unchanged.

### Changed

- Contract. Callers assemble a document as values. `DocumentSpec` holds the sections and blocks. Headers and footers are `BandSpec`. `behind` and `front` are `LayerMark` lists. `layout_spec` lays that value out. The library substitutes page numbers into `TextRun` when it paints a page. `PageWhen` chooses the pages for a `LayerMark`. A style scope is `Block::Styled`. Callers do not pass closures for the body, the header, the footer, `behind`, or `front`.
- **Breaking.** `document()`, `FlowWriter`, and the public builders that took closures are removed. Build a `DocumentSpec` and call `layout_spec`. Page numbers, page counts, section counters, and last-page marks are `TextRun` and `PageWhen` values. `Band`, `Layer`, `Flow`, `Section`, `IntoFlowItem`, and `ListBlock` are no longer exported. `Track`, `Ratio`, `Numbering`, and `Rendered` stay at the crate root. Layout itself goes through `layout_spec`.
- **Breaking.** `Document`, `Image`, `Document::new`, `Document::set_limits`, and `Document::load_image` are removed. The `document` module is gone. Font bytes and encoded images live on `Resources`. `Resources::add_image` returns an `ImageRef`. Caps are the `DocumentSpec::limits` field. `layout_spec` builds layout nodes directly. A damaged PNG fails while `layout_spec` registers images, with `ErrorKind::Image`. JPEG, GIF, and WebP pixel errors can still fail in `to_pdf` with `ErrorKind::Backend` and no page.
- **Breaking.** `Track::Fixed` takes `Length<Positive>`. `Track::Ratio` takes `Ratio`. `Border` is no longer exported. Call `Table::border`. `DocumentInfo` is no longer re-exported from the crate root. It remains `quire_pdf::canvas::DocumentInfo` for `Canvas::set_document_info`.
- **Breaking.** `ErrorKind::UnbreakableWiderThanFrame` carries `text: Option<String>`. `ErrorKind::UnexpectedLineBreakInSingleLine` carries `text: String`. `MissingGlyph`'s display includes the scalar.
- **Breaking.** A font or an image over its byte cap is `ErrorKind::LimitExceeded` with `LimitKind::FontBytes` or `LimitKind::ImageBytes`. Those failures were `ErrorKind::Font` and `ErrorKind::Image` strings.
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
- **Breaking.** `Rendered::to_pdf()` is now the free function `quire_pdf::to_pdf(rendered)`, still behind feature `backend-krilla`.

### Added

- Paragraphs support mixed body and bold font runs plus a typed first-line indent.
- `FontStore` measures nominal glyph advances with skrifa 0.47.0. Missing cmap entries fail the document. The krilla adapter draws the same glyph ids through `draw_glyphs`.
- `Canvas` and `RecordingCanvas`, with an ordered `CanvasCommand` log so layout tests can assert coordinates without PDF bytes.
- Branded lengths (`Length` with `Finite` / `NonNegative` / `Positive`), page geometry (`Point`, `Size`, `Rect`, `Margins`), paint (`Color`, `Paint`, `Stroke`), and non-empty `Path` / `PathBuilder` in `geom`, plus construction `Error` variants. Package name is `quire-pdf`.
- Repository scaffold, module tree placeholders, and agent/contributor conventions.

### Performance

- Root pagination and stack splitting dequeue with `VecDeque` instead of `Vec::remove(0)`. On a 5000-paragraph release baseline, layout fell from about 135 ms to about 116 ms on the same host.

### Fixed

- `keep_together` and a table cell count paragraph spacing between their children. Two lines that fit in the frame, with spacing between them that does not, fail with `ErrorKind::UnsplittableTallerThanFrame`. Spacing after the last child is still outside the group. Ordinary pagination still drops spacing that does not fit, and does not push the next block to the next page for that spacing alone.
- A PNG whose header is valid and whose pixel data is not fails while `layout_spec` registers that image, with `ErrorKind::Image`. JPEG, GIF, and WebP pixel errors can still fail in `to_pdf` with `ErrorKind::Backend` and no page.
- `Layer::text_at` and a custom list marker fail with `ErrorKind::UnexpectedLineBreakInSingleLine` when the string contains a forced line break. Nothing is drawn. Header and footer paragraphs still break on that line break.
- An `ImageRef` from a different `Resources` value is `ErrorKind::UnknownImage`. An image added to the same `Resources` paints, including inside a cell.
- A custom marker on an ordered list no longer advances the counter. Only an automatic label consumes a number. The last automatic label in `examples/hello.rs` is `d.`. That item was `e.` when the custom marker still consumed a number. `GeometryKind::MarkerOutOfRange` carries the next automatic ordinal. LowerAlpha starting at 26, with an automatic item, a custom marker, and another automatic item, fails with number 27.
