# Changelog for pdfcraft-orgchart

All notable changes to this crate will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this crate adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html)
while remaining `0.x`. See the [workspace changelog](../../CHANGELOG.md) for
cross-crate history.

## [Unreleased]

## [0.1.1] - 2026-10-30

Public entry points are unchanged. `validate_request`, `generate_pdf`,
`document_title`, and `ErrorCode::as_str` keep their signatures.

### Changed

- `missing_docs`, `missing_debug_implementations`, `missing_errors_doc`, and
  `missing_panics_doc` lints flipped from `allow` to `deny` in this crate's
  manifest. Undocumented public items now fail the build.
- Workspace pin policy split: the font/PDF stack stays `=`-pinned while leaf
  utilities use caret requirements. This crate has no direct leaf dependency,
  so its own resolve is unchanged; the workspace `deny.toml` now enforces
  `[bans]` and `[sources]`.
- CI basics job (`fmt` / module-dag / clippy) now gates heavy jobs.

### Added

- MSRV policy written down in `docs/msrv.md` (MSRV 1.92, follows krilla).

### Fixed

- Node and continuation-box strokes are black. Edges, edge-arrow fills,
  continuation leaders, and leader-arrow fills are 0.2 gray (byte 51).
  Continuation boxes use a 1.0 pt stroke, a 1.0 mm corner, and a 4/3 dash.
  Leader arrows are 4.5 pt at 30 degrees. Cue text is centered in its slot.
- Header titles start at the page margin. The subtitle sits 1.5 mm below the
  main title. A header line that does not fit ends with `…`.
- Ratio labels sit 1.2 mm to the right of a hugging stub. A node with no hugging
  stub is unlabeled. An investment baseline uses the descender plus 0.4 mm, not
  the full line height.
- A `top_n` fold sums the dropped ratios with the same finite sum as CPython
  `math.fsum`. Six copies of `100/6` are accepted. A sum that is clearly above
  100 is still `INVALID_PROTOCOL`.
- Sparse continuation buckets truncate `gap / 0.05` toward zero. A gap of 0.35
  counts as 6. Column count still caps at 64.
