# How: fonts + canvas glyphs + krilla emit (knife 4 grounding)

Parent how explorers: grok-4.7-high (three angles). Explainer skipped: parent already read the same files. Models `grok-4.7-xhigh-fast` / `claude-opus-5-5-max` are not in the Task allow-list. Seats used `grok-4.7-high`.

## Overview

Knife 4 adds skrifa measurement and krilla `draw_glyphs`. Line break, pagination, and document DSL stay stubs.

`FontId` already exists. `GlyphRun` already carries `advance_norm` (font units / upem, not times size). `RecordingCanvas` already records glyph runs. `KrillaCanvas` still poisons `draw_glyph_run`. `emit.rs` is the only place that may construct `Document` / `Page` / `Surface`. `fonts.rs` is id-only. `text/measure.rs` is a doc comment. `tests/fixtures/` has no font file.

## Key concepts

- `FontId(pub(crate) u32)` in `src/fonts.rs`. Public via `from_raw` / `as_u32` for tests. Design wants allocation from a store, not caller-invented ids at PDF time.
- `GlyphPlacement.advance_norm` and `GlyphRun` in `src/canvas.rs` (lines 83–116). User width is `advance_norm * font_size`.
- `KrillaCanvas` buffers `PagePlan` / `Mark::Fill|Stroke`. No live krilla types. Glyphs must become a new `Mark` and be drawn in `emit.rs`.
- Poison: first `Error` is stored. `unsupported_glyph_run_poisons_finish` in `src/backend/emit.rs` (~284) must change once glyphs work.
- Design PR5/PR6 (`docs/quire-design.md` 446–535, 1584–1596): `FontStore`, charmap, unscaled `advance_width`, missing cmap is `MissingGlyph`, krilla `Font::new` on same `Arc<Vec<u8>>` when feature on, upem delta `>= 0.01` is `FontMetricsMismatch`.
- krilla 0.8.2 `Font::new(Data, index) -> Option`, `Data: From<Arc<Vec<u8>>>`, `KrillaGlyph::new` wants normalized advances, `Surface::draw_glyphs(..., outlined: false)`. Do not call `draw_text`.
- Two skrifa copies: krilla pins `^0.42.0` (tree has 0.42.1). Direct pin is `=0.47.0`. Pass `u32` glyph ids only.

## How it works today

1. Tests call `RecordingCanvas::draw_glyph_run` and match `CanvasCommand::DrawGlyphRun`.
2. Tests call `KrillaCanvas::draw_glyph_run` and get `UnsupportedCanvasOperation { operation: "draw_glyph_run" }`, which poisons `finish`.
3. `emit::render` walks `Mark::Fill` / `Mark::Stroke` only.
4. Observable PDF uses `Serialization { pretty: true, compress_content_streams: false, enable_tagging: false }` via `new_observable_for_test`. Path tests pin `10 20 m` plus `1 0 0 -1 0 200 cm`. TJ tests must use that profile and pin the observed adjustment number, not the letters `Tj`.

## Where things live

| Path | Now | Knife 4 |
| --- | --- | --- |
| `Cargo.toml` | comment for skrifa | pin `skrifa = "=0.47.0"` always (measure without backend) |
| `src/fonts.rs` | `FontId` | `FontStore`, `FontSource`, `FaceMetrics`, register |
| `src/text/measure.rs` | stub | charmap + advance + `line_metrics` |
| `src/error.rs` | no font variants | `Font`, `MissingGlyph`, `FontMetricsMismatch`, `UnknownFont` |
| `src/backend/krilla_canvas.rs` | poison glyphs | record glyph `Mark`; need a way to resolve `FontId` to krilla `Font` at emit |
| `src/backend/emit.rs` | paths + links | `draw_glyphs`; replace unsupported glyph test |
| `tests/fixtures/` | README only | small OFL subset (ASCII, 0-9, U+2022, 第页共). Do not vendor full Source Han |

## Constraints from knife 3 (must keep)

- Identity convert. No pre-flip.
- `emit.rs` only constructs krilla document objects. `krilla_canvas.rs` must not `use krilla`.
- Therefore `krilla::text::Font` cannot live in `krilla_canvas.rs`. Store bytes/ids in the plan, or keep a font table that emit can read, with krilla `Font` built in emit or in a backend file that is allowed to import krilla.
- `fonts.rs` is outside `backend/`. It must not `use krilla::…`. Registering krilla `Font` belongs in `backend/` (cfg feature) or emit reconstructs from `Arc<Vec<u8>>`.
- Images stay `UnsupportedCanvasOperation`.
- No unicode-segmentation this knife (PR7). Measure per Unicode scalar. Combining graphemes wait for knife 5.
- No `LimitKind` module yet. Empty / unreadable fonts are `Error::Font`. Oversize can be `Error::Font` with a 32 MiB check, or skip until limits land. Prefer a constant 32 MiB mapped to `Error::Font` rather than inventing `LimitExceeded` this knife.
- `FontTable` / DSL insert is knife 8. `FontStore::register` is the crate-internal API.

## Tests the design demands

1. Fixture `descent < 0`. One known ASCII advance. One known CJK advance. Unmapped scalar is `MissingGlyph`. No glyph-0 branch.
2. `RecordingCanvas`: two glyphs. Second origin.x = first.origin.x + advance_norm * font_size.
3. Adapter: submit `advance_norm = hmtx/upem + 0.25`. Pretty uncompressed stream. Pin the TJ adjustment number after first observation. Comment that it is the 1000-unit hmtx delta, not `advance_norm`. Do not search for `Tj`.
4. upem mismatch path still exists; it does not replace (3).
5. Bad bytes / empty bytes → `Error::Font`.
6. `--no-default-features` still measures. Glyph PDF tests stay under `backend-krilla`.

## Gotchas

- Charmap `None` covers unmapped and `.notdef`. Do not special-case id 0.
- Adapter must not multiply `advance_norm` by size or divide by upem again. `KrillaGlyph` already multiplies by size in `x_advance(size)`.
- `draw_glyph_run` must set fill (glyph color) and clear stroke, same pen hygiene as paths.
- Unknown `FontId` on the krilla canvas is `UnknownFont`, not unsupported.
- Design still sketches `KrillaCanvas` owning a live `Document`. That sketch is stale. Follow the buffered plan.

## Open questions for arena (reversible; do not ask the human)

1. Where is krilla `Font` constructed: emit from bytes each draw, or a backend-only registry?
2. Does `KrillaCanvas` hold `Arc<FontStore>` (skrifa side only) or does each glyph `Mark` clone `Arc<Vec<u8>> + index`?
3. Public re-exports: `FontId` only, or also `FontSource`? Design has `FontSource` as public-ish; store methods are `pub(crate)`.
