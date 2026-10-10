# Candidate 2 rationale

## Problem

Knife 4 must measure nominal glyphs with skrifa and draw the same glyph ids through krilla. The current canvas records pages before `emit.rs` creates a krilla document. `FontId` alone is not enough for emission, but passing a live font store into emission would make buffered page plans depend on external state. The design must also keep krilla out of `fonts.rs` and `krilla_canvas.rs`, preserve measurement without the backend feature, and report an unknown id while the run is recorded.

## Usage (caller's view)

Crate-internal layout registers and measures a font without enabling krilla:

```rust
let mut fonts = FontStore::new();
let body = fonts.register(FontSource { bytes, index: 0 })?;
let measured = measure_text(&mut fonts, body, "合同 A-18")?;
```

Layout builds the existing `GlyphRun`. `RecordingCanvas` needs no font store and continues to record that run directly:

```rust
recording.draw_glyph_run(&run)?;
```

The private PDF path freezes the measured store behind an `Arc`. The adapter resolves `run.font` when it records the mark:

```rust
let mut canvas = KrillaCanvas::new(Arc::new(fonts));
canvas.start_page(A4)?;
canvas.draw_glyph_run(&run)?;
canvas.finish_page()?;
let pdf = canvas.finish()?;
```

An id absent from that store returns `Error::UnknownFont` from `draw_glyph_run`. It never reaches emission.

## Shape

`FontStore` contains `Arc<Vec<u8>>`, the face index, skrifa metrics, and a glyph-advance cache. It does not contain krilla types. `measure_text` maps each scalar through the skrifa charmap, reads an unscaled advance, and calculates `advance_norm` once. A missing map entry is `MissingGlyph`. There is no glyph-zero branch.

`Mark::Glyphs` contains the font byte `Arc`, the face index, the skrifa units per em, the positioned glyphs, and the source text. This self-contained variant makes the buffered plan the unit of emission, per Foundational Thinking and Model the Domain. `emit.rs` does not need a resolver or a store.

`KrillaCanvas` receives `Arc<FontStore>` in its private constructor. `draw_glyph_run` resolves the id and copies the resolved material into the mark. This places `UnknownFont` at the adapter boundary, per Boundary Discipline. `finish` drops the store before it calls `emit::render`.

`emit.rs` constructs `krilla::text::Font` from each glyph mark. It compares krilla units per em with the skrifa value and returns `FontMetricsMismatch` for a delta at or above `0.01`. It then passes every `advance_norm` unchanged to `KrillaGlyph` and calls `draw_glyphs` with `outlined: false`.

The interface remains narrow. Knife 4 adds no public builder, resolver, store, or source type. `Canvas::draw_glyph_run` does not change. Only `FontId` remains public. Keeping the existing trait and avoiding a second registry follows the Laziness Protocol. Dropping the store before emission also reduces the state a maintainer must trace, per Minimize Reader Load.

## Synthesis decision

filled by arena later

## Tradeoffs accepted

- We accept one cheap byte `Arc` clone plus owned glyph and text data per mark in exchange for page plans that emit without external state.
- We accept constructing a krilla font for each glyph mark in exchange for avoiding a second font registry in knife 4.
- We accept detecting a units-per-em mismatch during emission in exchange for keeping registration independent of krilla.
- We accept holding `Arc<FontStore>` during recording in exchange for reporting unknown ids before `finish`.

## Alternatives considered

- Pass `FontStore` to `emit::render`. This hides less inside each mark, but emission must resolve ids and can fail late for missing entries. The page plan is no longer self-contained.
- Keep a backend registry of `FontId` to `krilla::text::Font`. This can avoid repeated font construction, but it adds a second registry whose contents must stay synchronized with the skrifa store.
- Add a font resolver argument to `Canvas::draw_glyph_run`. This makes resolution explicit, but it expands every canvas implementation for a concern that only the private PDF adapter has.
- Call a feature-gated backend hook from `FontStore::register`. This moves the metrics check earlier, but it reverses the required dependency direction between fonts and the backend.

## Open questions and risks

- Will repeated `krilla::text::Font::new` calls become measurable for documents with many short runs, and would a render-local cache keyed by byte identity and face index remove that cost without changing the plan?
- Will copying placements and text into each mark raise peak memory for long documents beyond the page-plan cost already accepted by the buffered backend?
- Does the selected subset fixture have stable glyph advances, the required Chinese characters, U+2022, and a license that permits repository redistribution?
- Can the mismatch test inject an incorrect stored skrifa units-per-em value without adding a production-only escape hatch?

## Next implementation step

Add the unconditional skrifa pin, the four font errors, and the skrifa-only `FontStore` registration tests before connecting glyph marks to the backend.
