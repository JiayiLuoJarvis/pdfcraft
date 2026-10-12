# Rationale (synthesized)

## Problem

Cell content used a private one-slot enum and six constructors. Body content uses `FlowWriter` sugar over shared builders and `add(impl IntoFlowItem)`. Align cell entry with that model without handing cells the full body method set, so §2.6 forbids stay type-absent when `FlowWriter` grows.

## Usage (caller's view)

Everyday rows keep `Cell::paragraph`, `Cell::ul`, `Cell::new`, string rows, and span/fill chains. Helpers return `Paragraph`, `HLine`, `PlacedImage`, `Spacer`, or `ListBlock`. Both `doc.add(...)` and `Cell::empty().add(...)` take those values. Several children are several `.add`s. `cell.add(table)` and `cell.page_break()` do not compile.

## Shape

Sealed `IntoCellItem` in `table.rs`. `Piece<L>` holds children. Sugars are `empty().add(shared value)`. Lists defer as `ListBlock` until `store`. Non-list lower replays `IntoFlowItem::add_to` on a table child flow (graft). Layout matrix unchanged.

## Synthesis decision

Base **Candidate 2**. Raw score favored C1 by one point on shared construction, but C1 makes every future body sugar a cell call site and relies on runtime `InvalidCellContent` for §2.6 forbids. C2's seal keeps the allow list in `table.rs`. Graft from C1: replay `IntoFlowItem` for non-list lower, hard sugar/`add` parity test, preserve span sums on content failure. Cross-judge [93fdce7b](../synthesis/cross-judge.md) recommended the same base.

## Tradeoffs accepted

- Two seals (`IntoFlowItem` vs `IntoCellItem`) instead of one writer type.
- Public `Spacer` and `ListBlock` so body and cell share values.
- No in-cell `stack` method. Style via parent `stack` around the table or paragraph props.
- Generic `fn emit(w: &mut impl FlowWriter)` cannot target a cell. Shared values are the bridge.

## Alternatives considered

- **C1 `Table::cell` + `&mut Flow`.** Lost on forbid-surface growth when `FlowWriter` expands.
- **`cell.add(impl IntoFlowItem)` with reject arms.** Compiles illegal tables/columns into the cell path.
- **`ContentWriter` supertrait.** One `add` cannot be bounded by both wide and narrow sets without two entry points on `Document`.

## Open questions and risks

Resolved by defaults in `design.md` (detached token, error order, multi-item gap, empty cell).

## Next implementation step

Replace `CellContent` / `StoredContent` / `DeferredList` / `assemble_list` with `Piece`, `IntoCellItem`, `ListBlock`, and `Spacer` in the main checkout. Keep sugar signatures. Land tests and docs in the same change set. No git commit unless asked.
