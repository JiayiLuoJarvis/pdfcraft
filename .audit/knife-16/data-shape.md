# Knife 16 data shape (Cell::spacer)

**Authority.** `docs/提案书.md` §2.6 cell kids, §2.13 spacer (格内允许).  
**Baseline.** `feat/text-uri-link` feature tip `16749da` (audit tip may sit above; cut branch from feature tip).

## Domain object

A cell spacer is the same domain object as a flow spacer: a **non-negative vertical blank** with no paint commands. It is not a new layout node kind. The public gap is only that `document::Cell` cannot construct one yet.

```text
Public assemble:
  CellContent::Spacer(Length<NonNegative>)
  Cell::spacer(height) -> Cell

Lower (Table::lower):
  CellContent::Spacer(h) -> vec![FlowNode::Spacer(SpacerNode { height: h })]

Layout (already):
  CellBody::new → check_content(TableCell) allows FlowNode::Spacer
  SpacerNode::split → empty Fragment of height min(h, avail)
```

## Organizing structure

| Concern | Structure | Why |
| --- | --- | --- |
| Non-negative height | reuse `Length<NonNegative>` | §2.0 / §2.13; same as `Flow::spacer` |
| Public cell payload | add `CellContent::Spacer(...)` beside Text / Image / HLine | Discriminated union already owns cell kids |
| Layout body | keep `Vec<FlowNode>` inside `CellBody` | Matrix already allows Spacer; no new allowlist |
| Multi-kid cell | out of knife unless arena proves one kid is insufficient | Today each public cell holds one `CellContent` |
| Cross-page | table row stays atomic; spacer height feeds `natural_height` | Flow spacer split is for body flow, not mid-row |

## Invariants

1. Height is non-negative at the type boundary (same as flow spacer).
2. Spacer paints nothing; tests assert glyph/stroke absence and row geometry via neighbor ink or page count.
3. Forbidden cell kids stay forbidden (columns, pageBreak, keep_together, nested table).
4. No Cell rich paragraph / list in this knife.

## Open product calls (arena)

| Call | Autonomy default | Reverse |
| --- | --- | --- |
| API name | `Cell::spacer(height)` mirroring `Flow::spacer` | `space` |
| Zero height | allow (`NonNegative::ZERO`), Fit height 0 | `reject-zero` |
| Multi-content cell | one content per Cell this knife | `stack-cell` |
| Cross-page proof | assert spacer grows row / pushes next row; not mid-cell split | `cell-split` |
