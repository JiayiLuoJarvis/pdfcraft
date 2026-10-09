# Handoff: knives 7–12 complete

**Date:** 2026-10-09  
**Repo:** /mnt/data/demo/quire  
**Exit predicate:** met locally (no remote/PR).

| Knife | Branch | Commit |
| --- | --- | --- |
| 7 table/columns/list/decorate | feat/table-columns-list-decorate | 6dbf40c |
| 8 document DSL | feat/document-dsl | 7cbe823 |
| 9 image | feat/image | a12b610 |
| 10 section | feat/section | 9eda8da |
| 11 layer path | feat/layer-path | 7cb1654 |
| 12 flow hline | feat/hline | *(tip after commit)* |

Knife 12 tip stacks on `7cb1654`. Audit under `.audit/knife-12/` (untracked unless committed).

## What callers get (knife 12)

`Flow::hline(|h| …)` and `Cell::hline(|h| …)` append a §2.3 block-level horizontal rule. Defaults are full current-frame width, centered when shorter, 1 pt black solid, zero `space_before` / `space_after`. Optional width, thickness (`Positive`), color, spaces (`NonNegative`), and `start` / `end` alignment. The node is unbreakable (same split ladder as image). List items and columns reach it through nested `Flow`. Not behind/front path, not §2.2 blank, not flow canvas.

## Gates (parent re-verified 2026-10-09)

Seven commands green on `feat/hline` after hline landed. With `backend-krilla`: 161 lib + 1 ignored, 36 integration.

## Next (optional)

- style builders / `p.bold` / first-line indent
- §2.2 fill-in underscore
- flow canvas (§2.14)
- Stack/rebase onto master if desired
- Open PR when remote/token available
