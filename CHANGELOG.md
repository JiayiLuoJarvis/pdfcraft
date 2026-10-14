# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html)
while remaining `0.x`.

## [Unreleased]

### Documentation

- Confirmed `pdfcraft-post` in the proposal (§11), workspace crate map, and `AGENTS.md`. The crate post-processes existing PDF bytes: `SetMetadata`, `Encrypt` (presets `READ_ONLY`, `READ_ONLY_PRINTABLE`, `FULL_PERMISSIONS`), `StampAndEncrypt`, `merge_pdfs` (`preserve_outlines`), and `Pipeline`. Watermark and input limits are deferred. HTTP mapping and finalize orchestration stay out. `SetMetadata` may set Creator and Producer; that differs from `DocumentSpec.info`, which keeps Creator fixed and does not expose Producer. No dependency edges to or from `pdfcraft-pdf`, `pdfcraft-forms`, or `pdfcraft-orgchart`. Pins workspace `lopdf =0.45.0`. Errors are `Error` + `ErrorKind`. Version `0.1.0` may publish. Implementation stays in this repository; hcsw trees are read-only for behavior reference.
- Each of `pdfcraft-pdf`, `pdfcraft-forms`, and `pdfcraft-orgchart` ships a Diátaxis set: crates.io README (install / quick start), package `docs/guide.md` (how-to), and crate-root rustdoc (overview + example). `pdfcraft-forms` and `pdfcraft-orgchart` guides cover mental model, options, fonts, errors, and testing. Published packages include `docs/**/*`. The workspace root README indexes Guide / API for every crate.

### Changed

- **Breaking.** `pdfcraft-pdf` selects faces with a caller-defined key. `FontFace`, `FontFamily`, and `FontRegistry<K>` replace `Resources::body_font` and `Resources::bold_font`. Build resources with `Resources::with_fonts`. `DocumentSpec`, sections, blocks, paragraphs, inline runs, table cells, bands, and layer text carry `K` (`Copy + Eq + Hash + Debug`). `InlineRun::Text` and `InlineRun::Bold` are `{ text, font }`. `Block::paragraph(font, text)` and `ParagraphSpec::text(font, text)` take that key. `FontRegistry::single` registers one family under `()`. A key that was not registered fails with `ErrorKind::UnregisteredFont`. A bold run whose family has no bold face fails with `ErrorKind::BoldFaceRequired`. `pdfcraft-forms` and `pdfcraft-orgchart` still take one font byte buffer.
- Test and example fonts are no longer vendored in git. Set `PDFCRAFT_PDF_FONT` / `PDFCRAFT_CJK_FONT` (`pdfcraft-pdf`), `PDFCRAFT_FORMS_FONT` (`pdfcraft-forms`), and `PDFCRAFT_ORGCHART_FONT` or `PDFCRAFT_CJK_FONT` (`pdfcraft-orgchart`). Font-dependent tests skip when the variable is unset.
- Renamed crates from `quire-*` to `pdfcraft-*` (`pdfcraft-pdf`, `pdfcraft-forms`, `pdfcraft-orgchart`). Rust imports use `pdfcraft_pdf` / `pdfcraft_forms` / `pdfcraft_orgchart`. Env vars are `PDFCRAFT_FORMS_FONT`, `PDFCRAFT_CJK_FONT`, and `PDFCRAFT_CJK_BOLD_FONT`. PDF `/Creator` is `pdfcraft`.
- `pdfcraft-orgchart` docs describe both equity (shareholders above) and outbound investment (below the subject).

## [0.1.0] - 2026-10-14

### Changed

- `pdfcraft-pdf`, `pdfcraft-forms`, and `pdfcraft-orgchart` are `0.1.0`. These crates can be published.
- The published `pdfcraft-orgchart` package omits `SourceHanSerifHC-ExtraLight.ttf`. The git repository still has that face for tests. `gzip -c` of the face is 14798216 bytes, and crates.io rejects an upload larger than 10485760 bytes. `cargo publish --dry-run` does not report that limit.

### Documentation

- Confirmed `pdfcraft-forms` in the proposal. The crate takes PDF bytes, fills existing AcroForm fields, and flattens on every successful call, including an empty field map. It does not depend on `pdfcraft-pdf`.
- `docs/pdfcraft-orgchart-spec.md` is the behavior contract for equity charts.

- `pdfcraft-pdf` ships a self-contained crates.io-style README, `docs/guide.md` usage guide, crate/module rustdoc, dual license files in the package, and `package.metadata.docs.rs`. Caller docs do not require reading internal proposal notes.

### Fixed

- `pdfcraft-orgchart` returns `ErrorCode::LayoutFailed` when a page draft, page order, placed node, or ratio label has no measured box. Those lookups used to panic.
- `pdfcraft-orgchart` `generate_pdf` measures overflow boxes, continuation cues, and promoted annex subjects with the same skrifa face that draws the page.

### Added

- `pdfcraft-forms` `FillOptions` now covers production fill options. `h_align` / `v_align` and their overrides, `font_size_overrides`, `line_below`, and `absolute_lines` (`AbsoluteLine`, default red 0.8 pt). Unknown names in those maps follow `missing`. Vertical placement uses glyph ink. A last line of one glyph may drop the size by up to 2 pt. A TrueType collection is reduced to `FontBytes.index` before embed. CJK text fills when the caller supplies a `glyf` font. `PDFCRAFT_FORMS_FONT` turns on the Chinese fixture test. Subsetting and OTF are still out.
- `pdfcraft-forms` lists AcroForm widgets with `list_fields` and fills ASCII text, checkboxes, and choices with `fill_and_flatten`. A successful call removes every widget annotation and the catalog `AcroForm` entry. An empty field map still removes the form. Font bytes are an argument on `FillOptions`. The crate pins `lopdf` 0.45.0 with default features off. It does not depend on `pdfcraft-pdf`.
- `pdfcraft-orgchart` checks an equity chart with `validate_request` and builds a PDF info title with `document_title`. `top_n` of `None` or `Some(0)` leaves the request unchanged. A larger `top_n` folds extra shareholder children into one `其他股东(k家)` node. This step does not check the folded node count against `max_nodes`, and it does not draw a PDF.
- `pdfcraft-orgchart` builds in-box lines and page-chrome strings. A ratio stays outside the box. Shareholder and subject lines are the name and the legal person. Investment lines are the name, paid-in amount, location, and approval date. Page titles use `股权架构图`, `对外投资`, `续页`, and `附件`. The footer keeps `頁`. This step does not measure text or draw a PDF.
- `pdfcraft-orgchart` measures node boxes from `FontBytes` with skrifa `0.48.0`. A `ttcf` collection is `FONT_ERROR`. A missing glyph is `MISSING_GLYPH`. A box taller than the portrait limit is `NODE_TOO_TALL`. This step does not draw a PDF.
- `pdfcraft-orgchart` breaks a chart into pages. A chart that fits the elastic band stays on one page. Otherwise shareholder pages, including annexes for a direct row that does not fit, come before investment pages.
- `pdfcraft-orgchart` `generate_pdf` draws those pages with krilla `0.8.2` (`default-features = false`, no `Surface::draw_text`). The caller passes `FontBytes`. The PDF has no document-info title from this crate. Example `equity_chart`. Node lines are clipped to the same rounded rectangle as the box and centered horizontally in the box width.
- Repository is a Cargo workspace: `crates/pdfcraft-pdf` (existing layout library), `pdfcraft-forms`, and `pdfcraft-orgchart` (equity charts). Boundaries are in `docs/workspace-crates-spec.md`. Callers depend on the crate they need. Forms and orgchart do not depend on `pdfcraft-pdf`.
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

- **Breaking.** `Canvas` is crate-private. `layout_spec` and `to_pdf` do not take a caller canvas. `CanvasCommand`, `GlyphRun`, `GlyphPlacement`, `RecordingCanvas`, and `Rendered::pages` stay public so tests can read the recorded log. That log is not a pluggable backend.
- **Breaking.** `FontId::from_raw` and `ImageId::from_raw` are crate-private. `FontId::as_u32` and `ImageId::as_u32` stay public. `FontId` stays in `fonts`.
- **Breaking.** `DocumentSpec` has a public `info` field of type `DocumentInfo`. Struct literals must set it. `DocumentSpec::new` leaves it empty. `layout_spec` keeps the value, and `to_pdf` writes a non-empty title and author into the PDF information dictionary. Empty strings are omitted. Creator stays `quire`. Producer is not set. `pdfinfo` prints `Title` and `Author`. `DocumentInfo` is also available as `spec::DocumentInfo`.
- `LimitKind` is defined in `error` next to `ErrorKind::LimitExceeded`. `limits` and the crate root still re-export it, so `pdfcraft_pdf::LimitKind` is unchanged.
- Layout and backend policy thresholds (column counts, marker ranges, upem tolerance) and product defaults (column gap, blank stroke) are named constants next to the code that owns them. Behavior is unchanged.
- Bump the workspace `skrifa` pin from `0.47.0` to `0.48.0` in `pdfcraft-pdf` and `pdfcraft-forms`. `krilla` 0.8.2 still requires `skrifa` ^0.42.0 (resolved to 0.42.1 on 2026-10-14). No newer krilla is on crates.io, so the two copies remain. `emit` rejects a units-per-em mismatch, and a test locks the fixture glyph's TJ adjustment at `-250`.
- `pdfcraft-forms` pins `lopdf` 0.45.0 with default features off. Parsing no longer uses the removed `nom_parser` feature. `Document::get_page_content` returns the page bytes directly. Direct dependency versions live in `[workspace.dependencies]`.
- **Breaking.** `pdfcraft-forms` `FillOptions` is no longer `Copy` (alignment override maps are borrowed). Struct-update sites that relied on `Copy` need `Clone` or a fresh `FillOptions`.
- **Breaking.** `pdfcraft-forms` no longer exports `DEFAULT_FONT_SIZE`, `MIN_FONT_SIZE`, `FONT_SIZE_STEP`, `LINE_HEIGHT_FACTOR`, `PAD_X`, `PAD_BOTTOM`, or `PAD_TOP`. The values are unchanged.
- **Breaking.** `pdfcraft-forms` `ErrorKind::InvalidArgument` carries `field_name` and `reason: ArgumentReason` instead of `message`. `ErrorKind::FlattenFailed` carries `reason: FlattenReason` instead of `message`. The variant names are unchanged. A non-finite content-stream number and an out-of-range font width fail instead of being written as 0.
- **Breaking.** `pdfcraft-forms` `ErrorKind::FontNotConfigured` carries `reason: FontReason`. `FontReason::Unreadable` keeps the parser error as `source`. The other font reasons have no `source`. Object nesting deeper than 64 during flatten is `FlattenReason::NestingTooDeep`. A non-finite `font_size_overrides` or `line_below` value for a name that exists on the form is `InvalidArgument` even when `missing` is `Ignore`.

- Contract. Callers assemble a document as values. `DocumentSpec` holds the sections and blocks. Headers and footers are `BandSpec`. `behind` and `front` are `LayerMark` lists. `layout_spec` lays that value out. The library substitutes page numbers into `TextRun` when it paints a page. `PageWhen` chooses the pages for a `LayerMark`. A style scope is `Block::Styled`. Callers do not pass closures for the body, the header, the footer, `behind`, or `front`.
- **Breaking.** `document()`, `FlowWriter`, and the public builders that took closures are removed. Build a `DocumentSpec` and call `layout_spec`. Page numbers, page counts, section counters, and last-page marks are `TextRun` and `PageWhen` values. `Band`, `Layer`, `Flow`, `Section`, `IntoFlowItem`, and `ListBlock` are no longer exported. `Track`, `Ratio`, `Numbering`, and `Rendered` stay at the crate root. Layout itself goes through `layout_spec`.
- **Breaking.** `Document`, `Image`, `Document::new`, `Document::set_limits`, and `Document::load_image` are removed. The `document` module is gone. Font bytes and encoded images live on `Resources`. `Resources::add_image` returns an `ImageRef`. Caps are the `DocumentSpec::limits` field. `layout_spec` builds layout nodes directly. A damaged PNG fails while `layout_spec` registers images, with `ErrorKind::Image`. JPEG, GIF, and WebP pixel errors can still fail in `to_pdf` with `ErrorKind::Backend` and no page.
- **Breaking.** `Track::Fixed` takes `Length<Positive>`. `Track::Ratio` takes `Ratio`. `Border` is no longer exported. Call `Table::border`. `DocumentInfo` is no longer re-exported from the crate root. It remains `pdfcraft_pdf::canvas::DocumentInfo` for `Canvas::set_document_info`.
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
- **Breaking.** `Rendered::to_pdf()` is now the free function `pdfcraft_pdf::to_pdf(rendered)`, still behind feature `backend-krilla`.

### Added

- Paragraphs support mixed body and bold font runs plus a typed first-line indent.
- `FontStore` measures nominal glyph advances with skrifa 0.47.0. Missing cmap entries fail the document. The krilla adapter draws the same glyph ids through `draw_glyphs`.
- `Canvas` and `RecordingCanvas`, with an ordered `CanvasCommand` log so layout tests can assert coordinates without PDF bytes.
- Branded lengths (`Length` with `Finite` / `NonNegative` / `Positive`), page geometry (`Point`, `Size`, `Rect`, `Margins`), paint (`Color`, `Paint`, `Stroke`), and non-empty `Path` / `PathBuilder` in `geom`, plus construction `Error` variants. Package name is `pdfcraft-pdf`.
- Repository scaffold, module tree placeholders, and agent/contributor conventions.

### Performance

- Root pagination and stack splitting dequeue with `VecDeque` instead of `Vec::remove(0)`. On a 5000-paragraph release baseline, layout fell from about 135 ms to about 116 ms on the same host.

### Fixed

- `pdfcraft-forms` drops `Prev` and `XRefStm` from the trailer when it rewrites a PDF, and drops trailer keys that point at deleted objects.
- `pdfcraft-forms` no longer requires a U+0020 glyph for whitespace that is not drawn. Overflow comparison no longer allows an extra 0.05 pt. Font-size search uses millipoints and starts from the caller's size. A `font_size` above 256 pt fails before the PDF is opened.

- `keep_together` and a table cell count paragraph spacing between their children. Two lines that fit in the frame, with spacing between them that does not, fail with `ErrorKind::UnsplittableTallerThanFrame`. Spacing after the last child is still outside the group. Ordinary pagination still drops spacing that does not fit, and does not push the next block to the next page for that spacing alone.
- A PNG whose header is valid and whose pixel data is not fails while `layout_spec` registers that image, with `ErrorKind::Image`. JPEG, GIF, and WebP pixel errors can still fail in `to_pdf` with `ErrorKind::Backend` and no page.
- `Layer::text_at` and a custom list marker fail with `ErrorKind::UnexpectedLineBreakInSingleLine` when the string contains a forced line break. Nothing is drawn. Header and footer paragraphs still break on that line break.
- An `ImageRef` from a different `Resources` value is `ErrorKind::UnknownImage`. An image added to the same `Resources` paints, including inside a cell.
- A custom marker on an ordered list no longer advances the counter. Only an automatic label consumes a number. The last automatic label in `examples/hello.rs` is `d.`. That item was `e.` when the custom marker still consumed a number. `GeometryKind::MarkerOutOfRange` carries the next automatic ordinal. LowerAlpha starting at 26, with an automatic item, a custom marker, and another automatic item, fails with number 27.
