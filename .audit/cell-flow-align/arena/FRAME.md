# Arena frame: Cell entry ↔ FlowWriter / add

## Artifact

Each candidate writes a design package (not code):

1. `design.md` — usage-first call sites, then type/signature sketch (`not implemented` / TODO ok), module map, forbidden-content policy, migration notes for tests.
2. `RATIONALE.md` — shaped per architect `rationale-template.md` (Problem, Usage, Shape, Tradeoffs, Alternatives, Open questions, Next step). Leave "Synthesis decision" as `TBD (arena)`.

Output directory is assigned per runner. Write only there. Do not edit the main checkout.

## Grounding (read these)

Repo checkout for facts (worktree or `/mnt/data/demo/quire`):

- `.audit/cell-flow-align/how/explanation.md`
- `.audit/cell-flow-align/how/grounding-notes.md`
- `docs/flow-writer-spec.md` (follow-up: Cell shares FlowWriter/add build results)
- `docs/deref-flow-api.md` (Cell 入口与 add 对齐)
- `docs/提案书.md` §2.6 cell children
- `src/document/table.rs`, `src/document/flow_writer.rs`, `src/document/flow.rs`
- `src/layout/nesting.rs` (`check_content` / TableCell)

## Confirmed direction (not a fork)

- Align Cell content entry with the body model: sugar + independent element values + public `add`-style admission.
- Share build results with `FlowWriter` / `IntoFlowItem` (or a deliberately narrower sealed admission that still reuses the same element values / `build` / `build_on`).
- Do not reopen "keep parallel Cell::* zoo".
- Do not change proposal product semantics (still forbid columns / pageBreak / keep_together / nested table in cells).
- 0.x may break. Theme-internal rigor over compatibility patches. Minimal scope: this theme only.
- Do not design limits, letter fixtures, nested-table enablement, or `table/cell.rs` split unless required by the chosen shape.

## Structural diversity requirement

Produce a **whole-shape** alternative, not a tweak of the same shape. Two intended poles (runners may invent a third if stronger, but must differ structurally from the other candidate):

- **Pole A — Cell as writer over an internal flow.** Cell (or a builder handed to `row`) implements `FlowWriter` or writes through `flow_mut` / `add`, then freezes into stored nodes. Forbidden sugars either absent or rejected.
- **Pole B — Narrow admission trait, not full FlowWriter on Cell.** e.g. `IntoCellItem` / shared `ContentWriter` subset; Cell keeps value/`add` ergonomics without exposing columns/table/page_break sugars on the cell type.

Each runner picks one pole (or a clearly distinct third) and argues it. Do not converge to a mushy middle.

## Rubric (for judges; candidates do not self-score)

1. **Usage fit.** Everyday sugar for cells remains teachable; factory/`add` path works for helpers returning element values.
2. **Shared build results.** Paragraph/HLine/image/list construction is not a second parallel API.
3. **Product forbid surface.** columns / pageBreak / keep_together / nested table cannot be "accidentally correct" call sites; prefer type-level absence or one clear reject path with `InvalidCellContent` (or equivalent) at a documented time.
4. **Parent-flow / list lifetime.** Correct style/doc token for nested lists; borrow-until-row (or a proven replacement) still works for realistic table closures.
5. **Interface depth + laziness.** Small public surface hiding policy; no pass-through zoo; no hand-synced dual enums if avoidable.
6. **Migration cost.** Clear story for `Cell::new` / `From<&str>`, span/fill chains, and existing `tests/document.rs` call sites.
