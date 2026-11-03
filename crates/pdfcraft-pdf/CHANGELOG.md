# Changelog for pdfcraft-pdf

All notable changes to this crate will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this crate adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html)
while remaining `0.x`. See the [workspace changelog](../../CHANGELOG.md) for
cross-crate history.

## [Unreleased]

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
