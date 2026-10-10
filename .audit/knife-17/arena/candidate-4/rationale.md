# Knife 17 Candidate 4 — Rationale

## Recommendation in One Sentence

Introduce **`Cell::paragraph`** (via shared **`lower_paragraph`** emitting unwrapped **`FlowNode::Text`**) and **`Cell::ul` / `Cell::ol`** (deferring list construction to **`Table::lower`** via **`ListSeed`** with **`Box<dyn FnOnce(&mut List) + 'static>`**), preserving §2.6 whole-row pagination and §2.8 list rules with zero layout allowlist changes, adjusting **`Cell`** derives to **`Debug`**-only, and delivering via two sequential commits (`17a` paragraph, `17b` list).

---

## 1. Adherence to Autonomy Defaults

Candidate 4 adopts all autonomy defaults from `arena/frame.md` with zero reverse-word divergences:

| Autonomy Call | Candidate 4 Stance | Reverse Word Avoided | Rationale |
| --- | --- | --- | --- |
| **Paragraph API** | `Cell::paragraph(\|p\| { … })` | `items-only` | Directly mirrors `Flow::paragraph` and body ergonomics. Avoids a redundant or lower-level item-only builder. |
| **Plain Text Parity** | Keep `Cell::new(str)` unchanged | `rich-new` | Preserves `From<&str>`, backward compatibility, and clean shorthand for unstyled text. |
| **Inline Rich Features** | Reuse `Paragraph` methods (bold, blank, link) | `text-only` | Fully complies with proposal §2.6 requirement for paragraphs with fill blanks and links in cells. |
| **List API** | `Cell::ul` / `Cell::ol` mirroring `Flow` | `list-closure-on-row` | Allows cell definitions to remain orthogonal values inside row arrays `t.row([Cell::ul(...)])`. |
| **List Flow Seam** | Defer list closure to `Table::lower(&Flow)` | `table-child-flow` | Reuses existing `List::item` and parent style/token without polluting `Table` with child flow fields. |
| **Split Knife** | Ship both via sequential commits; fall back to 17a if slips | `force-both` | Pragmatic laziness: both are small enough for one evening, but 17a is cleanly separable if blocked. |
| **Multi-kid Cell** | Out (one primary payload per cell) | `stack-cell` | Respects Knife 16/17 boundary. Multi-kid cell requires a broader design discussion on cell sequences. |
| **Too-tall Cell** | `Error::UnsplittableTallerThanFrame` | `split-in-cell` | Strictly enforces §2.6 whole-row pagination invariant via existing `natural_height`. |

---

## 2. In-Depth Architectural Tradeoffs

### 2.1 Paragraph Lowering: `TextNode::from_items` vs `FlowNode::Paragraph`

- In the layout pipeline, `FlowNode::Text` holds unwrapped inline runs (`TextNode::from_items`). Line breaking occurs only during `TextNode::split`, which has access to the target frame width (`avail.width`).
- In table layout, the target frame width is the cell's `inner_w` (`spanned_column_width - 2 * inset`).
- If `Cell::paragraph` were to construct a `FlowNode::Paragraph`, it would force line breaking before the cell's column width is known, producing incorrect wraps, broken fill blanks (`blank_to_end`), and misplaced link rectangles.
- Therefore, emitting `FlowNode::Text` through a shared `lower_paragraph` helper is strictly mandatory.

### 2.2 Preserving Paragraph Errors

- `Paragraph` records assembly-time errors such as `Error::EmptyLink` (empty URI provided to `p.link`) and `Error::RunAfterFillBlank` (runs appended after `p.blank_to_end`).
- If `CellContent::Paragraph` only captured `Vec<InlineItem>`, these errors would be silently discarded, violating the layout honesty rubric.
- Storing the full `Paragraph` struct in `CellContent::Paragraph(Paragraph)` guarantees that `lower_paragraph` surfaces `paragraph.error` immediately at lower time, aborting layout cleanly.

### 2.3 List Seam: Deferral vs Table Child Flow vs Synthetic Flow

Why deferral via `ListSeed` is the winning design:

1. **Synthetic `Flow::new(default_style(), DocToken::next())` inside `Cell::ul` is flawed:**
   - It manufactures an artificial `Flow` disconnected from the document.
   - It fragments `DocToken` sequence and breaks the moment inherited or section-level styles are applied to `Flow`.
2. **`Table` holding a child `Flow` (`table-child-flow`) degrades ergonomics:**
   - If `Table` owned `parent: Flow`, building a list cell would require `t.ul_cell(...)` or passing `&mut Table` into `Cell::ul`.
   - This destroys the orthogonality where cells are standalone `Cell` values passed to `t.row([...])` or constructed by helper functions.
3. **Deferral to `Table::lower` achieves total reuse with zero DSL friction:**
   - `Table::lower(self, flow: &Flow)` already has the live parent `Flow`.
   - Calling `flow.child()` provides the authentic document styling and token.
   - Running the closure at lower time constructs the standard `List`, delegating item handling to `List::item` and `ListItem::new`.

### 2.4 Closure Storage: `FnOnce` vs `FnMut` and Lifetime Considerations

- Candidate 1 selected `Box<dyn FnMut(&mut List) + 'static>`.
- Candidate 4 refines this to `Box<dyn FnOnce(&mut List) + 'static>`:
  - Modern Rust (since 1.35) permits direct invocation of boxed `FnOnce` via `(seed.build)(&mut list)`.
  - `FnOnce` is strictly more general than `FnMut`, allowing caller closures to capture and move owned values (e.g. `move |list| ...`).
  - `Table::lower` executes the closure exactly once per cell during table construction, perfectly matching `FnOnce` semantics.
- Requiring `'static`:
  - Eliminates the need for a lifetime parameter on `Cell<'a>` and `Table<'a>`.
  - In document generation DSLs, data is either static string literals or owned formatted strings (`format!(...)`), which naturally satisfy `'static`.

### 2.5 `Cell` Derives Audit

- Current `Cell` has `#[derive(Debug, Clone, PartialEq, Eq)]`.
- Adding `Paragraph` introduces `Vec<InlineItem>` and `Option<Error>`, neither of which implements `Eq`. Furthermore, `Paragraph` does not implement `Clone`.
- Adding `ListSeed` introduces a boxed closure (`Box<dyn FnOnce... + 'static>`), which cannot implement `Clone`, `PartialEq`, or `Eq`.
- An exhaustive grep of the repository (`src/` and `tests/`) proves that `Cell` is never cloned and never compared for equality (`==`).
- Therefore, dropping `Clone`, `PartialEq`, and `Eq` from `Cell` is safe and sound.
- `Debug` is preserved on `Cell` by deriving `Debug` on `Paragraph` and providing a manual `Debug` implementation for `ListSeed`.

---

## 3. Layout Honesty and Error Diagnostics

### 3.1 Container Allowlist Matrix

No edits are needed in `src/layout/lower.rs`:
- `check_content(Container::TableCell, &nodes)` already allows `Text`, `Paragraph`, `Spacer`, `HLine`, `Image`, and `List`.
- `check_content(Container::ListItem, &nodes)` already allows `Text`, `Paragraph`, `Spacer`, `HLine`, and `List` (nested lists).

### 3.2 Error Diagnostic Precision

The container context is preserved with 100% accuracy:
- If a caller puts `table`, `columns`, `page_break`, or `keep_together` directly in a table cell:
  `Error::InvalidCellContent { container: "table cell", node: ... }`.
- If a caller puts `table`, `columns`, `page_break`, `keep_together`, or `image` inside a list item in a table cell:
  `Error::InvalidCellContent { container: "list item", node: ... }`.
- Notice that `image` is legally allowed directly in a table cell (`Cell::image`), but rejected inside a list item (`ListItem::new`). This nuance is enforced naturally without special casing.

### 3.3 Pagination Invariants: Whole-Row Splitting

- §2.6 mandates that tables paginate on physical rows, never splitting across a mid-cell boundary.
- During table measurement (`layout/table.rs`), each cell is measured via `natural_height(body.into_node(), ctx, cell_frame)`.
- `natural_height` accepts only `Split::Fit`. If a rich paragraph or list returns `Split::Partial` or `Split::MoveToNextPage`, `natural_height` converts it into `Error::UnsplittableTallerThanFrame`.
- This ensures that a cell exceeding the page height causes a clean, fail-fast error rather than corrupting pagination or partially orphan-rendering content.

---

## 4. Risk Register & Mitigations

| Risk | Impact | Mitigation Strategy |
| --- | --- | --- |
| `'static` closure captures | Caller cannot capture temporary borrowed references by reference | Document in rustdoc; callers use `move` and capture owned strings/scalars. |
| Narrow cell geometric failure | List marker requires 18 pt; column `< 18 pt` fails layout | Document that list cells require column width > 18 pt; fails with `InvalidGeometry`. |
| Loss of `Cell: Clone/Eq` | Breaking change if external callers relied on `Cell: Clone` | `quire` is `publish = false` (v0.x internal); no internal callers rely on it; mention in CHANGELOG. |
| Inadvertent layout drift | Cell paragraph behaving differently from body paragraph | Shared `lower_paragraph` helper ensures single source of truth for paragraph lowering. |

---

## 5. Fidelity Checklist (Arena Rubric)

1. **Proposal Fidelity:**
   - Cell paragraph supports bold, blank (`blank_em`, `blank_to_end`), and link (§2.6).
   - Cell list supports unordered bullets, ordered numbering, custom markers, and nesting (§2.8).
   - Forbidden children (`columns`, `page_break`, `keep_together`, nested `table`) stay strictly forbidden.
2. **Caller Surface:**
   - Reuses `Paragraph` and `List` APIs directly.
   - Clean, readable closure syntax `Cell::paragraph(|p| ...)`, `Cell::ul(|l| ...)`, `Cell::ol(...)`.
   - `Cell::new(str)` unchanged.
3. **Layout Honesty:**
   - Reuses existing `Container::TableCell` and `Container::ListItem` allowlists.
   - Preserves `natural_height` Fit-only semantics.
   - Accurately reports container names (`"table cell"` vs `"list item"`).
4. **Fit to Seams:**
   - Deferral via `ListSeed` in `Table::lower` connects directly to `Flow::child()`.
   - Zero krilla leakage outside `backend/`. No second PDF backend.
5. **Testability:**
   - Comprehensive test plan using `document().pages()` and `RecordingCanvas`.
   - Validates font IDs, blank stroke widths, link annotation rects, list marker positions, and error bailouts.
6. **Laziness & Slicing:**
   - Minimal new types (only `ListSeed` and new `CellContent` variants).
   - Clean decomposition into two sequential commits (`17a` then `17b`) within a single knife.
