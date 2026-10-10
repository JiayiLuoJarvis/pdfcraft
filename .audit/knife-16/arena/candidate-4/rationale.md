# Knife 16: Public Table Cell Spacer (`Cell::spacer`) — Rationale Document

This document records the architectural alternatives considered and rejected for Knife 16, detailing the trade-offs that justify the chosen design.

---

## 1. API Naming and Construction Signature

### Selected Design
`Cell::spacer(height: Length<NonNegative>) -> Self`

### Alternatives Considered and Rejected

1. **`Cell::space(...)` (Frame reverse word: `space`)**
   - *Why considered:* Shorter name.
   - *Why rejected:* In `quire-pdf`, the existing body flow primitive is named `Flow::spacer(height: Length<NonNegative>)` (`src/document.rs:726-728`). Using `Cell::space` would introduce gratuitous inconsistency between the flow API and the cell API. Mirroring `Flow::spacer` preserves cognitive ergonomics for callers.

2. **Closure-based Builder: `Cell::spacer(f: impl FnOnce(&mut SpacerBox))`**
   - *Why considered:* Precedents like `Cell::hline(f: impl FnOnce(&mut HLine))` and `Cell::image(img, f: impl FnOnce(&mut ImageBox))`.
   - *Why rejected:* `HLine` and `ImageBox` have multiple configurable properties (width, alignment, color, space_before, space_after, height). A spacer has exactly one semantic property: its height. Introducing a builder object and closure for a single property adds caller ceremony and allocation overhead without functional benefit.

3. **Implicit Conversion: `impl From<Length<NonNegative>> for Cell`**
   - *Why considered:* Allows writing `t.row([NonNegative::pt(10.0).unwrap()])` directly.
   - *Why rejected:*
     - Asymmetric with other non-text cells: Neither `Image` nor `HLine` implements `From` for `Cell`; only strings do (`&str`, `String`, `&String`).
     - Ambiguity: A bare length in a cell could easily be misinterpreted as a column width, padding override, or font size rather than an empty spacer node.
     - Requiring `Cell::spacer(...)` makes caller intent explicit and self-documenting.

---

## 2. Zero-Height Handling

### Selected Design
Allow zero height (`NonNegative::ZERO` is valid; fits at height 0 and emits no ink).

### Alternatives Considered and Rejected

1. **Rejecting Zero Height (Frame reverse word: `reject-zero`)**
   - *Why considered:* A zero-height spacer has no visual effect on layout or ink.
   - *Why rejected:*
     - Violates **提案书 §2.13**: Explicitly states *"高度为 §2.0 非负长度（类型层约束，可为 0）"*.
     - Layout support already exists: `SpacerNode::split` explicitly matches `h == 0.0` and returns `Split::Fit { height: 0.0, fragment: Fragment::empty() }` (`src/layout/flow.rs:182-186`).
     - Dynamic templates: Callers generating tables programmatically often pass computed heights where conditional gaps evaluate to zero. Rejecting zero would force callers to write boilerplate conditional logic (`if h > 0 { row.push(Cell::spacer(h)) }`).

---

## 3. Storage and Single-Content Architecture

### Selected Design
Extend the private `CellContent` enum with a single new variant: `CellContent::Spacer(Length<NonNegative>)`.

### Alternatives Considered and Rejected

1. **Multi-Content Cell Closure: `Cell::stack(impl FnOnce(&mut Flow))` (Frame reverse word: `stack-cell`)**
   - *Why considered:* `layout::table::Cell` internally accepts `Vec<FlowNode>`. Enabling multiple nodes per cell would let callers put a paragraph, spacer, and rule in one cell.
   - *Why rejected:*
     - Violates the Knife 16 scope: Frame rubric explicitly designates multi-kid public cell and cell closures as out-of-knife.
     - High complexity: Exposing `&mut Flow` to a cell exposes elements strictly forbidden in table cells by **提案书 §2.6** (`columns`, `page_break`, `keep_together`, nested tables). Handling validation or runtime error deferral inside cell closures expands the public API surface substantially.
     - Proposal fidelity: **提案书 §2.6** explicitly notes that cell DSL signatures are not pinned; rushing a multi-kid closure into Knife 16 would preempt product decisions.

2. **Storing `FlowNode::Spacer` Directly in `CellContent`**
   - *Why considered:* Avoids an intermediate variant in `CellContent`.
   - *Why rejected:*
     - Leaks internal layout types (`FlowNode`, `SpacerNode`) into `src/document.rs` public/semi-public structures.
     - Existing `CellContent` variants store raw data: `Text(String)`, `Image(Image, ImageBox)`, `HLine(HLine)`. Storing `Length<NonNegative>` preserves this structural pattern.
     - `Length<NonNegative>` implements `Eq`, allowing `Cell` to maintain `#[derive(PartialEq, Eq)]`.

---

## 4. Pagination and Overflow Semantics

### Selected Design
Retain `natural_height` measurement: a cell spacer taller than `inner_full` returns `Error::UnsplittableTallerThanFrame`.

### Alternatives Considered and Rejected

1. **Mid-Cell Page Splitting (Frame reverse word: `split-in-cell`)**
   - *Why considered:* In body flow, `SpacerNode::split` splits across pages (`Split::Partial`), allowing a tall spacer to span pages.
   - *Why rejected:*
     - Violates **提案书 §2.6**: The proposal explicitly mandates:
       > “跨页：允许；切分单位是整行，不在行中横切”
     - Table rows are atomic layout units. If a cell within a row could split mid-cell across a page break while neighboring cells stay on the first page, the table grid would fracture into inconsistent geometric fragments.
     - In `src/layout/table.rs`, table measurement uses `natural_height` specifically to determine whether a cell can fit into a page. Discarding `Partial` and failing if content cannot fit a single frame is the foundational contract of atomic row layout.

2. **Silent Clamping or Truncation**
   - *Why considered:* Prevents document failure when a spacer is oversized.
   - *Why rejected:*
     - Violates `quire`'s core design philosophy (explicit failure over silent document distortion; "缺字即错").
     - Silently truncating a spacer silently changes user-specified layout and proportions without warning.

---

## 5. Observation and Verification Strategy

### Selected Design
Verify spacer effect by measuring neighbor text y-coordinate offset via `document().pages()` and `pt_near`.

### Alternatives Considered and Rejected

1. **Page-Count Only (Frame reverse word: `page-count-only`)**
   - *Why considered:* Simpler test assertions (`assert_eq!(pages.len(), 2)`).
   - *Why rejected:*
     - Coarse and insufficient: A table moving to a second page proves that some vertical height was occupied, but cannot prove that the row height expanded by the exact `Length<NonNegative>` specified by the caller.
     - Inspecting the glyph origin of subsequent rows via `pt_near` proves exact geometric correctness.

2. **Emitting a Dummy/Sentinel Canvas Command**
   - *Why considered:* Makes asserting the presence of the spacer trivial (e.g. searching for a tag in `page.ops()`).
   - *Why rejected:*
     - Violates **提案书 §2.13**: A spacer is strictly an invisible vertical blank.
     - Emitting dummy operations pollutes the recording canvas and adds useless overhead to the sealed `krilla` backend.

---

## 6. Autonomy Defaults Summary

Candidate 4 adheres to all default positions from `arena/frame.md`:
- API: `Cell::spacer(height: Length<NonNegative>)` (default followed; no reverse word).
- Zero height: allowed (default followed; no reverse word).
- Cell payload: single content per cell (default followed; no reverse word).
- Frame overflow: `UnsplittableTallerThanFrame` (default followed; no reverse word).
- Proof: neighbor text y shifted by spacer height (default followed; no reverse word).
