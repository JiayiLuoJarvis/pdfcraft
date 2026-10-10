# Handoff: autonomous knives 5–8

**Date:** 2026-10-08  
**Repo:** /mnt/data/demo/quire  
**Crate:** quire-pdf (`publish = false`)  
**Authority:** docs/提案书.md > AGENTS.md > docs/quire-design.md (tech facts only)

## Exit predicate (still open)

Knives 6–8 each: one branch, one commit, four gates green (fmt / clippy -D / test / RUSTDOCFLAGS='-D warnings' cargo doc --no-deps) for both `--no-default-features` and `--features backend-krilla`. No remote/PR required.

## Done

| Knife | Branch | Commit | Notes |
| --- | --- | --- | --- |
| 1 geom+error | feat/geom-length-types | b414a45 | branded Length |
| 2 canvas | feat/canvas-recording | 20d1dca | Canvas + RecordingCanvas |
| 3 backend-krilla | feat/backend-krilla | f082ab5 | buffered plan + emit |
| 4 fonts/skrifa | feat/fonts-skrifa | debb997, tip **9392f85** | FontStore + measure + draw_glyphs |
| 5 line break | feat/line-break | **b9de080** | ClusteredText + PairClass + break_lines |

## Knife 5 shape (landed)

- Pin `unicode-segmentation = "=1.13.3"`.
- `BreakClass::{Hard, Pair(PairClass)}`; const 6×6 `ALLOW`; `classify` / `allows` (BK cannot enter matrix).
- `ClusteredText::new`: CRLF→`\n`, UAX #29 graphemes, `glyph_text()` omits BK for measure.
- `break_lines(&ClusteredText, avail, FnMut(Cluster)->f32)`: steps 2–9, leading U+0020 drop, left backtrack, `UnbreakableWiderThanFrame`.
- Monospace stub via callback (`scalar_count` as width). `measure.rs` unchanged.
- No justify / kerning / bidi / pagination / DSL.
- Audit (untracked): `.audit/line-break/` (how, arena cand1–4, cross-judge picked cand2, decisions.tsv).

## Gates proof (knife 5)

All green locally for both feature configs before commit `b9de080`.

## Next: knife 6 flow/paginate

- Branch from tip of `feat/line-break` @ `b9de080`.
- Design PR8 paragraph flow + PR9 paginate (提案书 §2.1 W = current frame; §2 keep_together / pageBreak / spacer).
- Prefer `RecordingCanvas` assertions. Do not invent §2-unconfirmed DSL builders.
- Wire `ClusteredText` + `break_lines` + existing measure; Start/Center/End alignment after break.

## Then

7 table/columns/list/decorate → 8 document DSL (confirmed parts only; §2 inventory closed for wired items, no invented builders).

## Remind

Open a **new chat** for knife 6+. Point it at this file, AGENTS.md, and 提案书确认总表.
