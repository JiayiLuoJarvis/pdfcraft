# Knife 7 arena frame — table / columns / list / decorate

## Artifact

One design package per candidate under `.audit/knife-7/arena/candidate-N/design.md`:

- Caller usage (internal test call sites only; no public DSL builders)
- Core types and signatures (`not implemented` ok)
- Module map
- Rationale (usage → types, conflicts resolved by 提案书)

## Baseline

`feat/flow-paginate` @ `a6ff61e`. Extend consume-self `FlowNode::split` → `Split::{Fit,Partial,MoveToNextPage}`. Plan-then-paint. No public `document` builders.

## Authority (提案书 wins)

| Topic | Confirmed rule | Design draft conflict |
| --- | --- | --- |
| columns | Allow cross-page: each column splits on remain height; head this page, tail next-page Columns; top align; W = column width (§2.1/§2.5) | Design “列表与并排栏” says unsplittable max(natural_height) — **reject** |
| table | Row atomic split; repeat header default on; Fixed/Ratio/≤1 Auto; cell Paragraph+Spacer (+list when list lands); forbid columns/pageBreak/keep_together/nested table; InvalidCellContent; **colSpan/rowSpan required** (§2.6) | Design Non-Goals / table v1 “没有单元格合并” — **reject; implement merge** |
| list | Enum markers (decimal/A/a/roman) + per-item custom string; hanging indent; marker first fragment only; nest; no table/columns in item (§2.8) | Design only ASCII `1.` and `•` — **extend to proposal** |
| decorate | Band height = margin.top/bottom; full page width (ignore L/R margin); PageNumber/PageCount; overflow → error; body breakpoints unchanged (§2.10) | Design three-slot with body.x — slots OK as internal shape if avail = **page width** and band = margin; do not shrink body |

## Landed invariants to keep

- `Frame { width, height, full_height }` height may be 0
- `FragmentCommand` currently Glyphs only; add Stroke (and only what borders need) when table draws
- `spacing_after` dropped when no room
- Prepared KeepTogether
- PageCursor three-state
- Tests prefer RecordingCanvas coordinates
- Errors: add `ColumnWidths`, `InvalidCellContent`, `DecorationTallerThanMargin`, `DecorationWiderThanFrame` as needed (public `Error`)

## Out of scope (knife 7)

- Public DSL builders (knife 8)
- image / qr / svg / toc / flow canvas / UAX#14 / bidi / shaping
- Second PDF backend

## Rubric (picker)

1. **Proposal fidelity** — columns splittable; table merge + repeat header; list hanging + enum/custom; decorate band=margin full width
2. **Protocol fit** — new nodes are `FlowNode` variants (or thin wrappers) using existing Split; decorate post-plan does not re-split body
3. **Domain modeling** — ColumnWidth / cell content / list marker / page inline as types, not stringly bools
4. **Interface depth** — small internal surface; complexity inside split/resolve helpers
5. **Testability** — concrete RecordingCanvas assertions named; no krilla content-stream asserts
6. **Laziness** — no public Block enum / DSL; no unused abstractions for knife 8

## Success criteria (implementation later)

Gates (both feature sets): fmt check, clippy -D, test, RUSTDOCFLAGS=-D warnings doc --no-deps.
Behavior tests as above.
