# Knife 8 synthesis

## Pick

**Base:** candidate-1 (builder lowers live to `FlowNode`; `Decorations` hold page-time hooks).

**Why.** Agrees with [cross-judge](cross-judge.md) (24 vs 22). Closest to 提案书 §1 (non-`'static` header borrow, behind/front). C2 freezes a public `Block` mirror of `FlowNode` and invents style/track/paper builders frame forbade. C2's zero layout churn is valuable; graft tests and section page fields, keep most layout edits.

## Grafts from candidate-2

1. Fail-with-zero-pages assertions (missing glyph, decoration overflow).
2. `Page` carries section counters equal to document totals for the single-section sugar.
3. Table spans use `NonZeroU16` at the DSL boundary where spans are set.
4. Test that body first text node is lazy `Text` (via layout behavior: wrap at column width), not a pre-wrapped paragraph.
5. Prefer `pt_near` where bit-identity is brittle across decorate.

## Defaults (reverse word in parentheses)

| Call | Default | Reverse |
| --- | --- | --- |
| Header slot indent | Left/right string slots inset by `margin.left` / `margin.right` so they line up with the body. Full page band height unchanged. Center still page-centered. | `full-bleed-slots` |
| `p.bold` | Omit. Single-style only. | knife later / rich text |
| Style builders | Defaults only (first face, 12 pt, factor 1.2, black, spacing 0, Start). No public size/align setters this knife. | `style-builders` |
| Paper | Reuse `geom::Size` / `A4` / `LETTER`. No `PageSize` enum until A-series list is pinned. | `page-size-enum` |
| behind/front | `text_at` only. No `image_at`. | image knife |
| Band Columns | Allow `Columns` in bands only if every descendant also passes Band rules (recursive). No nested table in a header column. | (required graft) |
| Replay to PDF | Layout into `RecordingCanvas`, then replay commands into `KrillaCanvas` when `to_pdf` is called. Add a replay-identity test into a second `RecordingCanvas`. | |

## Knife-7 edits (allowed)

1. `Decorations` / `BandContent` gain lifetime `'a`.
2. behind/front layers; paint order behind → header → body → footer → front.
3. Band content matrix allows Columns with recursive Band checks.
4. `paginate_root` signature follows lifetime.
5. Replay helper for RecordingCanvas → KrillaCanvas (backend feature).

Do not change `split` / row algorithms.

## Module map

```text
src/document.rs          document(), Document, Flow, builders, Rendered
src/lib.rs               re-export document surface
src/layout/decorate.rs   lifetime + behind/front
src/layout/paginate.rs   paint order + lifetime
src/layout/lower.rs      Band+Columns recursive check
tests/document.rs        public API integration tests
```

## Success criteria

- §1-shaped call sites compile (minus bold/image_at/PageSize enum).
- RecordingCanvas tests for paragraph, columns, table, list, header page numbers, behind/front order.
- `to_pdf` under backend-krilla asserts `%PDF`.
- Four gate pairs green. One commit on `feat/document-dsl`.

## Next step

Implement `document` + `Flow` + live lower for paragraph/spacer/page_break/keep_together, then columns/table/list, then decorations with lifetime + behind/front, then `to_pdf` replay.
