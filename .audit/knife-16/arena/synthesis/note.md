# Knife 16 arena synthesis note

**Base.** candidate-2 (cross-judge recommendation).  
**Parent prior lean.** candidate-3 for fixture-tight extras. Disagreement resolved by accepting the judge on the base (identical API; c2 primary proof is the tighter required suite) and grafting c3's strongest unique tests.

**Cross-judge.** `.audit/knife-16/arena/cross-judge.md` — base c2; grafts from c3 (alone no-ops, optional short-beside-text) and c1 (rustdoc body vs cell contrast).

**Convergence.** All four candidates agreed on `Cell::spacer(Length<NonNegative>)` → `CellContent::Spacer` → `FlowNode::Spacer`. No reverse words.

**Grafts applied to implementer brief.**

1. From c3: `cell_spacer_alone_records_no_ops` (spacer-only table, empty `ops()`).
2. From c3: `short_cell_spacer_does_not_grow_a_text_row` if cheap after the required suite.
3. From c1: rustdoc notes body spacer splits; cell spacer over `inner_full` is `UnsplittableTallerThanFrame`.
4. From c2 (base): spacer-only row + zero vs positive y delta; taller-than-body error; optional `cell_allows_spacer`.

**Verification of this note.** Designs re-read end to end; judge file present; no production code yet.
