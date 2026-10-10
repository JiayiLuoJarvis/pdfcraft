# Knife 16: Public Table Cell Spacer (`Cell::spacer`) — Design Document

## 1. Executive Summary

This design package specifies the addition of `Cell::spacer` to the public document DSL for `quire-pdf`, fulfilling **提案书 §2.6** (table cell children allow spacer) and **§2.13** (spacer is a non-negative vertical blank allowed in cells).

The layout engine already permits `FlowNode::Spacer` inside `Container::TableCell` (`src/layout/lower.rs:33-36`), and `CellBody` already wraps content for layout measurement. The gap is strictly at the DSL and lowering boundary: `CellContent` has no spacer arm, `Cell` lacks a constructor, and `Table::lower` has no match arm mapping to `FlowNode::Spacer`.

This design makes the minimal, fully honest addition:
- A new public constructor `Cell::spacer(height: Length<NonNegative>) -> Cell`.
- A private storage variant `CellContent::Spacer(Length<NonNegative>)`.
- An exhaustive lowering arm producing `vec![FlowNode::Spacer(SpacerNode { height })]`.
- Adheres to all autonomy defaults from `arena/frame.md`:
  - API name: `Cell::spacer(height)` (matches `Flow::spacer`).
  - Zero height: allowed (`NonNegative::ZERO` fits at height 0).
  - Multi-content cell: one content per cell (single-child cell model preserved).
  - Frame overflow: `UnsplittableTallerThanFrame` preserved via existing `natural_height`.
  - Proof: neighbor text y-coordinate shifted by spacer height, verified via `document().pages()` and `pt_near`.

---

## 2. Caller Usage Sketch (Rust DSL)

Callers construct table cells using `Cell::spacer` in rows or headers, optionally applying cell span modifiers:

```rust
use quire_pdf::{document, Cell, Color, Length, NonNegative, PageSize, Positive, Size};

let page_size = PageSize::custom(Size::try_pt(400.0, 300.0).unwrap());
let spacer_h = NonNegative::pt(25.0).unwrap();

let rendered = document(page_size, |doc| {
    doc.font(FONT_BYTES.to_vec())?;

    doc.table(|t| {
        // 1. Header row
        t.header(["Col 1", "Col 2"]);

        // 2. Body row with a vertical spacer cell beside a text cell
        // The spacer expands the row height to 25 pt; the text cell stays top-aligned.
        t.row([
            Cell::spacer(spacer_h),
            Cell::new("Top-aligned text"),
        ]);

        // 3. Spacer cell spanning multiple columns
        t.row([
            Cell::spacer(NonNegative::pt(10.0).unwrap())
                .col_span(std::num::NonZeroU16::new(2).unwrap()),
        ]);

        // 4. Zero-height spacer (valid, fits at height 0, draws nothing)
        t.row([
            Cell::spacer(NonNegative::ZERO),
            Cell::new("Zero spacer neighbor"),
        ]);
    });

    Ok(())
})?;

let pages = rendered.pages();
```

---

## 3. Types, Enum Arms, and Method Signatures

### 3.1. Internal Storage: `CellContent`

In `src/document.rs`, extend the private `CellContent` enum with the `Spacer` variant:

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
enum CellContent {
    Text(String),
    Image(Image, ImageBox),
    HLine(HLine),
    Spacer(Length<NonNegative>),
}
```

*Note on traits:* `Length<NonNegative>` implements `Clone`, `Copy`, `PartialEq`, `Eq`, and `Debug`. Therefore, existing derived traits on `CellContent` (`Debug`, `Clone`, `PartialEq`, `Eq`) remain satisfied with zero boilerplate.

### 3.2. Public Constructor: `Cell::spacer`

In `src/document.rs`, add the constructor on `impl Cell`:

```rust
impl Cell {
    // ... existing Cell::new, Cell::image, Cell::hline ...

    /// A one-by-one cell holding vertical blank space of the given `height`.
    ///
    /// Like horizontal lines and images, this creates a cell containing a single
    /// spacer element. Negative heights are rejected when constructing [`Length<NonNegative>`].
    pub fn spacer(height: Length<NonNegative>) -> Self {
        Self {
            content: CellContent::Spacer(height),
            col_span: NonZeroU16::MIN,
            row_span: NonZeroU16::MIN,
        }
    }

    // ... existing col_span, row_span ...
}
```

*Span support:* Because `Cell::spacer` initializes `col_span` and `row_span` to `NonZeroU16::MIN` (1), chaining `.col_span(n)` and `.row_span(n)` works identically to `Cell::new` and `Cell::hline`.

*Conversion trait:* No `From<Length<NonNegative>> for Cell` is added. In `quire-pdf`, only text implements `From` for `Cell` (`&str`, `String`, `&String`). Neither `Image` nor `HLine` provides `From`, maintaining explicit constructor call semantics across all non-text cell elements.

### 3.3. Lowering: `Table::lower`

In `src/document.rs`, extend the exhaustive match inside `Table::lower`:

```rust
let content = match cell.content {
    CellContent::Text(text) => vec![flow.text(text)],
    CellContent::Image(image, image_box) => {
        vec![image_box.lower(&image, flow.doc)?]
    }
    CellContent::HLine(line) => vec![line.lower()],
    CellContent::Spacer(height) => vec![FlowNode::Spacer(SpacerNode { height })],
};
```

`SpacerNode` is already imported into `src/document.rs` (`src/document.rs:22`). Lowering is infallible and requires neither parent `flow` styles nor `flow.doc` tokens.

### 3.4. Re-exports

`Cell` is already re-exported in `src/lib.rs:21-24`:
```rust
pub use document::{
    document, Band, Blank, Cell, Columns, Document, Flow, HLine, Image, ImageBox, Layer, List,
    Numbering, Page, Paragraph, Rendered, Section, Shapes, Table,
};
```
No additions or modifications to `src/lib.rs` are needed.

---

## 4. Module Map

| File | Status | Description of Changes |
| --- | --- | --- |
| `src/document.rs` | **MODIFIED** | Add `CellContent::Spacer(Length<NonNegative>)`<br>Add `Cell::spacer(height)` constructor<br>Add match arm in `Table::lower` emitting `FlowNode::Spacer(SpacerNode { height })` |
| `src/layout/table.rs` | **MODIFIED (tests only)** | Add layout unit test `cell_allows_spacer` beside `cell_allows_hline` verifying `table::Cell::new(vec![FlowNode::Spacer(...)])` succeeds |
| `tests/document.rs` | **MODIFIED (tests only)** | Add integration tests covering geometry offset (`pt_near`), zero-height spacer, and overflow error (`UnsplittableTallerThanFrame`) |
| `src/lib.rs` | **UNTOUCHED** | `Cell` is already exported; no new types exposed |
| `src/layout/lower.rs` | **UNTOUCHED** | `check_content` already permits `FlowNode::Spacer` in `Container::TableCell` (`line 33-36`) |
| `src/layout/flow.rs` | **UNTOUCHED** | `SpacerNode`, `split`, and `natural_height` already handle blank spacing and frame bounds |
| `src/layout/table.rs` (prod) | **UNTOUCHED** | `TableNode::split`, `measure`, and `split_measured` already measure cell stacks with `natural_height` |
| `src/backend/*` | **UNTOUCHED** | Backend translates canvas recording commands; spacer produces no canvas commands |
| `src/error.rs` | **UNTOUCHED** | Existing errors (`UnsplittableTallerThanFrame`, `NegativeLength`) cover all error paths |

---

## 5. Lowering and Measurement Behavior Notes

### 5.1. Assembly and Seam Traversal

1. **Cell Construction:** Caller calls `Cell::spacer(height)`. Returns `Cell { content: CellContent::Spacer(height), col_span: 1, row_span: 1 }`.
2. **Table Collection:** Caller passes `Cell` to `t.row([...])` or `t.header([...])`.
3. **Table Lowering:** In `Table::lower`:
   - `match cell.content` matches `CellContent::Spacer(height)` and produces `vec![FlowNode::Spacer(SpacerNode { height })]`.
   - Passes the node vector to `table::Cell::new(content)`.
   - `table::Cell::new` calls `CellBody::new(nodes)`.
   - `CellBody::new` calls `check_content(Container::TableCell, &nodes)`.
   - `check_content` matches `FlowNode::Spacer(_)` on line 34 of `src/layout/lower.rs`, which returns `true`.
   - `table::Cell::with_span(col_span, row_span)` attaches span metadata.

### 5.2. Measurement under `natural_height`

During pagination, `TableNode::split` runs table measurement via `src/layout/table.rs:191-232`:
1. **Frame Setup:**
   For each placed cell, layout computes:
   ```text
   inset = border_width / 2 + padding
   inner_full = avail.full_height - 2 * inset
   frame = Frame { width: inner_w, height: inner_full, full_height: inner_full }
   ```
   *Current state:* Public tables have `border_width = 0` and `padding = 0`, so `inset = 0` and `inner_full = avail.full_height`.
2. **Cell Body Execution:**
   Layout calls `natural_height(body.into_node(), ctx, frame)`.
   `CellBody::into_node` wraps the child vector into `FlowNode::Stack(vec![FlowNode::Spacer(SpacerNode { height })])`.
3. **`SpacerNode::split` vs `natural_height` Outcomes:**
   - **Case A: `height == 0.0`**
     `SpacerNode::split` returns `Split::Fit { height: 0.0, fragment: Fragment::empty() }`.
     `natural_height` returns `Ok((0.0, empty_fragment))`.
     The cell contributes `0.0` to the row height.
   - **Case B: `0.0 < height <= inner_full`**
     `take = min(height, avail.height) = height`. `rest = 0.0`.
     `SpacerNode::split` returns `Split::Fit { height, fragment: Fragment { height, spacing_after: 0.0, commands: vec![] } }`.
     `natural_height` returns `Ok((height, empty_fragment))`.
     The cell contributes `height` to the row height.
   - **Case C: `height > inner_full`**
     `take = inner_full`. `rest = height - inner_full > 0.0`.
     `SpacerNode::split` returns:
     ```rust
     Split::Partial {
         head_height: inner_full,
         head: Fragment { height: inner_full, spacing_after: 0.0, commands: vec![] },
         tail: FlowNode::Spacer(SpacerNode { height: NonNegative::pt(rest)? }),
     }
     ```
     In `natural_height` (`src/layout/flow.rs:341-350`):
     ```rust
     match node.split(ctx, frame)? {
         Split::Fit { height, fragment } => Ok((height, fragment)),
         Split::Partial { .. } | Split::MoveToNextPage => {
             Err(Error::UnsplittableTallerThanFrame)
         }
     }
     ```
     `natural_height` explicitly rejects `Partial` and returns `Err(Error::UnsplittableTallerThanFrame)`.

### 5.3. Layout Honesty: Why Cell Spacers Do Not Split Across Pages

In body flow, `FlowNode::Spacer` splits across pages: if an 80 pt spacer meets a 50 pt page body, 50 pt is consumed on page 1, and the remaining 30 pt continues at the top of page 2 (`src/layout/paginate.rs:525-554`).

**Inside a table cell, mid-cell splitting is forbidden:**
- **提案书 §2.6** explicitly defines table pagination:
  > “切分单位是整行（含因合并而横跨的逻辑行规则见下），不在行中横切”
  A physical row is atomic; table pagination never slices through the middle of cells or rows.
- If a cell spacer exceeds the inner height of a full page (`height > inner_full`), the row can never fit on any page. It is unsplittable and taller than the frame, so returning `Error::UnsplittableTallerThanFrame` accurately reflects the table model.
- If a cell spacer fits within a full page (`height <= inner_full`), but the row does not fit in the *remaining space* on the current page, `natural_height` succeeds during measurement, and then `split_measured` moves the *entire row* to the next page.

### 5.4. Fragment Placement and Painting

- When building row fragments (`src/layout/table.rs:246-259`), cell fragment commands are offset by `(x + inset, y + inset)`.
- Because `SpacerNode` generates an empty `commands` vector, **no drawing commands are emitted to the canvas** for the spacer itself (no `DrawGlyphRun`, no `StrokePath`, no `FillPath`).
- The spacer acts purely as a geometric box, expanding `row_h = row_h.max(cell_height)` and pushing subsequent rows or following content downward by `height`.

---

## 6. Test Plan

All tests will be added under `tests/document.rs` (public DSL tests) and `src/layout/table.rs` (layout seam verification).

### 6.1. Public Integration Tests (`tests/document.rs`)

1. **`test_cell_spacer_expands_row_and_shifts_neighbor`**
   - **Setup:** A table with two rows.
     - Row 1: `["Header A", "Header B"]`
     - Row 2: `[Cell::spacer(NonNegative::pt(40.0).unwrap()), Cell::new("Data B")]`
     - Row 3: `["Data C1", "Data C2"]`
   - **Oracles:**
     - Top-aligned text: In Row 2, "Data B" baseline is at `row2_top + text_baseline`.
     - Neighbor shift: Row 3's text baseline is located exactly `40.0 pt` below Row 2's top (since 40 pt > font line height). Assert coordinate using `pt_near`.
     - Absence of ink: Filter `page.ops()` to ensure no drawing commands originate from the spacer cell.
2. **`test_cell_spacer_zero_height_fits_and_draws_nothing`**
   - **Setup:** A table containing `Cell::spacer(NonNegative::ZERO)` alongside a standard text cell.
   - **Oracles:**
     - Table renders cleanly to 1 page without error.
     - Row height is governed solely by the text cell's line height.
3. **`test_cell_spacer_taller_than_frame_errors`**
   - **Setup:** Page body height is 100 pt (`PageSize::custom(Size::try_pt(200.0, 100.0).unwrap())`).
     A table cell holds `Cell::spacer(NonNegative::pt(120.0).unwrap())`.
   - **Oracles:**
     - `document(...)` returns `Err(Error::UnsplittableTallerThanFrame)`.
4. **`test_negative_cell_spacer_rejected_early`**
   - **Setup:** Calling `NonNegative::pt(-5.0)` inside the closure.
   - **Oracles:**
     - Returns `Err(Error::NegativeLength)`, proving invariant enforcement before cell creation.

### 6.2. Layout Seam Unit Test (`src/layout/table.rs`)

5. **`cell_allows_spacer`**
   - **Setup:** Mirror `cell_allows_hline` (`src/layout/table.rs:659-670`):
     ```rust
     #[test]
     fn cell_allows_spacer() {
         let spacer = SpacerNode {
             height: NonNegative::pt(10.0).unwrap(),
         };
         Cell::new(vec![FlowNode::Spacer(spacer)]).unwrap();
     }
     ```
   - **Oracle:** Asserts that `table::Cell::new` with `FlowNode::Spacer` succeeds through `CellBody::new` and `check_content`.

---

## 7. Explicit Out-of-Knife List

To maintain strict minimalism and prevent feature creep, the following remain explicitly out of Knife 16:

1. **Multi-content cell stack / closure:** Cells continue to hold exactly one payload (`CellContent`). Callers cannot place both a paragraph and a spacer in the same cell. (A multi-child cell closure is an unconfirmed DSL surface per §2.6).
2. **Cell rich paragraph / blanks:** `Blank`, inline links, and text formatting inside cells remain out of knife.
3. **Cell lists:** `Cell::ul` / `Cell::ol` remain out of knife.
4. **Nested tables / columns:** Forbidden in table cells per §2.6 and `check_content`.
5. **Mid-cell / mid-row page splitting:** Rows remain atomic units of table pagination per §2.6.
6. **Modifying `natural_height`:** `natural_height` continues rejecting `Partial` as `UnsplittableTallerThanFrame`.
7. **Spacer styling / color fills:** Spacers remain strictly transparent vertical blanks per §2.13 ("色块背景不做").
8. **Public table padding and border DSL:** `TableSpec` padding and border default to 0; adding public table setters is out of knife.
