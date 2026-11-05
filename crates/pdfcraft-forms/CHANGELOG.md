# Changelog for pdfcraft-forms

All notable changes to this crate will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this crate adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html)
while remaining `0.x`. See the [workspace changelog](../../CHANGELOG.md) for
cross-crate history.

## [Unreleased]

## [0.3.0] - 2026-11-05

### Breaking

Replace `list_fields(&pdf)?` with `Form::open(&pdf)?.fields()`.
Replace `fill_and_flatten(&pdf, &fields, &options)?` with
`Fill::new(&form, request, font)?.to_pdf()?`.

Build `request.fields` as `BTreeMap<String, FieldFill>`. Put a size or an
alignment on `FieldFill.style`. A name that only had a size or an alignment,
and no value, cannot be written. Put underline offsets in `request.rules` as
`FieldRule`. Put `absolute_lines` in `request.lines`, and put the line width
and color on `AbsoluteLine.style`. Pass one `sfnt` to `Font::new`. Pass a
`ttcf` to `Font::from_collection(bytes, index)`.

Compare `error.kind()`. Read the widget name, page index, and missing
character with `field()`, `page()`, and `scalar()`. `Error` is not
`PartialEq`.

- Open a PDF with `Form::open`, compile a fill with `Fill::new`, and write bytes with `Fill::to_pdf`. `list_fields`, `fill_and_flatten`, `FillOptions`, and `FontBytes` are removed. There is no deprecated alias.
- Values are a `BTreeMap<String, FieldFill>`. Size and alignment overrides live on `FieldFill.style` (`FieldStyle`). A name with only a size or alignment and no value cannot be written. Rules are a `BTreeMap<String, FieldRule>`. `AbsoluteLine` stores width and color on `style: StrokeStyle`.
- `Font::new` and `Font::from_shared` read one `sfnt` and reject `ttcf`. `Font::from_collection` and `Font::from_shared_collection` read one face from a `ttcf`. Construction does not clear `head` on the caller's bytes. The embedded subset still clears `head` on the subset only.
- `Error` fields are private. Compare `kind()`. Read the widget name, page index, and missing character with `field()`, `page()`, and `scalar()`. `Error` is not `PartialEq`. There is no public `Error::new` and no `From<ErrorKind>`.
- `FieldInfo` fields are private. Use `name()`, `field_type()`, `page_index()`, and `options()`. `FieldType` and `FieldValue` are exhaustive. `CheckBox(true)` draws the check. `CheckBox(false)` draws nothing. A `FieldStyle` on a checkbox is checked and does not change the check mark.
- A bad PDF fails in `Form::open` before an illegal font size can be reported. A non-finite line fails in `Fill::new` before a missing glyph. A line page past the document is `InvalidArgument(PageOutOfRange)` from `Fill::new`, and `page()` is that index. `FlattenFailed(PageOutOfRange)` is removed.
- A missing glyph, a glyph with no advance, and text that does not fit return from `Fill::new`. `to_pdf` subsets, clears the subset `head`, paints, strips, and writes.
- When several problems are present, which one returns follows the order in the reshape spec §5. `Display` text can differ from `0.2.0` and does not include `source()`.

Successful fills keep the `0.2.0` PDF bytes.

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
