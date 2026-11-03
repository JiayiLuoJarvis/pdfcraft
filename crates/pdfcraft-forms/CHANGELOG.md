# Changelog for pdfcraft-forms

All notable changes to this crate will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this crate adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html)
while remaining `0.x`. See the [workspace changelog](../../CHANGELOG.md) for
cross-crate history.

## [Unreleased]

## [0.2.0] - 2026-11-03

### Changed

- MSRV is now 1.98 (was 1.92). Per VER-1 this raises the `0.y` digit.
- Lint configuration inherits the workspace `[workspace.lints]` table. Writing
  rules live in [`docs/rust-engineering-standard.md`](../../docs/rust-engineering-standard.md).
- `missing_docs`, `missing_debug_implementations`, `missing_errors_doc`, and
  `missing_panics_doc` fail the build (deny).
- Workspace pin policy: the font/PDF stack stays `=`-pinned; leaf utilities use
  caret requirements. This crate has no direct leaf dependency. `deny.toml`
  enforces `[bans]` and `[sources]`.

### Fixed

- When more than one `font_size_overrides` or `line_below` entry is invalid, the
  error names the lexicographically first field. Successful fills are unchanged.

## [0.1.0] - 2026-10-14

Initial publish. See the workspace changelog for the forms fill and flatten
surface.
