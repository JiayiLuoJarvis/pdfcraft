# Rationale — candidate 1 (Pole A: cell content is a `&mut Flow`)

## Problem

Body content goes in through `FlowWriter`: sugar methods that build independent element values (`Paragraph::build`, `HLine::build`, `PlacedImage::build`, `List::build_on`) and push them through `add(impl IntoFlowItem)`. Table cells run on a separate path. There are six free constructors (`Cell::paragraph/ul/ol/image/hline/spacer`), a single-item `CellContent<'a>` enum that `Table::store` copies into a parallel `StoredContent`, a boxed `DeferredList<'a>` because a free `Cell::ul` has no parent flow, and `lower_stored_rows` that lowers again at `Table::lower`. The constraints: the 提案书 §2.6 product rules must stay (cells allow paragraph, hline, spacer, image, list; they forbid columns, pageBreak, keep_together, and nested table). Nested lists must see the table's style and doc token. Realistic table closures that borrow locals must keep compiling. `t.row(["a", "b"])` and the span/fill chains must survive. Layout already takes `Vec<FlowNode>` per cell and already enforces the matrix in `nesting::check_content`. The shape isn't obvious because a free cell *value* has no parent flow, and that missing flow is the only reason the deferred-closure machinery exists.

## Usage (caller's view)

```rust
doc.table(|t| {
    t.header(["Item", "Qty"]);
    t.row([
        t.cell(|c| {
            c.paragraph(|p| { p.text("Pen "); p.bold("set"); });
            c.ul(|l| l.item(|i| i.paragraph(|p| p.text("black"))));
        }),
        Cell::new("12").fill(GREY),
    ]);
    for line in &lines {
        t.row([Cell::new(&line.name), t.cell(|c| c.add(money(line.amount)))]);
    }
});
```

There is one rule to teach: a cell is a flow, just like a list item (`List::item(|i: &mut Flow|)`) and a column (`Columns::col(|c: &mut Flow|)`). Strings stay strings. Factories return element values and go in through `add`. See `design.md` §1 for the hello.rs header, the borrow-a-local case, and what a forbidden child looks like.

## Shape

- **Data first.** `Table` rows become `Vec<Vec<layout_table::Cell<FlowNode>>>`, which are lowered and matrix-checked when they are placed. A public `Cell` is plain data with no lifetime: `Text(String) | Checked(layout cell) | Failed`, plus span, fill, and an `ErrorBag`. `StoredCell`, `StoredContent`, `CellContent<'a>`, `DeferredList`, `lower_stored_rows`, and `assemble_list` are deleted. Each dominant access pattern maps straight onto the structure. Writing content goes `Flow` → nodes. Placing a row moves the cell and applies span. Lowering the table moves rows into `TableSpec`. Nothing gets re-matched or re-lowered.
- **`Table::cell(&self, f: FnOnce(&mut Flow)) -> Cell` is the writer-then-freeze point.** It runs `f` on `self.parent.child()`, so style and doc token are correct by construction, then calls `layout_table::Cell::new(nodes)`. That call is the existing `CellBody` / `check_content(TableCell)` gate. Validate at the boundary and trust the type inside (per boundary-discipline): once a cell is stored, it is valid.
- **Shared build results with no new trait.** The closure gets the real `Flow`, so every sugar and `add` comes from the `FlowWriter` default methods. There is no `IntoCellItem` and no second set of impls (per single source of truth).
- **One forbid rule.** The §2.6 matrix stays in `layout/nesting.rs`, shared with list items, columns, and bands. The document side keeps no list of its own (avoids the hand-synced-list red flag).
- **Eager beats deferred.** Running the closure inside the `row(...)` argument list removes the reason for `Box<dyn FnOnce + 'a>`. Every borrow that is valid until `row` is also valid at `cell`.
- **Interface depth.** The public surface goes from six constructors plus a lifetime to one method. That method hides lowering, list parenting, multi-block stacking, and matrix checking. The caller learns nothing new, because `&mut Flow` is already the crate's container-slot type.
- **Deliberately not done:** no cell-specific writer type, no early rejection inside sugar overrides, no change to `layout/`, and no fix for detached `Table::build` and image doc tokens (out of theme).

## Synthesis decision

TBD (arena).

## Tradeoffs accepted

- We accept that `t.cell(|c| c.columns(..))` compiles and fails at `document()` with `InvalidCellContent { container: "table cell" }`, in exchange for one writer type and one matrix across every nested container. List items already work this way.
- We accept that a helper returning a *cell* must take `&Table`, in exchange for no lifetime on `Cell` and no boxed closures. Helpers returning *content* (`Paragraph`, `HLine`, `PlacedImage`) don't need it.
- We accept one extra level at old one-liner sites (`Cell::paragraph(f)` becomes `t.cell(|c| c.paragraph(f))`), in exchange for deleting six constructors and getting multi-block cells.
- We accept that `Cell::new` and `t.cell(|c| c.paragraph(..))` overlap for plain text. `Cell::new` is the only form that works in `From<&str>` arrays, and it doesn't need a parent flow.
- We accept that a `Cell` built by `t.cell` and then dropped without `row` loses its errors. `#[must_use]` covers it, the same way it does today.
- We accept that cell content errors are recorded earlier (at `t.cell` instead of `Table::lower`). All of them still surface at `document()`. Only the relative order inside one table can change.

## Alternatives considered

- **`Cell` itself implements `FlowWriter` over an owned `Flow`** (`let mut c = t.cell(); c.paragraph(..); t.row([c])`). Rejected. It puts `page_break` and the other sugars on the `Cell` type permanently and mixes content methods with the consuming span/fill chains. It also forces an imperative "build, then place" style that breaks the array form. The interface gets wider and hides nothing more.
- **Deferred free constructor `Cell::build(|c: &mut Flow|) -> Cell<'a>` run at `row`.** Same writer, no `&Table` needed. Rejected. It keeps a boxed closure and a public lifetime, delays the matrix check until placement, and keeps the deferred machinery that eager placement makes unnecessary. Its one advantage, helpers that return cells without `&Table`, is a small convenience.
- **A cell writer newtype that overrides `columns` / `table` / `page_break` / `keep_together` to reject immediately.** Rejected. The overrides would be a second copy of the §2.6 matrix, kept in sync by hand with `check_content`. That gives earlier errors at the cost of a hand-synced list.
- **Row-time builder `t.row_with(|r| r.cell(..))`.** Rejected. It loses `t.row(["a", "b"])` and creates two ways to write a row.
- **Narrow sealed `IntoCellItem` admission (Pole B).** Rejected for this candidate. It needs a second impl list that parallels `IntoFlowItem`, and it makes cells the one container whose writer differs from list items and columns.

## Open questions and risks

- `t.row([t.cell(..), t.cell(..)])` with `t: &mut Table` compiles through two-phase borrows. I checked this with a standalone rustc 2021 reduction that has the same `&self` / `&mut self` + generic `IntoIterator` signatures and a closure that borrows a local. It still needs confirming against the real `Table` in the spike.
- Should an empty cell `t.cell(|_| {})` be allowed (zero nodes pass `check_content`), or rejected? Today it can't be expressed.
- Should `place` attach the row/column `CellIndex` location to absorbed content errors such as `InvalidCellContent`? It is known at `row` time, and it would make the new runtime rejection easier to find.
- Is losing the compile-time absence of `Cell::columns` acceptable to the product owner, given that list items already chose runtime rejection?
- Can any test assert `error.first()` across a cell error and a table-level error in the same table, so that the earlier recording changes which one comes first?

## Next implementation step

Spike `Table::cell` plus `place` in `table.rs` against two migrated tests (`cell_list_can_borrow_a_table_local` and `cell_paragraph_pins_bold_blank_and_link`) to confirm the borrow pattern on real types. After that, delete the stored enums.
