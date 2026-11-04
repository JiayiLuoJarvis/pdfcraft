# Changelog for pdfcraft-pdf

All notable changes to this crate will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this crate adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html)
while remaining `0.x`. See the [workspace changelog](../../CHANGELOG.md) for
cross-crate history.

## [Unreleased]

Version `0.6.0` (shape refactor, proposal §12.7). Lands over several changes; this
section collects them. The field map from `0.5` is
[`docs/migrate-0.6.md`](docs/migrate-0.6.md).

### Breaking

- `ParagraphSpec` is now `{ style: ParagraphStyle, children: Vec<Span<K>> }`.
  `runs` is `children`. `color` and `font_size` move into `style.text`
  (`RunStyle`). `first_line_indent` and `align` move into `style`.
  `line_height_factor: Option<f32>` is `style.line_height: Option<LineHeight>`.
  `..ParagraphSpec::default()` still works.
- `InlineRun` is renamed `Span`, and `Link.runs` is `Link.children`.
  `InlineRun::split_latin` is `Span::split_latin`. There is no alias.
- `CellBlock` is removed. `CellSpec.content` is `children: Vec<Block<K>>`.
  Cell text is `Block::paragraph`. Columns, nested tables, keep-together
  groups, and page breaks in a cell fail at `layout_spec` with
  `InvalidCellContent` and container `"table cell"`.
- Columns move presentation into `ColumnsStyle` and `ColumnStyle`;
  `columns` / `blocks` are now `children`.
- Lists are `{ style: ListStyle<K>, children }`. Ordered-list `numbering`,
  `start`, and `format` move into `ListKind::Ordered`; list-item `blocks` is
  `children`.
- Tables are `{ style: TableStyle, header: Option<TableHeader<K>>, body }`.
  Row `cells` is `children`, and cell `fill` moves into `CellStyle`.
- `Block::KeepTogether` is now `{ children }`. `Block::Inset` is
  `{ style: InsetStyle, children }`. `Block::Anchor.child` is replaced by
  `children`.
- `ImageSpec` is now `{ image, style: ImageStyle }`. `width`, `height`, and
  `align` move into `style`.
- `CanvasSpec` is now `{ size, style: CanvasStyle, marks }`. `align` moves
  into `style`. `CanvasMark` is unchanged.
- `Block::HLine` takes `HLineStyle { width, align, thickness, color }`.
  `HLineSpec` is removed, with no alias. `space_before` and `space_after`
  are removed.
- `SectionSpec.page_size` and `SectionSpec.margins` move into
  `style: SectionStyle`. `BandSpec.inset` moves into `style: BandStyle`.
- `TextRun` is renamed `PageText`. There is no alias.
- `BandMark` is renamed `Mark`. There is no alias. `Mark::Rotated.degrees`
  is `Angle`, not `f32`.
- `BandSlot` is a struct. The old `Text` variant is `link: None`. The old
  `Link` variant is `link: Some(uri)` with the visible runs in `children`.
- `LayerMark` is `{ when, child: Mark }`. The old `Text`, `Image`, `Line`,
  `Rect`, and `Ellipse` variants are `Mark::Shape`. The old `Rotated`
  variant is `child: Mark::Rotated`.

### Added

- `ParagraphStyle` holds a paragraph's reusable presentation: `text`, `align`,
  `first_line_indent`, `line_height`. It is `Copy`, has public fields and
  `Default`, and is meant to be built with `..ParagraphStyle::default()`.
  Adding a field to it is a breaking change while `0.x`.
- `ParagraphStyle::text` accepts every `RunStyle` field. Bold, italic,
  underline, strikethrough, and tracking can now be set for a whole paragraph.
  They apply over an enclosing `Block::Styled` and under each span's own style.
  Bold or italic still needs the face, or fails with `BoldFaceRequired` and the
  related errors.
- `spec::LineHeight::new(factor)` validates a line-height factor.
- `Block::Styled` may wrap cell children. Its fields apply to those paragraphs.
- `ColumnsStyle`, `ColumnStyle`, `ListStyle`, `TableStyle`, `CellStyle`,
  `InsetStyle`, `ImageStyle`, `CanvasStyle`, and `HLineStyle` hold reusable
  presentation. `ImageStyle` defaults to unset width and height with
  `Alignment::Start`. `CanvasStyle` defaults to `Alignment::Start`.
  `HLineStyle` defaults to full width, `Alignment::Center`, 1 pt, and black.
- `SectionStyle` holds `page_size` and `margins`. `None` page size still
  uses the document page size. `BandStyle` holds `inset`.
- `Angle::degrees` builds a finite clockwise angle. `as_degrees` reads it
  back. `+0` and `-0` compare equal. NaN and infinity fail with
  `GeometryKind::NonFiniteRotation`.
- `TableHeader { repeat, rows }` makes the header slot and its repetition rule
  explicit. An empty `rows` list is equivalent to no header.
- `Block::Anchor.children` accepts multiple blocks and body-level page breaks.
  The destination lands on the first effective fragment after any break.

### Changed

- An invalid line-height factor (NaN, infinite, zero, or negative) now fails
  when `LineHeight::new` is called, with the same
  `GeometryKind::LineHeightFactor { factor }`. It used to fail inside
  `layout_spec`. Line heights for valid factors are unchanged.
- A table inside `Block::Styled { bold: Some(true), .. }` paints cell
  paragraphs in the bold face. Italic, underline, strikethrough, and tracking
  follow the same way. `CellBlock::Text` used to keep only size and color. A
  missing bold face fails with `BoldFaceRequired`. List markers stay on size
  and color.
- An empty cell paragraph (`Block::paragraph(font, "")`) counts as one block.
  Empty `CellBlock::Text` counted as zero.
- A page break directly inside a top-level anchor follows the normal body
  page-break rules. Nested frames still reject page breaks.
- A horizontal rule's height is now its `thickness`, and the stroke runs
  `thickness / 2` below its top. It used to add `space_before` and
  `space_after`. For the old spacing, put `Block::Spacer` before and after
  the rule. On one page the coordinates are the same.
- Across a page boundary the migrated form behaves differently. A spacer
  splits and carries its remainder to the next page, while the rule itself
  still moves whole. A rule whose spacing used to push it to the next page
  may now stay on the current page. Spacing that was taller than the body
  used to fail with `UnsplittableTallerThanFrame` and now flows. Wrap the
  three blocks in `Block::KeepTogether` to keep the old behavior, including
  the error.
- Each spacer counts toward `LimitKind::Blocks`. A migrated rule counts up
  to 3 blocks, or 4 inside `KeepTogether`.
- A non-finite angle now fails in `Angle::degrees`, with the same
  `GeometryKind::NonFiniteRotation { degrees }`. It used to fail inside
  `layout_spec`. Finite angles, including `+0` and `-0`, still paint the
  same way. `Affine::rotate_cw_about` takes `Angle` and returns the map
  directly. A pivot far from the origin can still fail later as
  `NonFiniteLength` when the rotated bounds are checked.

## [0.5.0] - 2026-11-03

### Breaking

- `ListSpec` adds the public field `item_gap`. A struct literal must set
  `item_gap: NonNegative::ZERO` or end with `..ListSpec::default()`.
- `ListSpec.nest_indent` is now `body_indent`. The meaning is unchanged. It is
  the body column's left edge, measured from the list's left edge. `None`
  still uses `marker_width`. Replace the field name. There is no alias.
- `ListSpec` adds `marker_format`, `marker_font`, and `marker_align`. A struct
  literal must set them or end with `..ListSpec::default()`. The defaults are
  `MarkerFormat::DotSuffix`, `None`, and `MarkerAlign::Start`.
- `Block` and `CellBlock` each gain an `Inset { left, right, children }`
  variant. Both enums are exhaustive, so a `match` that listed every variant
  needs a new arm. Add `Block::Inset { .. }` and `CellBlock::Inset { .. }`,
  or a wildcard.

### Added

- `ListSpec::item_gap` is extra space between adjacent items. It is added to
  the previous item's trailing spacing. A gap that does not fit in the
  remaining height is dropped and is not carried onto the next page.
- `MarkerFormat` sets the suffix or parentheses of a generated number
  (`1.`, `1、`, `1)`, `(1)`, `1`). Bullets and custom markers ignore it.
  `MarkerAlign` places the marker on the left or right edge of the marker
  column. `marker_font` draws bullets, numbers, and custom markers with that
  family's regular face. `None` keeps the first registered face. Size and
  color still follow `Block::Styled`. Bold and italic do not.
- `LimitKind::NestingDepth` and `Limits::{max_nesting_depth,
  set_max_nesting_depth}` cap the total nesting depth of the document. The
  default is 16 and the setter accepts up to 64. The check runs before fonts
  and images are read, so input nested too deeply fails with an error and does
  not overflow the stack.
- `Block::Inset` and `CellBlock::Inset` shift their children right by `left`
  and lay them out in a frame narrowed by `left + right`. They paint nothing
  and add no vertical space. A continuation page keeps the same insets. The
  first text baseline is the child's, so a horizontal shift does not move a
  list marker that aligns to that block. `left + right` greater than or equal
  to the frame fails with `GeometryKind::InsetWiderThanFrame { left, right, avail }`.
  The three fields are points. The check uses the frame being laid out. A child
  that is still too wide for the narrowed frame reports its own error. An inset
  counts as one block and one nesting level. Header and footer bands are not
  built from blocks, so they have no inset.
- `docs/lists.md` is a caller guide for list and inset usage, with recommended
  patterns and examples.

### Changed

- List items may hold every block except a page break, including tables,
  columns, images, canvases, and keep-together groups. Documents that used to
  fail with `InvalidCellContent` for those blocks now lay out. A list inside a
  table cell still keeps the cell's rules in every item. `InvalidCellContent`
  now names the innermost container that forbids the node, so columns in a
  list item in a cell report `"table cell"`, and a page break there reports
  `"list item"`.
- A list marker aligns to the first text baseline of the item's first block.
  A paragraph uses its first line. A table uses its first physical row, and
  that row is the header when the table has one. Columns use the topmost
  first line among the columns. A nested list uses its first marker. When
  that block has no text baseline (an image, a canvas, a rule, or a spacer
  taller than zero), the marker's line box sits on the item top. A spacer
  used as the item's first block no longer moves the marker down to a later
  paragraph. Put that blank space before the list, or set `item_gap` between
  items.

## [0.4.0] - 2026-11-03

### Changed

- MSRV is now 1.98 (was 1.92). Per VER-1 this raises the `0.y` digit.
- Lint configuration inherits the workspace `[workspace.lints]` table. Writing
  rules live in [`docs/rust-engineering-standard.md`](../../docs/rust-engineering-standard.md).
- Examples that need `to_pdf` require feature `backend-krilla` (including
  `anchors`). Integration tests that call `to_pdf` are gated the same way so
  `--no-default-features` stays green.

## [0.3.0] - 2026-11-02

### Breaking

- `BandSpec` adds the public field `marks: Vec<BandMark<K>>`. A struct literal
  must set `marks: Vec::new()` or end with `..BandSpec::default()`. `Default`
  leaves `marks` empty. Callers can still build `BandSpec` with a struct literal.
- `GeometryKind::BandMarkOutside { x, y }` reports a header or footer mark
  whose bounds start above or to the left of the band. `x` and `y` are that
  corner. The geometry display is `header or footer mark starts outside its band at (x, y)`.

### Added

- `BandMark<K>` is `Shape(LayerShape<K>)` or `Rotated { degrees, pivot, shapes }`.
  A header or footer paints those marks in order, then the left and right slots.
  Coordinates start at the band top-left. `x` matches the page. Footer `y` starts
  at the top of the footer band. `BandInset` does not move marks. A box that
  crosses the bottom of the margin is `DecorationTallerThanMargin`. A box that
  crosses the page width is `DecorationWiderThanFrame`. The library does not clip
  the mark and does not grow the margin. Choose the page with `BandFn::from_fn`.
  `BandMark` has no `PageWhen`. See `docs/migrate-band-marks.md`.

## [0.2.0] - 2026-11-02

### Breaking

- `SectionSpec::header` and `SectionSpec::footer` are `Option<BandFn<K>>`.
  `BandFn::fixed` keeps one `BandSpec` on every page of the section.
  `BandFn::from_fn` runs after body pagination, once per page, header then
  footer. `Ok(None)` skips painting that side. The margin stays, and the body
  is not laid out again. `None` means that side does not exist.
- `ErrorKind::BandFn` and `Error::band_fn` report a function failure. Display
  stays `header or footer function failed`. The caller's error is
  `std::error::Error::source`. `layout_spec` requires `K: 'static`.
- `CellIndex` is now `#[non_exhaustive]`. External code can no longer build it
  with a struct literal (`CellIndex { row, col }`). Add a wildcard arm when
  matching, and obtain values from layout output instead of constructing them.
- `unicode-segmentation` moved from an `=` pin to a caret requirement, so
  `cargo update` can freshen it. The font/PDF stack (`krilla`, `skrifa`,
  `read-fonts`, `lopdf`, `png`) stays exactly pinned because measurement and
  drawing share one glyph pipeline (`emit::check_upem` plus the `TJ` fixture lock).

### Changed

- `missing_docs`, `missing_debug_implementations`, `missing_errors_doc`, and
  `missing_panics_doc` lints flipped from `warn` to `deny` in this crate's
  manifest. Undocumented public items now fail the build.
- CI basics job (`fmt` / module-dag / clippy) now gates heavy jobs, and the
  deny job also checks `bans` and `sources` with a skip-list for the known
  intentional duplicates (`skrifa`, `read-fonts`, `font-types`, `miniz_oxide`,
  `weezl`).

### Added

- MSRV policy written down in `docs/msrv.md` (MSRV 1.92, follows krilla).
