## Problem

Cell content is a private one-slot enum with its own constructors. Body content is `FlowWriter` sugar over `Paragraph::build`, `HLine::build`, `PlacedImage::build`, and `List::build_on`, admitted by `add(impl IntoFlowItem)`. Layout cells already store `Vec<FlowNode>` and wrap it in a stack at measure time. Proposal §2.6 allows several of paragraph, line, spacer, image, and list, and forbids columns, pageBreak, keep-together, and a nested table. The non-obvious part is sharing those build results without giving `Cell` the body method set. `IntoFlowItem` already includes `Table` and `Columns`. `page_break`, `keep_together`, and `stack` are not values; they write `flow_mut`. A free `Cell::ul` still has no parent flow, and the list closure must run inside `row` / `header` so it can borrow the table closure (`Cell<'a>`, `cell_list_can_borrow_a_table_local`). `Cell::new` / `From<&str>` must keep `TextNode::new`, because `lower_paragraph` overwrites parent alignment.

## Usage (caller's view)

Everyday rows stay `Cell::paragraph`, `Cell::ul`, `Cell::image`, `Cell::hline`, `Cell::spacer`, `Cell::new`, and `"Qty"`, then `.col_span` / `.fill`. A helper returns `Paragraph`, `HLine`, `PlacedImage`, `Spacer`, or `ListBlock` and both `doc.add(...)` and `Cell::empty().add(...)` take it. Several children are several `.add`s, top to bottom, then the span chain. `Cell::ul(f)` means `Cell::empty().add(ListBlock::bullet(f))`. `doc.ul(f)` means `doc.add(ListBlock::bullet(f))`. `cell.add(table)`, `cell.page_break()`, and `cell.columns(...)` do not compile. The type sketch in [Shape](#shape) is that usage. `src/document/table.rs` implements it.

## Shape

`Cell<'a>` owns `Vec<Piece<ListBlock<'a>>>` plus span, fill, and `ErrorBag`. `add(self, impl IntoCellItem<'a>) -> Self` appends and returns the cell so it chains like `col_span`. The trait is sealed in `table.rs`. Impls are `Paragraph`, `HLine`, `PlacedImage`, `Spacer`, and `ListBlock` only. `IntoFlowItem` keeps a separate seal in `flow_writer.rs`, so `Table` and `Columns` cannot grow a cell impl by accident (per encode-lessons-in-structure). Sugars are one call to `add` of the existing builder. `Cell::new` pushes `Piece::Text` and is not an `IntoCellItem`.

`Piece<L>` is the only child list. `map_list` is the only converter: store replaces `ListBlock` with `Option<FlowNode>`; every other variant is carried. `BuiltList` is crate-private, built by `List::build_on` once, and is not admissible. `ListBlock::resolve(&table.parent)` is what both `IntoFlowItem::add_to` and `Table::store` call. Body runs it inside `add`. Cell runs it at store, which is the last moment the table closure's borrows are alive and the first moment the parent flow exists. One `child()` snapshot, inside `build_on`, matching today's `assemble_list(parent.child())`. `assemble_list` goes away. Kind conversion and the `list.start` overwrite live only in `BuiltList::lower`.

`Spacer::into_result` is the length check (per boundary-discipline). The flow reports `Err` through `push_err`, so a rejecting flow still surfaces `ContentOutsideSection`. The cell records `Err` on `cell.errors` and stores no piece. Paragraph, line, and image errors still surface in `Table::lower`, after every row has stored, so list errors stay ahead of paragraph errors. A `None` piece fails that row-set after the other pieces lower. Empty content with no error is a zero-height slot. Measure still uses `CellBody::into_node`, including for one child.

`add` on a cell takes `self` by value because the cell is a temporary. `FlowWriter::add` stays `&mut self` because the document is already borrowed. Not one trait, and not a `ContentWriter` supertrait: one `add` cannot be bounded by both the wide set and the narrow set.

What callers see: the old sugars, `empty` / `add`, and three value names (`Spacer`, `ListBlock`, and the trait). What they do not see: `FlowNode`, the piece enum, `BuiltList`, store versus lower, or a reject match. Forbidden flow control is absent, so there is no error variant to learn for `cell.page_break()`. Interface depth is that omission plus deferred list binding. The public surface does not grow a writer with the body method list.

`add` twice means two children. That is the operation, not a retry (per make-operations-idempotent, append is not idempotent). Store consumes the cell once. No shared writer: the cell owns the pieces until `store` moves them (per separate-before-serializing-shared-state, there is no second writer).

## Synthesis decision

TBD (arena).

## Tradeoffs accepted

- We accept two seals and two traits in exchange for a cell allow list that cannot name `Table` or `Columns`.
- We accept `ListBlock` and `Spacer` as public values in exchange for one constructor each, used by body `add` and cell `add`. Body `ul` / `ol` / `spacer` become `add` of those values; their behavior does not change.
- We accept list resolution at `store` and paragraph lowering at `Table::lower` in exchange for the existing borrow and the existing error order.
- We accept `Cell::new` as a text piece outside `IntoCellItem` in exchange for not rewriting parent alignment on `t.row(["Qty"])`.
- We accept a content-less `Cell::empty()` placed in a row as a zero-height slot in exchange for a factory seed. It is not `InvalidCellSpan::EmptySlot`.
- We accept no in-cell `stack`. Parent `stack` around the table, and paragraph `color` / `font_size`, are the style knobs. §2.6's allow list does not include stack.
- We accept that a list item can still call `item.table()` and fail at runtime with `container: "list item"`. Narrowing `FlowWriter` on items would change body lists.
- We accept a one-line sugar beside `add` of the same value, the split the body already teaches. The sugar's body is the `add` call, so it cannot grow a second lower path.
- We accept `Piece<L>`'s `map_list` match plus the lower match. One enum definition; a new variant fails the build in both matches. The lifetime seam is the list payload, not a hand-copied `CellContent` / `StoredContent`.

## Alternatives considered

- **`Cell: FlowWriter` over an internal flow (Pole A).** The trait's method set is the whole body, including `columns`, `table`, `page_break`, `keep_together`, and `stack`. An impl cannot delete methods. Illegal calls would be real methods that return `InvalidCellContent`, which is an accidentally valid call site until a test runs. A second trait that copies the safe methods is the narrow admission anyway, plus a flow the free cell does not have. Building that flow detached uses the wrong style and `DocToken`. Building it only inside `row(|cell| ...)` drops `t.row([Cell::new("Total").col_span(n)])`.
- **`cell.add(impl IntoFlowItem)` with a reject arm for `Table` and `Columns`.** Same failure mode: `cell.add(table)` compiles. The forbid list is a match that must be kept next to the proposal. `page_break` still would not be a value, so the reject arm cannot see it; the method would exist only if `Cell` were a `FlowWriter`. The trait approach makes that method absent.
- **A public `ContentWriter` supertrait of `FlowWriter`.** `FlowWriter::add` and a narrower `add` cannot share a name. Two adds on `Document` is two ways to append a paragraph. Default methods would re-export cell policy onto the document or document policy onto the cell.
- **Keep one child and let `add` replace it.** Layout already has a vector, and §2.6 does not require one child. A replacing `add` throws away the previous value. Repeated `add` matches paint order.
- **Eager `List::build_on` inside `Cell::ul` on a detached flow.** The closure can run that early and still borrow the local, but the parent style and document token would not be the table's. Images in the list would not see the document registry. Store is the binding point that keeps both the borrow and the token.

## Open questions and risks

- A `ListBlock` added through `Table::build` (detached token) still resolves against that token. Images inside the list can report `UnknownImage`. Is a rustdoc warning enough, as it is for today's detached `Cell::image`?
- Cross-row error order (a later row's list error before an earlier row's paragraph error) is preserved because stores all run before `Table::lower`. No current test locks that pair. Should a later cleanup lower lists with paragraphs, accepting `'static` or a behavior change?
- Default multi-item gap is the stack's `spacing_after` (zero at the default style). Should a cell insert a different gap than a body stack of the same nodes?

## Next implementation step

Replace `CellContent` / `StoredContent` / `assemble_list` with `Piece<L>`, `IntoCellItem`, and `ListBlock::resolve`, leaving `Cell::paragraph` / `ul` / `new` / span chains as one-line wrappers and without editing `src/layout/nesting.rs`.
