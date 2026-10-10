# How: Canvas subsystem (slice 2 grounding)

## Overview

quire draws PDFs through a sealed private krilla adapter. Layout and tests never see krilla. They talk to a crate-owned `Canvas` trait. `RecordingCanvas` implements that trait by appending paint commands to a `Vec` so layout tests can assert coordinates without PDF bytes. Slice 1 already shipped branded geom (`Point`, `Size`, `Rect`, `Path`, `Paint`, `Stroke`, `Color`) and construction `Error`s. `src/canvas.rs` is still a module stub. Backend / fonts / document builders stay out of this slice.

## Key Concepts

- **Canvas**: page-scoped drawing surface. Knows pages, paths, images, glyph runs, link annotations. Does not break lines or paginate.
- **RecordingCanvas**: test double. Ordered `PaintCommand` log. No PDF. No `finish_document`.
- **GlyphRun / GlyphPlacement**: positioned glyphs with normalized advances (`font_units / upem`). Adapter must not rescale again.
- **DecodedImage / ImageId**: pixel size + opaque id. No krilla types on the trait.
- **DocumentInfo**: metadata bag used by `set_document_info`. Lives with document types in the design; needed on the trait now.
- **Coordinates**: top-left origin, y down, PDF points. Identity mapping into krilla 0.8.2 Surface later.

## How It Works (intended flow)

1. Caller / paginate builds positioned fragments.
2. `set_document_info` then `start_page(size)`.
3. Draw: `fill_path` / `stroke_path` / `draw_image` / `draw_glyph_run` / `annotate_link` (links queue until page end on the real adapter).
4. `finish_page`. Repeat pages. Adapter-only `finish(self) -> Vec<u8>` is not on the trait.
5. Tests use `RecordingCanvas::commands()` and absolute 0.01 pt tolerance.

## Where Things Live

| Path | Role now |
| --- | --- |
| `src/geom.rs` | Length brands, Point/Size/Rect/Path/Paint/Stroke (done) |
| `src/error.rs` | Construction errors only today; canvas may need `EmptyLink` |
| `src/canvas.rs` | Target of this slice |
| `src/fonts.rs` | Placeholder; `FontId` may land as a thin newtype |
| `src/document.rs` | Placeholder; `DocumentInfo` may land as a data struct (no DSL) |
| `src/backend/*` | Feature-gated stubs; out of scope |
| `docs/quire-design.md` §Canvas | Technical shape for the trait (authority below 提案书) |

## Gotchas

- Proposal §2.14: flow-inline `canvas` element is deferred. This slice is the drawing trait, not that element.
- Design's bare `f32` margins/sizes are superseded by slice-1 branded lengths. Canvas signatures take `Size` / `Rect` / `Stroke` from geom.
- Design lists many `Error` variants not yet present. Add only what RecordingCanvas needs (`EmptyLink` for empty URI). Do not flood the enum for unused backend cases.
- `Path` is already non-empty; `Stroke` width is `Positive`. RecordingCanvas need not re-validate geometry.
- AGENTS forbids public API / DSL builders before settled items. Thin `DocumentInfo` / `FontId` data types are scaffolding for the trait, not element builders.
- No remote / gh token may block Opening a PR (same as slice 1).
