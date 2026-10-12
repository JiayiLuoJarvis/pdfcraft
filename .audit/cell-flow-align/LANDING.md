# Cell ↔ FlowWriter/add landing

| Item | Value |
| --- | --- |
| Branch | `feat/flow-writer` (uncommitted working tree) |
| Base design | Arena C2 + C1 grafts (see `arena/synthesis/`) |
| Implementer | agent 37a1b96d-07a8-4c84-b9ac-c7025f435a49 |
| Orchestrator re-gate | 2026-10-12: fmt / clippy test-utils / test test-utils / doc -Dwarnings — all pass |

## Public shape

- `IntoCellItem` (sealed): `Paragraph`, `HLine`, `PlacedImage`, `Spacer`, `ListBlock`
- `Cell::empty` / `Cell::add`; sugars unchanged as `empty().add(…)`
- Body `ul` / `ol` / `spacer` admit `ListBlock` / `Spacer`
- `Cell` is not `FlowWriter`

## Deviations accepted

- `ListBlock` stores `ListKind`; `build_on` + `BuiltList::lower` split for borrowck
- No `PlacedImage::lower` helper; cell uses `IntoFlowItem::add_to`
- `#[allow(clippy::should_implement_trait)]` on `Cell::add` with one-line reason
