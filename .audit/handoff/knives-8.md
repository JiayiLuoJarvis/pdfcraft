# Handoff: autonomous knife 8 (document DSL)

**Date:** 2026-10-08  
**Repo:** /mnt/data/demo/quire  
**Crate:** quire-pdf (`publish = false`)  
**Authority:** docs/提案书.md > AGENTS.md > docs/quire-design.md (tech facts only)

## Exit predicate (still open for knife 8)

Knife 8: one branch, one commit, four gates ×2 green (fmt / clippy -D / test / RUSTDOCFLAGS='-D warnings' cargo doc --no-deps). No remote/PR required.

## Done

| Knife | Branch | Commit | Notes |
| --- | --- | --- | --- |
| 1–5 | (prior) | … | geom → line-break |
| 6 flow/paginate | feat/flow-paginate | a6ff61e | FlowNode split + PageCursor |
| 7 table/columns/list/decorate | feat/table-columns-list-decorate | **6dbf40c** | Columns/Table/List + margin-band decorate |

## Knife 7 shape (landed @ 6dbf40c)

- `FlowNode::{Text, Columns, Table, List}` + existing Paragraph/Spacer/PageBreak/KeepTogether/Stack
- Lazy `TextNode` wraps at `avail.width`; `ParagraphNode` width guard
- `FragmentCommand::{Glyphs, Stroke}`; `with_offset(dx, dy)`
- Decorate: plan → `measure_bands` → paint header→body→footer; not on `PagePlan`
- `CellBody` / `ItemBody` via `check_content`; `InvalidCellSpan`, `ColumnWidths`, decoration errors
- Tests: 100 no-default / 118 +1 ignored with backend-krilla
- Audit (untracked): `.audit/knife-7/`

## Next: knife 8 document DSL

- Branch: `feat/document-dsl` from **6dbf40c**.
- **Design pick:** base candidate-1 (live lower to `FlowNode`). Synthesis: `.audit/knife-8/arena/synthesis/design.md`. Cross-judge: `.audit/knife-8/arena/synthesis/cross-judge.md`.
- Defaults locked: header L/R slots inset by margins (`full-bleed-slots` reverses); no `p.bold`; no style builders; `geom::A4` not PageSize enum; behind/front `text_at` only; recursive Band+Columns check.
- Wire §1-shaped builders. Do **not** invent style/track/Paper/`blocks()` public AST.
- `publish = false` stays.
- Prefer a **new chat** with this file + AGENTS.md + 提案书确认总表 + synthesis.

## Gates

Same seven commands as knife 7 (fmt check, clippy ×2, test ×2, doc ×2).

## Remind

Knives 7 design+implement context is large. Open a new chat for knife 8+ with this handoff.
