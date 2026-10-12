# Cell ↔ FlowWriter grounding (orchestrator notes)

Filled before / alongside the how explainer. Facts from code on branch `feat/flow-writer` @ `c4a5831`.

## Confirmed product / API direction (do not reopen)

- Body inflow is `FlowWriter` + public `add` + sugar over `El::build` / `IntoFlowItem`.
- Cell entry must share that model (sugar + independent element values), not a parallel constructor zoo.
- Proposal §2.6 cell children: allow paragraph (incl. blanks), hline, spacer; image/list as landed. Forbid `columns`, `pageBreak`, `keep_together`, nested `table` (for now).
- 0.x may break; theme-internal rigor over compatibility patches.
- Do not touch limits / letter fixtures / deferred proposal items.

## Current shape

### Body path (`flow_writer.rs`)

- `IntoFlowItem: seal::Sealed` with `add_to(&mut Flow)` for `Paragraph`, `HLine`, `Table`, `Columns`, `PlacedImage`, private `BuiltList`.
- `FlowWriter::add` → `item.add_to(flow_mut())`.
- Sugars build then `add`, except `spacer` / `page_break` / `keep_together` / `stack` which write `flow_mut` directly.
- `List` / `Table` / `Columns` use `build_on(parent_flow)` so nested style/doc token match the live flow. Detached `Table::build` / `Paragraph::build` exist for factory + `add`.

### Cell path (`table.rs`)

- `Cell` is a **single** `CellContent` plus span/fill/`ErrorBag`.
- Constructors: `new`/`From<&str>`, `paragraph`, `ul`/`ol` (deferred `FnOnce` until `Table::store`), `image`, `hline`, `spacer`.
- `Table::store` absorbs cell errors, eagerly assembles deferred lists via `assemble_list(parent.child(), …)`, stores `StoredContent`.
- `lower_stored_rows` turns each stored item into `Vec<FlowNode>` (usually length 1) for `layout_table::Cell`.
- Layout cells already accept **multiple** nodes; DSL cells only ever supply one content variant today.

## Touch points for alignment

- Replace or rewire `Cell::*` content constructors to share `Paragraph::build` / `HLine::build` / `PlacedImage::build` / list build path with `FlowWriter`.
- Decide admission: reuse `IntoFlowItem` with reject-forbidden, or a narrower sealed trait, or `Cell: FlowWriter` over an internal flow.
- Preserve `col_span` / `row_span` / `fill` and string `From` shorthand.
- Preserve deferred-list borrow-until-row semantics or prove an eager path that keeps the same call sites working.
- Update tests that call `Cell::paragraph` / `Cell::ul` / …; CHANGELOG breaking note; mark follow-up done in `flow-writer-spec.md` / `deref-flow-api.md`.

## Open factual questions for designs (not product forks)

1. Multi-item cell via repeated `add` (matches layout `Vec<FlowNode>`) vs keep single-item Cell?
2. Should forbidden sugars exist on Cell and error, or be absent from the type?
3. Does `Cell` implement `FlowWriter`, a subset trait, or only `add(IntoCellItem)`?
