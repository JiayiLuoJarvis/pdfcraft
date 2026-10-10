# Knife 16 — `Cell::spacer` (candidate 1)

**Scope.** Public DSL bridge only: callers can put a §2.13 spacer inside a §2.6 table cell. Layout already has `FlowNode::Spacer`, table-cell allowlist, and `SpacerNode::split`; this knife does not add a second spacer type or change pagination semantics.

**Baseline.** Branch from `feat/text-uri-link` @ `16749da`.

**Autonomy.** All frame defaults adopted with no divergence (no reverse words).

---

## 1. Caller usage sketch

Spacer-only cells sit beside existing `Cell::new`, `Cell::image`, and `Cell::hline`. Same branded height as body flow.

```rust
use quire::{
    document, Cell, NonNegative, PageSize, Size,
    // … FONT, runs helpers as in tests/document.rs
};

let gap = NonNegative::pt(24.0)?;

document(PageSize::custom(Size::try_pt(200.0, 120.0)?), |doc| {
    doc.font(FONT.to_vec())?;
    doc.table(|t| {
        t.row([
            Cell::new("Top"),
            Cell::spacer(gap),
        ]);
        t.row([
            Cell::new("After gap"),
            Cell::new("B"),
        ]);
    });
    Ok(())
})?;

// Second row glyphs start lower on the page by ~gap (row box grew; spacer paints nothing).
```

Zero height is legal and useful as a no-op cell payload:

```rust
t.row([Cell::spacer(NonNegative::ZERO), Cell::new("x")]);
```

Spans compose like other cells:

```rust
Cell::spacer(gap).col_span(NonZeroU16::new(2).unwrap())
```

Negative heights still fail at `NonNegative::pt(...)` / `Length` construction, before `Cell::spacer` runs (same as `Flow::spacer`).

There is **no** nested `Flow` closure on `Cell`; callers cannot stack text + spacer in one public cell this knife.

---

## 2. Types, enum arms, signatures

### Public (`src/document.rs`)

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
enum CellContent {
    Text(String),
    Image(Image, ImageBox),
    HLine(HLine),
    Spacer(Length<NonNegative>), // NEW
}

impl Cell {
    /// A one-by-one cell holding vertical blank space as wide as the cell content box.
    ///
    /// Paints nothing. Height must fit the cell's inner frame at measure time; taller spacers
    /// error with [`Error::UnsplittableTallerThanFrame`] (table rows are not split mid-cell).
    pub fn spacer(height: Length<NonNegative>) -> Self {
        Self {
            content: CellContent::Spacer(height),
            col_span: NonZeroU16::MIN,
            row_span: NonZeroU16::MIN,
        }
    }

    // col_span / row_span unchanged
}
```

Doc comment should **not** claim body-flow cross-page splitting inside a cell; optionally one line contrasting `Flow::spacer` (splittable in body) vs cell measure (fit-only).

### Lower bridge (`Table::lower` in `src/document.rs`)

Extend the exhaustive `match cell.content`:

```rust
CellContent::Spacer(height) => {
    vec![FlowNode::Spacer(SpacerNode { height })]
}
```

`SpacerNode` and `Length<NonNegative>` are already in scope for `Flow::spacer`.

### No new public types

- Reuse `Length<NonNegative>` (not raw `f32`, not `u32`).
- Do **not** add `From<Length<NonNegative>> for Cell` (image/hline also use named constructors; `From` remains text-only).

### Layout (unchanged signatures)

| Item | Location | Action |
| --- | --- | --- |
| `FlowNode::Spacer(SpacerNode)` | `src/layout/flow.rs` | untouched |
| `SpacerNode { height: Length<NonNegative> }` | `src/layout/flow.rs` | untouched |
| `check_content(Container::TableCell, …)` | `src/layout/lower.rs` | untouched (Spacer already allowed) |
| `natural_height(…)` | `src/layout/flow.rs` | untouched (Partial → `UnsplittableTallerThanFrame`) |
| `layout::table::Cell::new(Vec<FlowNode>)` | `src/layout/table.rs` | untouched |

### Re-exports (`src/lib.rs`)

No change required: `Cell` is already re-exported; no new public type names.

---

## 3. Module map

| File | Change |
| --- | --- |
| `src/document.rs` | `CellContent::Spacer`; `Cell::spacer`; `Table::lower` arm; doc comments |
| `tests/document.rs` | New integration test(s) via `document().pages()` |
| `src/layout/table.rs` | Optional `#[test] fn cell_allows_spacer()` mirroring `cell_allows_hline` |

**Untouched (explicit):**

- `src/layout/flow.rs` — split, `natural_height`, `SpacerNode`
- `src/layout/lower.rs` — allowlist matrix, `CellBody`
- `src/layout/paginate.rs` — body spacer split behavior
- `src/layout/table.rs` — measure, `split_measured`, row atomicity (except optional unit test)
- `src/canvas.rs`, `src/backend/**`, `src/lib.rs` (unless doc-only cross-links)
- `CHANGELOG.md` — follow repo habit on merge PR, not required in knife design

Dependency direction unchanged: `document → layout`; no krilla.

---

## 4. Lower + measure behavior

### Lower path

1. Caller builds `Cell::spacer(h)` → stored `CellContent::Spacer(h)`.
2. `Table::lower` maps to `vec![FlowNode::Spacer(SpacerNode { height: h })]` (length 1).
3. `table::Cell::new` → `CellBody::new` → `check_content(TableCell)` — already permits `Spacer`.
4. `CellBody::into_node` wraps as `FlowNode::Stack([Spacer])` (same as other single-child cells).

### Measure / paint

Table measurement uses full inner cell frame `inner_full` (page body full height minus inset; today public tables have zero padding/border so `inner_full` equals page body height).

| Spacer height | `SpacerNode::split` | `natural_height` | Visible effect |
| --- | --- | --- | --- |
| `0` | `Fit`, height 0 | OK | No ops; row height unchanged unless other cells taller |
| `0 < h ≤ inner_full` | `Fit`, height h | OK | Row/cell box grows by h; **no** fill/stroke/glyph from spacer |
| `h > inner_full` | `Partial` (head eats frame, tail shorter spacer) | **`UnsplittableTallerThanFrame`** | Document fails; no partial row on next page |

**Critical honesty (rubric):** §2.13 “可拆” applies to **body flow** pagination, which preserves `Split::Partial`. Cell bodies go through `natural_height`, which **rejects** Partial and MoveToNextPage. A spacer taller than the cell’s full inner frame therefore **does not** continue on the next page inside the cell; it errors like any other unsplittable cell content. This matches §2.6 row atomicity (整行不切).

After successful measure:

- Row height = max of participating cell content heights in that row.
- Spacer-only cell in a row with text neighbors: content is **top-aligned**; extra vertical space is empty row box below shorter cells, not centered spacer ink.
- If the measured row does not fit the **remaining** page body, `split_measured` moves the **whole row** to the next page — same as today. That is row pagination, not mid-cell spacer splitting.

### Body vs cell (document for implementers)

| Context | Oversized spacer |
| --- | --- |
| `Flow::spacer` in body | Splits across pages (`spacer_remainder_leads_next_page` in `paginate.rs`) |
| `Cell::spacer` | Errors at measure if `h > inner_full` |

Do not “fix” cell behavior to match body split in this knife (`split-in-cell` is out of frame).

---

## 5. Test plan

All geometry proofs use **`document()` → `.pages()` → `RecordingCanvas`** and existing helpers (`runs`, `glyph`, `pt_near` from `quire` / `tests/document.rs` patterns). Do not assert spacer-specific canvas ops (there are none).

### A. `cell_spacer_pushes_following_row` (required, `tests/document.rs`)

- Small custom page (e.g. 200×200 pt), fixture font registered.
- Table with two rows, two columns:
  - Row 0: `Cell::new("A")`, `Cell::spacer(NonNegative::pt(H)?)` with `H = 24.0` (or similar).
  - Row 1: `Cell::new("B")`, `Cell::new("C")`.
- Capture baseline: same table but row 0 uses `Cell::spacer(NonNegative::ZERO)` or omit spacer column (control row heights).
- Assert second-row glyph **y** increases by ~`H` vs baseline using `pt_near`.
- Assert page 0 has glyph runs for "B"/"C"; no requirement for stroke/fill from spacer cell alone.

### B. `cell_spacer_only_cell_no_extra_ops` (required)

- Single-row table: `[Cell::spacer(gap), Cell::new("x")]`.
- Compare op count / absence of `StrokePath` / `FillPath` attributable to spacer column (neighbor text still draws glyphs).
- Optional: spacer height 0 → same glyph y as table without spacer column.

### C. `cell_spacer_taller_than_body_errors` (required)

- Page body height `B` (e.g. 40 pt via custom `PageSize`).
- `Cell::spacer(NonNegative::pt(B + 1.0)?)` in one cell, minimal other content so table lowers.
- `document(...)` returns `Err(Error::UnsplittableTallerThanFrame)`.
- Mirror `hline_taller_than_body_fails_before_any_page` style; **do not** expect two pages with remainder spacer (that would be body semantics).

### D. `cell_allows_spacer` (optional, `src/layout/table.rs`)

```rust
Cell::new(vec![FlowNode::Spacer(SpacerNode {
    height: NonNegative::pt(1.0).unwrap(),
})])
.unwrap();
```

Parallel to existing `cell_allows_hline`.

### E. Negative height (optional, cheap)

- `NonNegative::pt(-1.0)` fails in `?` before layout; can live in `document.rs` next to existing spacer negative test or be considered covered by geom tests.

### F. Not in this knife

- Mid-cell split across pages for oversized spacer.
- Multi-child public cell (text + spacer in one `Cell`).
- PDF byte assertions beyond optional smoke `%PDF` with `backend-krilla` (not required for knife proof).

---

## 6. Out-of-knife list

Explicit exclusions (frame + proposal):

| Item | Reason |
| --- | --- |
| Cell rich paragraph, blank fields, URI link in cell | Other knives / §2.6 phased rollout |
| Cell list (`ul`/`ol` in cell) | Separate PR |
| Public multi-content cell / `Cell` + nested `Flow` closure | `stack-cell`; layout can hold `Vec` but DSL not pinned |
| `Cell::space` or raw `f32` height API | Autonomy default is `spacer` + `Length<NonNegative>` |
| Reject zero height | Autonomy default allows `NonNegative::ZERO` |
| Change `natural_height` or table measure to split spacers inside cells | `split-in-cell`; conflicts §2.6 row unit |
| New layout node kind / duplicate spacer struct | Rubric: reuse `FlowNode::Spacer` |
| Touch `check_content` allowlist | Spacer already allowed |
| Body `Flow::spacer` pagination changes | Laziness |
| canvas / qr / svg / toc / named styles / encryption | Non-goals |
| krilla / backend changes | No paint path for spacer |
| Proof by page count only without neighbor y | `page-count-only` |

---

## Implementation checklist (for merge PR)

1. Add enum arm + constructor + lower arm (~15 lines).
2. Add tests A–C (and optional D).
3. `cargo fmt`, `cargo clippy -D warnings`, `cargo test`.
4. No CHANGELOG requirement unless project convention on feature knives says otherwise.
