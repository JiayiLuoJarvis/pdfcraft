# Knife 16 candidate 1 — rationale

## Recommendation in one sentence

Add **`CellContent::Spacer` + `Cell::spacer(height)` + one `Table::lower` arm** and prove geometry with **`document().pages()`**; leave all layout/spacer split code unchanged.

---

## Alternatives considered and rejected

### 1. `Cell::space(height)` instead of `Cell::spacer`

**Rejected.** Frame autonomy default mirrors `Flow::spacer` and keeps one vocabulary for the same domain object (§2.13). `space` would diverge without product benefit.

### 2. `reject-zero` — forbid `NonNegative::ZERO` in `Cell::spacer`

**Rejected.** Proposal §2.13 allows zero; `SpacerNode::split` already `Fit`s at height 0 with no ops. Flow spacer accepts zero; cell should match.

### 3. `stack-cell` — public API to put spacer + text in one cell

**Rejected for this knife.**

- §2.6 allows spacer *as a cell child type*, not a pinned multi-kid DSL.
- Today every public cell lowers to exactly one `FlowNode`; layout’s `Vec<FlowNode>` is internal.
- Exposing `Cell::stack(|f| { … })` or `Cell::flow(...)` is a larger API decision (paragraph, list, ordering, style inheritance) and belongs to a separate knife.
- A spacer-*only* cell satisfies the immediate gap: vertical gap **between rows/columns** via dedicated cells.

### 4. `split-in-cell` — teach `natural_height` or table measure to preserve `Partial` for spacers

**Rejected.**

- Conflicts with §2.6: table splits on **whole rows**, not mid-row/c mid-cell.
- `natural_height` intentionally maps Partial → `UnsplittableTallerThanFrame` so cell stacks must fit the full inner frame before row pagination runs.
- Body-flow split (`paginate.rs`, `spacer_remainder_leads_next_page`) is the correct place for §2.13 cross-page spacer; conflating the two would change semantics for every cell stack, not just spacers.
- Document the difference in `Cell::spacer` rustdoc rather than changing layout.

### 5. Nested `Flow` on table cells (caller runs `f.spacer(h)` inside cell closure)

**Rejected.**

- `Table::row` only accepts `Into<Cell>`, unlike `List::item` / `Columns::col`. Adding a Flow closure is new surface area and duplicates the lower path (`Flow::spacer` already exists for body).
- Minimal bridge: store height on `CellContent` and lower to the same `SpacerNode` as flow.

### 6. Second public type `CellSpacer` or wrapper struct

**Rejected.** Rubric and `data-shape.md`: one domain object, one layout node. Extra types add API weight with no layout benefit.

### 7. `From<Length<NonNegative>> for Cell`

**Rejected.** Precedent: `Cell::image` and `Cell::hline` use named constructors; `From` is reserved for text shorthand. Avoids ambiguous `t.row([gap.into(), "hi"])` if both could convert.

### 8. Modify `check_content` to add Spacer allowance

**Rejected.** Explorer and `lower.rs` already list `FlowNode::Spacer` in the `TableCell` arm. Duplicating the rule risks drift and expands diff without behavior change.

### 9. Layout-only delivery (internal tests building `table::Cell::new(vec![Spacer])` without public API)

**Rejected.** Knife goal is **caller-facing** §2.6/§2.13 compliance. Layout test D is optional supplement; public `Cell::spacer` is required.

### 10. `page-count-only` proof (assert two pages without y math)

**Rejected.** Frame default requires neighbor glyph **y** shifted by spacer height (or equivalent row-gap geometry). Page count alone does not distinguish row move vs mid-cell split vs unrelated breaks.

### 11. Assert dedicated “spacer paint” commands or background rects

**Rejected.** §2.13: no visible content; no colored background in scope. Tests should observe **absence of ink** and **presence of vertical gap** via neighbors.

### 12. Lower via private helper shared with `Flow::spacer`

**Considered, not required.** Both paths are one-liners:

```rust
FlowNode::Spacer(SpacerNode { height })
```

A shared `fn spacer_node(height: Length<NonNegative>) -> FlowNode` in `document.rs` is optional sugar; rubric prefers minimal diff — inline in both sites unless duplication bothers review.

### 13. Integration test only in `src/layout/table.rs` via direct `TableNode`

**Rejected as sole proof.** Frame rubric asks for public `document().pages()` path. Layout `cell_allows_spacer` is cheap regression on allowlist wiring.

---

## Why this fits the seams

| Rubric axis | How this design scores |
| --- | --- |
| Proposal fidelity | Spacer allowed in cell; non-negative height; no paint; forbidden kids unchanged |
| Caller surface | Symmetric to `Flow::spacer` / `Cell::hline`; one height arg |
| Layout honesty | Reuses existing node; documents Partial rejection in cell measure |
| Fit to seams | Only `document.rs` + tests |
| Testability | `pt_near` on following row; error case; optional allowlist unwrap |
| Laziness | No paragraph-in-cell, no pagination rewrite |

---

## Risks (accepted)

1. **Doc confusion:** Callers may expect cell spacers to split like body spacers. Mitigation: rustdoc on `Cell::spacer` contrasts with `Flow::spacer`.
2. **Future padding/border on public tables:** Shrinks `inner_full`; error threshold moves — no API change needed.
3. **Top-aligned multi-column rows:** Spacer column grows row height; text in sibling columns stays at top — correct per current table alignment, not vertical centering.

---

## Baseline alignment

Cut from `feat/text-uri-link` @ `16749da`. No dependency on URI link behavior; same `document.rs` cell/table area as hline/image cells.
