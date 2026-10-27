# Changelog for pdfcraft-forms

All notable changes to this crate will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this crate adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html)
while remaining `0.x`. See the [workspace changelog](../../CHANGELOG.md) for
cross-crate history.

## [Unreleased] - 2026-10-27

### Breaking

- No breaking change in this cycle. (`CellIndex` `non_exhaustive` and the pin
  split touch other crates; listed here only to keep the record complete.)

### Changed

- `missing_docs`, `missing_debug_implementations`, `missing_errors_doc`, and
  `missing_panics_doc` lints flipped from `warn` to `deny` in this crate's
  manifest. Undocumented public items now fail the build.
- Workspace pin policy split: the font/PDF stack stays `=`-pinned while leaf
  utilities use caret requirements. This crate has no direct leaf dependency,
  so its own resolve is unchanged; the workspace `deny.toml` now enforces
  `[bans]` and `[sources]`.
- CI basics job (`fmt` / module-dag / clippy) now gates heavy jobs.

### Added

- MSRV policy written down in `docs/msrv.md` (MSRV 1.92, follows krilla).

### Fixed

- Nothing crate-specific in this cycle.
