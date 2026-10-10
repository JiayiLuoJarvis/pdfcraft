# Arena frame: knife 4 fonts/skrifa design sketch

## Artifact

One candidate design package (usage + type sketch + rationale) for FontStore, skrifa measure, and krilla `draw_glyphs` on branch `feat/fonts-skrifa`.

Write:

- `SKETCH.md` (types, signatures, module map, `not implemented` bodies)
- `RATIONALE.md` per rationale-template.md

Do not implement production code. Do not edit `src/`.

## Grounding (must honor)

Read:

- `/mnt/data/demo/quire/.audit/fonts-skrifa/how.md`
- `/mnt/data/demo/quire/docs/提案书.md` §3 §4 and 确认总表
- `/mnt/data/demo/quire/docs/quire-design.md` fonts/measure/PR5/PR6 (approx 446–535, 1584–1596)
- `/mnt/data/demo/quire/AGENTS.md` layering
- `/mnt/data/demo/quire/.audit/backend-krilla/arena/synthesis/SYNTHESIS.md` (buffered plan is the landed shape)

Facts:

- skrifa `=0.47.0`. Charmap then unscaled advance. Same glyph ids to `draw_glyphs`. No `draw_text`.
- Missing cmap → whole document fail (`Error::MissingGlyph`). No `.notdef`, no fallback.
- `fonts/` and `text/` must not `use krilla`. Only `backend/` may.
- `krilla_canvas.rs` currently does not import krilla. Keep that. Glyph `Mark` uses crate types. `emit.rs` draws.
- `GlyphRun.advance_norm` is already font_units/upem.
- TJ test: deliberately wrong `advance_norm` (hmtx/upem + 0.25), pin observed TJ number, never assert stream contains `Tj`.
- No line break, paginate, DSL, images, unicode-segmentation, LimitKind this knife.
- Measure without backend-krilla must work.
- Knife 3 `unsupported_glyph_run_poisons_finish` is replaced by real glyph emit tests.

## Assigned structural direction (do not converge)

You are **{CANDIDATE}**. Follow only your direction. Name what you rejected.

- **candidate-1:** `FontStore` is skrifa-only (bytes, metrics, advance cache). `KrillaCanvas` holds `Arc<FontStore>`. `Mark::Glyphs` stores `FontId` + `GlyphRun` data. `emit.rs` builds `krilla::text::Font` from the store's `Arc<Vec<u8>>` at draw (and caches on the emit stack if needed). upem check happens in `backend/` at first use or via a `backend` helper called from `FontStore::register` through a `cfg` hook that lives in backend, not fonts.rs.

- **candidate-2:** `FontStore` stays skrifa-only. Each glyph `Mark` clones `Arc<Vec<u8>>` plus `index` plus placements so `emit` never looks up a store. `KrillaCanvas` does not hold `FontStore`. `draw_glyph_run` takes fonts from a resolver argument **or** you extend `KrillaCanvas::new` with an `Arc<FontStore>` only long enough to copy bytes into the mark, then drop the dependency from emit. Unknown id fails at record time.

- **candidate-3:** Split a tiny `src/backend/fonts.rs` (allowed to `use krilla`) that wraps `krilla::text::Font` keyed by `FontId`. `FontStore::register` in `src/fonts.rs` does skrifa; then, under `backend-krilla`, a function in backend registers the same bytes into the krilla map and compares upem. `KrillaCanvas` holds `Arc` of that krilla map. emit only looks up `Font`.

## Rubric (picker only; you do not see scores)

1. Layering: no krilla in fonts/text/layout/document; emit-only Document/Page/Surface.
2. Same glyph pipeline: charmap → advance_norm → KrillaGlyph without rescaling.
3. Missing glyph is `MissingGlyph`; bad font is `Font`; unknown id is `UnknownFont`; mismatch is `FontMetricsMismatch`.
4. Tests match PR5+PR6: fixture metrics, RecordingCanvas x, mismatched TJ number, both feature configs.
5. Minimal public API: `FontId` (existing). Store methods `pub(crate)`. No DSL.
6. Direction is actually distinct, not a rename of another candidate.

## Models

- candidate-1: claude-opus-5-5-medium
- candidate-2: gpt-5.6-sol-high
- candidate-3: grok-4.7-high
