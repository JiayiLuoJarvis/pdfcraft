# Changelog for pdfcraft-post

All notable changes to this crate will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this crate adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html)
while remaining `0.x`. See the [workspace changelog](../../CHANGELOG.md) for
cross-crate history.

## [Unreleased] - 2026-10-27

### Breaking

- `getrandom` moved from an `=` pin to a caret requirement, so `cargo update`
  can freshen it. This is dependency metadata only, not an API change, but it
  can shift this crate's lockfile resolve, so it is called out here.

### Changed

- `missing_docs`, `missing_debug_implementations`, `missing_errors_doc`, and
  `missing_panics_doc` lints flipped from `allow` to `deny` in this crate's
  manifest. Undocumented public items now fail the build.
- The workspace `deny.toml` now enforces `[bans]` and `[sources]` with a
  skip-list for the known intentional duplicates. CI basics job
  (`fmt` / module-dag / clippy) now gates heavy jobs.

### Added

- MSRV policy written down in `docs/msrv.md` (MSRV 1.92, follows krilla).

### Fixed

- V5/R6 encryption now declares `StdCF` `/Length 32` (plus `/AuthEvent /DocOpen`
  for qpdf parity), top-level `/Length 256`, and a trailer `/ID` when absent,
  so strict readers (Adobe Acrobat, poppler) accept the output. Only the
  emitted framing dictionaries changed; passwords, keys, and permissions
  are untouched.
