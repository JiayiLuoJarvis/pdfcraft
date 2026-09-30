# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html)
while remaining `0.x`.

## [Unreleased]

### Added

- `Canvas` and `RecordingCanvas`, with an ordered `CanvasCommand` log so layout tests can assert coordinates without PDF bytes.
- Branded lengths (`Length` with `Finite` / `NonNegative` / `Positive`), page geometry (`Point`, `Size`, `Rect`, `Margins`), paint (`Color`, `Paint`, `Stroke`), and non-empty `Path` / `PathBuilder` in `geom`, plus construction `Error` variants. Package name is `quire-pdf`.
- Repository scaffold, module tree placeholders, and agent/contributor conventions.
