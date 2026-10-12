# Candidate 1 — Pole A: cell content is a `&mut Flow`, frozen by `Table::cell`

Base: `feat/flow-writer` @ `c4a5831`. Design package only; no source edits.

One sentence: **a cell's content slot is the same `&mut Flow` writer that list items, columns, keep-together, and stack already hand out; `Table::cell` runs it eagerly on the table's child flow, lowers, checks against the table-cell matrix, and returns a plain-data `Cell` (no lifetime) that `row` / `header` place.**

---

## 1. Usage first

### 1.1 Quickstart (what the README teaches)

```rust
use quire::{Cell, FlowWriter, Numbering};

doc.table(|t| {
    t.header(["Item", "Qty"]);                     // strings stay strings
    t.row([
        t.cell(|c| {                                // a cell is a flow
            c.paragraph(|p| { p.text("Pen "); p.bold("set"); });
            c.ul(|l| {
                l.item(|i| i.paragraph(|p| p.text("black")));
                l.item(|i| i.paragraph(|p| p.text("blue")));
            });
        }),
        Cell::new("12").fill(Color::rgb(240, 240, 240)),
    ]);
    t.row([Cell::new("Total").col_span(two)]);
});
```

What the reader learns: *inside a cell you write exactly what you write in a list item or a column.* The `c` is a `&mut Flow`; every `FlowWriter` sugar and `add` is there. Span / fill are value chains on the returned `Cell`, unchanged from today.

### 1.2 Call site: factory helper returning an element value (`add` path)

```rust
fn money(v: Decimal) -> Paragraph {
    Paragraph::build(|p| { p.align_end(); p.text(v.to_string()); })
}
fn rule() -> HLine { HLine::build(|h| h.thickness(0.5)) }

doc.table(|t| {
    for line in &invoice.lines {
        t.row([
            Cell::new(&line.name),
            t.cell(|c| { c.add(money(line.amount)); c.add(rule()); }),
        ]);
    }
});
```

Same `Paragraph::build` / `HLine::build` / `PlacedImage::build` values and the same `IntoFlowItem` impls as the body. No `IntoCellItem`, no `Cell::paragraph`.

### 1.3 Call site: header row with fills (today's `examples/hello.rs`)

```rust
let head = |t: &Table, s: &str| {
    t.cell(|c| c.paragraph(|p| { p.color(WHITE); p.bold(s); })).fill(DARK)
};
t.header([head(t, "Code"), head(t, "Item"), head(t, "Notes"), head(t, "Qty")]);
```

A helper that returns a *cell* takes `&Table`. A helper that returns *content* returns an element value and the caller wraps it with `t.cell(|c| c.add(..))`.

### 1.4 Call site: borrowing table-closure locals (today's `cell_list_can_borrow_a_table_local`)

```rust
doc.table(|t| {
    let label = String::from("borrowed");
    t.row([t.cell(|c| c.ul(|l| l.item(|i| i.paragraph(|p| p.text(&label)))))]);
});
```

The closure runs **now**, inside the argument list of `row`. Anything alive at the `row` call is alive at the `cell` call, so "borrow until row" becomes "borrow until cell" — a strict superset. No boxed closure, no `'a`.

### 1.5 What a forbidden child looks like

```rust
t.row([t.cell(|c| c.page_break())]);
// document() -> Err, first: InvalidCellContent { container: "table cell", node: "page break" }
```

Same for `c.columns(..)`, `c.table(..)`, `c.keep_together(..)`, and for those nested in `c.stack(..)` (the matrix recurses into stacks). This is the exact rule list items already enforce (`cell_list_item_rejects_forbidden_kids`).

---

## 2. Types and signatures (derived from §1)

### 2.1 Public surface delta

| Before | After |
| --- | --- |
| `pub struct Cell<'a>` | `pub struct Cell` (no lifetime) |
| `Cell::new`, `From<&str/String/&String>` | unchanged |
| `Cell::col_span / row_span / fill` | unchanged |
| `Cell::paragraph / ul / ol / image / hline / spacer` | **deleted** |
| — | `Table::cell(&self, f: impl FnOnce(&mut Flow)) -> Cell` |
| `Table::row / header<'c, I> where I::Item: Into<Cell<'c>>` | `row / header<I> where I::Item: Into<Cell>` |

Net: −6 constructors, −1 lifetime parameter, +1 method. `FlowWriter`, `IntoFlowItem`, `Flow` untouched.

### 2.2 `src/document/table.rs` sketch

```rust
/// Rows being added to a table.
pub struct Table {
    pub(crate) parent: Flow,
    /// Already lowered and checked. `Table::lower` only assembles geometry.
    pub(crate) header: Vec<Vec<layout_table::Cell<FlowNode>>>,
    pub(crate) body: Vec<Vec<layout_table::Cell<FlowNode>>>,
    pub(crate) repeat_header: bool,
    pub(crate) tracks: Option<Vec<Track>>,
    pub(crate) padding: Length<NonNegative>,
    pub(crate) border: Border,
    /// Any entry blocks lowering. Cell content errors land here at `row` / `header`.
    pub(crate) errors: ErrorBag,
}

impl Table {
    /// Writes one cell's content and returns the cell for [`Table::row`] or [`Table::header`].
    ///
    /// `f` runs immediately on a flow that has this table's text style and
    /// document. Everything [`FlowWriter`] offers is available. A cell may hold
    /// paragraphs, lines, spacers, images, lists, and `stack` groups. Columns,
    /// page breaks, keep-together groups, and nested tables are
    /// [`ErrorKind::InvalidCellContent`]; the error is reported by `document()`
    /// like every other content error.
    ///
    /// Content taller than the cell's inner full frame is
    /// [`ErrorKind::UnsplittableTallerThanFrame`]. A spacer inside a cell does
    /// not split across pages.
    pub fn cell(&self, f: impl FnOnce(&mut Flow)) -> Cell {
        let mut flow = self.parent.child();
        f(&mut flow);
        let (nodes, mut errors) = flow.into_parts();
        // TODO: failed child flow => skip the check, keep Failed (the bag is non-empty).
        let content = match layout_table::Cell::new(nodes) {   // check_content(TableCell)
            Ok(checked) => CellContent::Checked(checked),
            Err(error) => { errors.record(error); CellContent::Failed }
        };
        Cell::with(content, errors)
    }

    pub fn header<I>(&mut self, cells: I) where I: IntoIterator, I::Item: Into<Cell> {
        let row = cells.into_iter().map(|c| self.place(c.into())).collect();
        self.header.push(row);
    }

    pub fn row<I>(&mut self, cells: I) where I: IntoIterator, I::Item: Into<Cell> { /* same */ }

    /// Absorbs the cell's errors and applies span / fill. Text cells get the table style here.
    fn place(&mut self, cell: Cell) -> layout_table::Cell<FlowNode> {
        self.errors.absorb(cell.errors);
        let checked = match cell.content {
            CellContent::Text(text) => layout_table::Cell::new(vec![self.parent.text(text)]),
            CellContent::Checked(checked) => Ok(checked),
            // Placeholder keeps the row's span sum so `lower` reports no
            // spurious ColumnWidths; the table is already blocked by `errors`.
            CellContent::Failed => layout_table::Cell::new(Vec::new()),
        };
        // TODO: Err arm records into self.errors and returns the empty placeholder.
        not_implemented!()
            // checked.with_span(cell.col_span, cell.row_span).with_fill(cell.fill)
    }

    pub(crate) fn lower(self, into: &mut ErrorBag) -> Option<FlowNode> {
        // As today, minus `lower_stored_rows`: header/body are moved straight
        // into TableSpec. The `failed` early-out stays the single gate.
        not_implemented!()
    }
}

/// One table cell: content plus span and fill.
///
/// Make one with [`Table::cell`], or from a string with [`Cell::new`] / `From`.
#[derive(Debug)]
#[must_use]
pub struct Cell {
    content: CellContent,
    col_span: NonZeroU16,
    row_span: NonZeroU16,
    fill: Option<Color>,
    errors: ErrorBag,
}

#[derive(Debug)]
enum CellContent {
    /// From `Cell::new` / `From`. No table yet; styled at `place`.
    Text(String),
    /// From `Table::cell`. Lowered and passed the table-cell matrix.
    Checked(layout_table::Cell<FlowNode>),
    /// From `Table::cell`. The reason is in `Cell::errors`.
    Failed,
}

impl Cell {
    pub fn new(text: impl Into<String>) -> Self { /* Text */ }
    pub fn col_span(self, n: NonZeroU16) -> Self { /* unchanged */ }
    pub fn row_span(self, n: NonZeroU16) -> Self { /* unchanged */ }
    pub fn fill(self, color: Color) -> Self { /* unchanged */ }
}
impl From<&str> for Cell { .. }  impl From<String> for Cell { .. }  impl From<&String> for Cell { .. }
```

Deleted from `table.rs`: `StoredCell`, `StoredContent`, `CellContent<'a>` (6 variants), `DeferredList`, its `Debug` impl, `lower_stored_rows`, and the six constructors. `assemble_list` in `flow.rs` loses its last caller and is deleted (body lists already go through `List::build_on` + `BuiltList`).

### 2.3 Invariants and where they live

| Invariant | Encoded by |
| --- | --- |
| A stored table cell obeys §2.6 | Type: `Table` rows hold `layout_table::Cell<FlowNode>`, constructible only via `CellBody::new` → `check_content(TableCell)`. One matrix, `layout/nesting.rs`, unchanged. |
| Cell paragraphs / lists / images use the table's style and doc token | `Table::cell` writes on `self.parent.child()` — the same flow `build_on` captured from the live parent. |
| Cell build results are body build results | The closure gets `&mut Flow`; sugars and `add` are `FlowWriter` defaults; there is no other cell entry point. |
| Any cell error blocks the table | `place` absorbs into `Table::errors`; `Table::lower`'s existing `failed` gate. |
| `Cell` holds no borrow | No lifetime parameter; content is frozen nodes or a `String`. |

---

## 3. Module map

| File | Change |
| --- | --- |
| `src/document/table.rs` | Rewrite `Cell` section and `store`→`place`; add `Table::cell`; drop stored enums and `lower_stored_rows`. Net shrink. |
| `src/document/flow.rs` | Delete `assemble_list` (dead). |
| `src/document/mod.rs` | Drop `assemble_list` from `pub(crate) use`. Public re-exports unchanged (`Cell` still exported). |
| `src/document/flow_writer.rs` | Doc only: `FlowWriter` rustdoc lists `Table::cell` among places that hand out a `&mut Flow`. |
| `src/layout/**` | **No change.** `nesting.rs` matrix and `layout_table::Cell::new` already do the work. |
| `tests/document.rs`, `examples/hello.rs`, `src/document/tests.rs` | Call-site migration (§5). |
| `CHANGELOG.md`, `docs/flow-writer-spec.md`, `docs/deref-flow-api.md` | Breaking note; mark the Cell follow-up landed. |

Data path, three files max: `table.rs` (`Table::cell`) → `flow_writer.rs` (sugars/`add` on `Flow`) → `layout/nesting.rs` (`check_content`).

---

## 4. Forbidden-content policy

- **Where:** one rule, `nesting::check_content(Container::TableCell, ..)`, reached only through `layout_table::Cell::new`. No second list on the document side.
- **When:** at `Table::cell` return (recorded in the cell's bag), moved into the table's bag at `row` / `header`, surfaced by `document()` with the deferred-error model. Same as list items, which check in `List::item`.
- **What:** `InvalidCellContent { container: "table cell", node: "columns" | "page break" | "keep together" | "table" }`. Recurses into `stack`.
- **Not done:** no overriding of `FlowWriter` defaults to reject early, no cell-specific writer type. Both would duplicate the matrix (see RATIONALE).
- **Tradeoff stated plainly:** `t.cell(|c| c.columns(..))` compiles. Today it cannot be written. This matches every other nested container in the crate.

## 5. Migration

Mechanical rewrite table (all in-repo call sites):

| Old | New |
| --- | --- |
| `Cell::paragraph(f)` | `t.cell(\|c\| c.paragraph(f))` |
| `Cell::ul(f)` | `t.cell(\|c\| c.ul(f))` |
| `Cell::ol(n, f)` | `t.cell(\|c\| c.ol(n, f))` |
| `Cell::image(img, f)` | `t.cell(\|c\| c.image(img, f))` |
| `Cell::hline(f)` | `t.cell(\|c\| c.hline(f))` |
| `Cell::spacer(h)` | `t.cell(\|c\| c.spacer(h))` |
| `Cell<'_>` in signatures | `Cell` |
| `Cell::new`, `"x"`, `.col_span/.row_span/.fill` | unchanged |

Tests:

- All `tests/document.rs` cell tests keep their assertions; only constructors change. `cell_list_can_borrow_a_table_local` keeps its name and passes trivially (eager).
- `cell_list_item_rejects_forbidden_kids`: rewrite the harness `move |list| list.item(item)` inside `t.cell(|c| c.ul(..))`; unchanged expectations.
- **New** `cell_rejects_forbidden_kids`: `columns`, `page_break`, `keep_together`, `table`, and `stack(|s| s.page_break())` each yield `InvalidCellContent { container: "table cell", .. }`.
- **New** `cell_holds_several_blocks`: `t.cell(|c| { c.paragraph(A); c.hline(..); c.paragraph(B); })` — assert `RecordingCanvas` y-order inside one cell.
- **New** `cell_add_matches_sugar`: `t.cell(|c| c.add(Paragraph::build(f)))` vs `t.cell(|c| c.paragraph(f))` record identical pages.
- **New** `cell_stack_scopes_style`: `c.stack(|s| s.color(RED), |s| s.paragraph(..))` — style applies inside the cell only.

CHANGELOG (Breaking): "`Cell::paragraph`, `ul`, `ol`, `image`, `hline`, and `spacer` are removed. Write `t.cell(|c| c.paragraph(..))` and so on: a cell is a flow with the same methods as a list item. A cell may now hold several blocks. `Cell` no longer has a lifetime parameter. Columns, page breaks, keep-together groups, and nested tables inside a cell are reported as `InvalidCellContent`."
