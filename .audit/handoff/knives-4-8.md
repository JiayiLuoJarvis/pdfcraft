# Handoff: autonomous knives 4–8

**Date:** 2026-10-08  
**Repo:** /mnt/data/demo/quire  
**Crate:** quire-pdf (`publish = false`)  
**Authority:** docs/提案书.md > AGENTS.md > docs/quire-design.md (tech facts only)

## Exit predicate (still open)

Knives 5–8 each: one branch, one commit, four gates green (fmt / clippy -D / test / RUSTDOCFLAGS='-D warnings' cargo doc --no-deps) for both `--no-default-features` and `--features backend-krilla`. No remote/PR required.

## Done

| Knife | Branch | Commit | Notes |
| --- | --- | --- | --- |
| 1 geom+error | feat/geom-length-types | b414a45 | branded Length |
| 2 canvas | feat/canvas-recording | 20d1dca | Canvas + RecordingCanvas |
| 3 backend-krilla | feat/backend-krilla | f082ab5 | buffered plan + emit |
| 4 fonts/skrifa | feat/fonts-skrifa | debb997, tip **9392f85** | FontStore + measure + draw_glyphs; tip drops emit Font cache, parses TJ |

## Knife 4 shape (landed)

- Pin `skrifa =0.47.0` always. `FontStore` is skrifa-only. No krilla in `fonts` / `text`.
- `Mark::Glyphs` clones `Arc<Vec<u8>>` at record time. `emit.rs` builds `krilla::text::Font`, `check_upem`, `KrillaGlyph` with unscaled `advance_norm`, `draw_glyphs(..., outlined: false)`.
- `KrillaCanvas::new()` keeps an empty store. Glyph tests use `new_with_fonts` / `new_observable_with_fonts`.
- Errors: `Font`, `MissingGlyph`, `UnknownFont`, `FontMetricsMismatch`.
- Fixture: `tests/fixtures/quire-measure.ttf` (QuireMeasure, OFL subset of Source Han Serif ExtraLight). Pins: `A` advance 707 fu, `第` 1000 fu, upem 1000, descent < 0.
- TJ test: two glyphs, first `advance_norm + 0.25`, stream contains `-250`. Do not assert `Tj`.
- Measure is per Unicode scalar. No unicode-segmentation / line break this knife.
- Audit (untracked): `.audit/fonts-skrifa/` (how, arena C2 base, decisions.tsv).

## Gates proof (knife 4)

All green locally for both feature configs before commit.

## Next: knife 5 line break

- Branch from tip of feat/fonts-skrifa after its commit.
- Pin `unicode-segmentation =1.13.3`.
- `cluster` / `break_class` / `line_break` per design PR 7 + 提案书 §3.
- Monospace stub for break tests; real skrifa fixture already present.
- Do not invent document DSL. Do not paginate.

## Then

6 flow/paginate → 7 table/columns/list/decorate → 8 document DSL  
§2 DSL builders only after proposal inventory closed; placeholders OK. Knife 8 wires confirmed parts only.

## Remind

Open a **new chat** for knife 5+. Point it at this file, AGENTS.md, and 提案书确认总表.
