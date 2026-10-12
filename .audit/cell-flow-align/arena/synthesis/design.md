# Synthesis: Cell admission via `IntoCellItem`

Base: Candidate 2 (Pole B), per [cross-judge](cross-judge.md) and orchestrator agreement.
Grafts from Candidate 1 listed below.
Branch target: `feat/flow-writer` working tree (main checkout). Arena worktrees are design-only.

## Synthesis decision

Pick **C2**. Cell stays a value for `row` / `header`. Content enters through sealed `IntoCellItem` and `Cell::add`, reusing `Paragraph::build`, `HLine::build`, `PlacedImage::build`, plus new shared values `Spacer` and `ListBlock`. Body `ul` / `ol` / `spacer` admit those same values. `Cell` does **not** implement `FlowWriter`. Forbidden §2.6 children are absent from the cell type and from the cell seal.

Reject C1 as base. Handing `&mut Flow` into a cell makes every future `FlowWriter` sugar a cell call site. List items already accept that tradeoff because they are flows. Cells must not widen §2.6 when the body API grows.

## Grafts from C1

1. **Lower non-list pieces by replaying `IntoFlowItem::add_to`.** In `Table::lower` (or helper), for each stored non-list piece create `table.parent.child()`, call the piece's existing `IntoFlowItem::add_to`, take `into_parts()`. Do not keep a hand-written parallel `lower_piece` match that reimplements `lower_paragraph` / `HLine::lower` / image lower. Lists still resolve at `store` via `ListBlock::resolve` → `BuiltList::lower`.
2. **Hard test `cell_add_matches_sugar`.** For paragraph, hline, image, spacer, and bullet list: sugar path and `empty().add(build…)` must yield identical `RecordingCanvas` pages (or equivalent stable node dump already used in the crate).
3. **Failed content must not change column span sums.** Keep today's span-sum / track checks. A failed lower clears the table node (`None`) without dropping the cell from the row's span accounting in a way that invents `ColumnWidths` / `EmptySlot`.

## Must-fix before land (from cross-judge §5)

- Homogeneous `row` / `header` arrays in docs and examples. No new `From` to mix `&str` with `Cell` in one array beyond what already compiles today (`Into<Cell>` for strings already exists, so `["a", "b"]` stays; mixing `"Qty"` with `Cell::paragraph` in one literal array is fine today via `Into<Cell>`). Fix C2's *prose* if it claimed something that does not compile. Do not add a second string door.
- `BuiltList` not `IntoCellItem`. `ListBlock::resolve` must not double-`child()`.
- After body `ul` / `ol` / `spacer` rewire: rejecting-flow bad spacer still `ContentOutsideSection`; ordered `start` merge matches today's `assemble_list`.
- No `IntoCellItem` for `Table` / `Columns` / `BuiltList`. No `page_break` / `keep_together` / `stack` / `columns` / `table` on `Cell`.

## Shape (contract for implementer)

Follow C2 `design.md` for types and module map, with grafts above. Summary:

| Item | Contract |
| --- | --- |
| `IntoCellItem<'a>` | Sealed in `table.rs`. Impls: `Paragraph`, `HLine`, `PlacedImage`, `Spacer`, `ListBlock` |
| `Cell::add(self, item) -> Self` | Append piece; chain with span/fill |
| Sugars | `empty().add(...)` one-liners; keep public signatures |
| `Cell::new` / `From<&str>` | `Piece::Text` via `Flow::text` at lower; not through `Paragraph::build` |
| `Spacer`, `ListBlock` | Public; also `IntoFlowItem` |
| `BuiltList` | `pub(crate)`; body lists go `ListBlock` → resolve → `BuiltList::lower` |
| Multi-child | `Vec` of pieces; measure still stacks via `CellBody` |
| Layout | No change to `nesting.rs` matrix |
| Delete | `CellContent` / `StoredContent` dual enums as separate shapes; `DeferredList`; `assemble_list` |

## Defaults for open questions

- Detached `Table::build` + image/`ListBlock` with wrong `DocToken`: keep today's `UnknownImage` behavior; rustdoc note if missing.
- Cross-row error order: preserve store-all-lists-before-lower-paragraphs.
- Multi-item gap: default stack `spacing_after` (zero at default style). No new cell gap API.
- Empty `Cell::empty()` in a row: zero-height slot, allowed.

## Verification

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --features test-utils -- -D warnings
cargo test --features test-utils
cargo doc --no-deps
```

Update `CHANGELOG.md`, `docs/flow-writer-spec.md`, `docs/deref-flow-api.md` status. No commit unless the human asks.
