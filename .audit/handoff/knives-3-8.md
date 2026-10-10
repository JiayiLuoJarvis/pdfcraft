# Handoff: autonomous knives 3–8

**Date:** 2026-10-08  
**Repo:** /mnt/data/demo/quire  
**Crate:** quire-pdf (`publish = false`)  
**Authority:** docs/提案书.md > AGENTS.md > docs/quire-design.md (tech facts only)

## Exit predicate (still open)

Knives 3–8 each: one branch, one commit, four gates green (fmt / clippy -D / test / RUSTDOCFLAGS='-D warnings' cargo doc --no-deps). No remote/PR required.

## Done

| Knife | Branch | Commit | Notes |
| --- | --- | --- | --- |
| 1 geom+error | feat/geom-length-types | b414a45 | branded Length |
| 2 canvas | feat/canvas-recording | 20d1dca | Canvas + RecordingCanvas |
| 3 backend-krilla | feat/backend-krilla | **f082ab5** | buffered plan + emit; default feature on |

## Knife 3 shape (landed)

- Base arena candidate-2; grafts InvalidGeometry + dump lever (c1), emit split (c3).
- `KrillaCanvas` holds no live Document; `emit.rs` only constructs Document/Page/Surface.
- Identity convert; path ops in PDF stay API coords (`10 20 m`) because `1 0 0 -1 0 200 cm`; Annot `/Rect [10 126 90 170]`.
- `UnsupportedCanvasOperation` for glyphs/images; poisons finish.
- Errors: +InvalidCanvasState, UnsupportedCanvasOperation, InvalidGeometry, Backend(String); Error lost Copy.
- CI: clippy/test/doc for `--no-default-features` and `--features backend-krilla`.
- Audit (untracked): `.audit/backend-krilla/` (decisions.tsv, arena, SYNTHESIS).

## Gates proof (knife 3)

All green locally on f082ab5 for both feature configs.

## Next: knife 4 fonts/skrifa

- Branch tip ready: `feat/fonts-skrifa` @ f082ab5 (checked out).
- Pin `skrifa =0.47.0`; FontStore; measure via charmap+advance; missing glyph = whole-doc fail (§4).
- Register krilla Font when backend-krilla on; upem cross-check.
- TJ mismatch / advance_norm tests per design PR 6 — do not assert mere presence of Tj.
- Playbook: Feature (how → architect/arena → implement → gates → commit).
- Poteto: use `poteto-agent` for code delegates; models from available list (no pstack-models.mdc yet).

## Then

5 line break → 6 flow/paginate → 7 table/columns/list/decorate → 8 document DSL  
§2 DSL builders only after proposal inventory closed; placeholders OK.

## Remind

Open a **new chat** for knife 4+; point it at this file and AGENTS.md / 提案书确认总表.
