# How: Cell vs FlowWriter / IntoFlowItem / add

Source: how explainer (agent 8f9e59d3-59fb-47dd-9898-d19dc6ab6c39), lightly filed.

## Overview

Table cells and body content use two separate build paths today.

- Body: `FlowWriter` sugars build independent element values, then `add(impl IntoFlowItem)`. `add_to` lowers and `Flow::push`es.
- Cell: free constructors produce `Cell<'a>` with a private single-item `CellContent`. `Table::store` turns that into `StoredContent`. `lower_stored_rows` wraps each item as `vec![node]` for `layout_table::Cell::new(Vec<FlowNode>)`.

Layout cells already accept multiple nodes. The document DSL forces one content item. Both paths eventually share lower helpers (`lower_paragraph`, `HLine::lower`, `ImageBox::lower`, `List::lower`), but admission types, enums, and lower timing differ.

## Key Concepts

See orchestrator grounding in `grounding-notes.md` plus explainer detail: `Flow`, `FlowWriter`, `IntoFlowItem`, `Cell` / `CellContent` / `DeferredList`, `StoredCell` / `StoredContent`, `layout_table::Cell`, `nesting::check_content` (TableCell matrix).

## How It Works

### Body inflow

1. Parent-free elements (`paragraph`, `hline`, `image`) → `El::build` → `add`.
2. Parent-tied elements (`columns`, `table`, `ul`, `ol`) → `build_on(flow_mut())` → `add`.
3. Flow-control sugars (`spacer`, `page_break`, `keep_together`, `stack`) write `flow_mut` directly.
4. Factory path: `Table::build` on a detached flow for `doc.add(...)`.

### Cell inflow today

1. Construct: `new` / `From<&str>`, eager `paragraph` / `image` / `hline` / `spacer`, deferred `ul` / `ol`.
2. `row` / `header` → `store`: absorb cell errors; assemble deferred lists with `assemble_list(parent.child(), …)`.
3. `Table::lower` → `lower_stored_rows`: lower remaining content with table parent style/doc; wrap as `vec![node]`.

Lists defer because free `Cell::ul` has no parent flow. Lifetime `'a` exists only for `DeferredList`.

### Product constraints (提案书 §2.6)

Allow paragraph, hline, spacer, image, list. Forbid columns, pageBreak, keep_together, nested table. Proposal does **not** require single-child cells.

### Alignment touch points

`CellContent` / `StoredContent` vs `IntoFlowItem` impls; `store` + `lower_stored_rows`; parent-flow context for lists; duplicate list-kind merging; public `Cell<'a>`, string `From`, span/fill chains; CHANGELOG + spec status. Layout matrix can stay.

## Gotchas

- Type-level forbid today becomes runtime `InvalidCellContent` if Cell gains full `FlowWriter`.
- Paragraph style in cells uses table-build parent style, not the eventual target flow.
- Detached `Table::build` + `Cell::image` may hit `UnknownImage` (inferred).
- List lowers at `store`; other content at `Table::lower`.
- Body spacer splits across pages; cell spacer does not.
- `Cell::paragraph` / `Cell::image` do not reuse `Paragraph::build` / `PlacedImage` today.
