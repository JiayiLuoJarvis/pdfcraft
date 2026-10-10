# Knife 4 fonts/skrifa — synthesized design

## Synthesis decision

**Base:** Candidate 2 (self-contained `Mark::Glyphs` with `Arc<Vec<u8>>`).

**Dropouts:** candidate-1 and candidate-3 had not written SKETCH.md when this unit proceeded. Arena rule: continue with N-1.

**Grafts**

- Keep `KrillaCanvas::new()` / `new_observable_for_test()` with an empty `Arc<FontStore>` so existing path tests stay one-liners. Add `new_with_fonts(Arc<FontStore>)` for glyph tests.
- Fixture path is `tests/fixtures/quire-measure.ttf` (QuireMeasure, OFL subset), not `quire-subset.ttf`.
- Extract `check_upem(skrifa, krilla)` in `emit.rs` so mismatch is tested without a production escape hatch.
- Rejected from C1: `InvalidGlyphRun`, requiring `Arc<FontStore>` in `emit::render`, `UnknownFont { id: FontId }` (landed as `id: u32`), and a page-local krilla `Font` `HashMap` (briefly landed then removed in 9392f85 to match C2).

**Hard rejects (from C2 rationale, still apply)**

- Passing `FontStore` into `emit::render`
- Second krilla Font registry in knife 4
- Changing `Canvas::draw_glyph_run`
- `fonts.rs` calling krilla

## Shape

Implement C2 SKETCH.md with the grafts above.
