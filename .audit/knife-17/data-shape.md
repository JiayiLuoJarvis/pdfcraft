# Knife 17 data shape (cell paragraph + list)

**Authority.** `docs/提案书.md` §2.6 cell kids, §2.8 格内列表.  
**Baseline.** `feat/cell-spacer` tip (stack on current HEAD after knife 16 audit).

## Domain

Cell kids that §2.6 already named and layout already allows, but public `CellContent` cannot store:

1. Rich paragraph ink (body/bold/blank/link) via existing `Paragraph` → `TextNode::from_items` → `FlowNode::Text`.
2. List via existing `List` → `FlowNode::List`.

Still one public payload per `Cell` this knife unless arena proves a multi-kid cell is required.

## Organizing structure

| Concern | Structure | Why |
| --- | --- | --- |
| Rich text | Reuse `Paragraph` (not a second builder) | Same coalesce / blank / link rules as body |
| Stored cell arm | `CellContent::Paragraph(Paragraph)` or equivalent owned items+indent+error | Closure runs at construct like `hline` |
| Lower paragraph | `TextNode::from_items(items, flow.style with indent)` | Matches `Flow::paragraph` |
| List builder | Reuse `List` | §2.8 item body already is a Flow |
| List + Flow | **Arena fork** | `List::item` needs `Flow` today; `Table` has no child Flow |
| Forbidden kids | unchanged `check_content` | columns / pageBreak / keep_together / nested table |
| Measure | `natural_height` unchanged | Tall cell content → `UnsplittableTallerThanFrame` |

## Arena forks (product / seam)

| Call | Notes |
| --- | --- |
| List wiring | Defer list closure until `Table::lower` has `&Flow` vs give `Table` a child Flow like `Columns` vs `Cell::ul` that stores pre-lowered `Result<FlowNode>` somehow |
| Split 17a/17b | If list Flow wiring balloons, ship paragraph first |
| Cell::new string | Keep as plain `TextNode::new` shortcut |
| Multi-kid cell | Default out (`stack-cell`) |
