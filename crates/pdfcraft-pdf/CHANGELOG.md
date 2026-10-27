# Changelog for pdfcraft-pdf

All notable changes to this crate will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this crate adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html)
while remaining `0.x`. See the [workspace changelog](../../CHANGELOG.md) for
cross-crate history.

## [Unreleased] - 2026-10-27

### Breaking

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

### Fixed

- Nothing crate-specific in this cycle.
