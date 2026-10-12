# Cell admission: `IntoCellItem` on the cell value

Pole B. `Cell` stays a value you pass to `row` / `header`. It gains `add` for the same element values the body already builds. It does not implement `FlowWriter`, and it has no `columns`, `table`, `page_break`, `keep_together`, or `stack` method.

`Table` and `Columns` stay `IntoFlowItem` only. A call that would put them in a cell fails at compile time. Layout `check_content(Container::TableCell)` stays the backstop for nodes, not a second DSL policy.

## Usage

Everyday cells keep today's shape. Sugar builds a value, then admits it. `col_span` / `row_span` / `fill` still consume the finished cell. Strings still coerce.

```rust
use std::num::NonZeroU16;

use quire::{document, Cell, Color, FlowWriter, PageSize};

let navy = Color::rgb(0x19, 0x2d, 0x74);
let label = String::from("borrowed");

document(PageSize::A4, |doc| {
    doc.font(font_bytes())?;
    doc.table(|t| {
        t.header([Cell::new("H").fill(navy)]);
        t.row([
            "Qty",
            Cell::new("Total").col_span(NonZeroU16::new(2).unwrap()),
            Cell::paragraph(|p| {
                p.text("Fee ").bold("12");
            }),
            Cell::ul(|list| {
                list.item(|item| item.paragraph(|p| p.text(&label)));
            }),
            Cell::hline(|h| h.color(navy)),
            Cell::spacer(8.0),
        ]);
    });
    Ok(())
})?;
```

`Cell::paragraph` / `ul` / `ol` / `image` / `hline` / `spacer` / `new` keep these signatures. Each content sugar is defined as `Cell::empty().add(...)` of the shared value, except `Cell::new` / `From<&str>` (plain text, below).

A helper returns the same value the body would `add`. The cell appends it. Chain order is top to bottom.

```rust
use quire::{Cell, HLine, ListBlock, Paragraph, PlacedImage, Spacer};

fn fee(amount: &str) -> Paragraph {
    Paragraph::build(|p| {
        p.text("Fee ").bold(amount);
    })
}

fn rule(color: quire::Color) -> HLine {
    HLine::build(|h| h.color(color))
}

fn notes<'a>(body: &'a str) -> ListBlock<'a> {
    ListBlock::bullet(move |list| {
        list.item(move |item| item.paragraph(|p| p.text(body)));
    })
}

// Body and cell take the same values.
doc.add(fee("12"));
doc.add(rule(navy));
doc.add(notes("net 30"));
doc.add(Spacer::new(8.0));

t.row([
    Cell::empty()
        .add(fee("12"))
        .add(rule(navy))
        .add(Spacer::new(6.0))
        .add(notes(&label))
        .add(PlacedImage::build(&logo, |image| image.width(24.0)))
        .col_span(NonZeroU16::new(2).unwrap())
        .fill(navy),
]);
```

One-item sugar and `add` of the same value are one implementation. `Cell::ul(f)` is `Cell::empty().add(ListBlock::bullet(f))`. `doc.ul(f)` is `doc.add(ListBlock::bullet(f))`.

These are not cell call sites. They do not compile: `Cell` has no such method, and `Table` / `Columns` do not implement `IntoCellItem`.

```rust
// compile_fail: no method `table` / `columns` / `page_break` / `keep_together` / `stack` on `Cell`
Cell::empty().page_break();

// compile_fail: `Table: IntoFlowItem`, not `IntoCellItem`
Cell::empty().add(Table::build(|t| t.row(["a"])));
Cell::empty().add(Columns::build(|c| { /* ... */ }));
```

A list item is still a `Flow` (`FlowWriter`). `item.table(...)` inside `Cell::ul` still compiles and still fails at layout with `InvalidCellContent { container: "list item", .. }`. That matrix is the item's, not the cell's.

## Types

```rust
/// Value [`Cell::add`] accepts.
///
/// Sealed. Implementors: [`Paragraph`], [`HLine`], [`PlacedImage`], [`Spacer`],
/// [`ListBlock`]. [`Table`] and [`Columns`] implement [`IntoFlowItem`] only.
///
/// Admission stores the value. It does not lower. Paragraphs, lines, images,
/// and spacers lower with the table's parent style and document token.
/// A [`ListBlock`] runs when the cell is stored on a row, on that same parent.
pub trait IntoCellItem<'a>: seal::Sealed {
    fn add_to_cell(self, cell: &mut Cell<'a>);
}
```

The seal is a private trait in `table.rs`. Only that module can add an impl. `flow_writer.rs` keeps its own private seal for `IntoFlowItem`. A type is in a set exactly when that module impls it. `Table` is not sealed for cells, so `cell.add(table)` is a missing-trait error, not `InvalidCellContent`.

```rust
#[derive(Debug)]
#[must_use]
pub struct Cell<'a> {
    pieces: Vec<Piece<ListBlock<'a>>>,
    col_span: NonZeroU16,
    row_span: NonZeroU16,
    fill: Option<Color>,
    errors: ErrorBag,
}

/// One child list. `L` is [`ListBlock`] before `row` / `header`, and
/// `Option<FlowNode>` after store resolves it.
enum Piece<L> {
    Text(String),
    Paragraph(Paragraph),
    Line(HLine),
    Image(PlacedImage),
    Spacer(Length<NonNegative>),
    List(L),
}

impl<L> Piece<L> {
    fn map_list<M>(self, f: impl FnOnce(L) -> M) -> Piece<M> {
        match self {
            Piece::Text(text) => Piece::Text(text),
            Piece::Paragraph(paragraph) => Piece::Paragraph(paragraph),
            Piece::Line(line) => Piece::Line(line),
            Piece::Image(image) => Piece::Image(image),
            Piece::Spacer(height) => Piece::Spacer(height),
            Piece::List(list) => Piece::List(f(list)),
        }
    }
}
```

`map_list` is the only converter. Store is `piece.map_list(|block| block.resolve(parent, errors))`. Adding a variant breaks that match and the lower match. There is no second `CellContent` / `StoredContent` pair.

`Piece::Text` is not an `IntoCellItem`. `Cell::new` / `From<&str>` / `From<String>` / `From<&String>` push it. `lower_paragraph` always writes `align` and `first_line_indent` onto a clone of the parent style (`flow.rs`, `lower_paragraph`). `Flow::text` keeps the parent style, including alignment. Routing `"Qty"` through `Paragraph::build` would change `t.row(["Qty"])`. Strings stay on `TextNode::new`.

```rust
impl<'a> Cell<'a> {
    /// No children. A placed empty cell is a zero-height grid slot, not a hole.
    pub fn empty() -> Self { unimplemented!() }

    /// Appends one admitted element. Order is paint order, top to bottom.
    ///
    /// Twice appends twice. Does not lower and does not bind a list to a flow.
    pub fn add(mut self, item: impl IntoCellItem<'a>) -> Self {
        item.add_to_cell(&mut self);
        self
    }

    pub fn new(text: impl Into<String>) -> Self { unimplemented!() }

    pub fn paragraph(f: impl FnOnce(&mut Paragraph)) -> Self {
        Self::empty().add(Paragraph::build(f))
    }

    pub fn hline(f: impl FnOnce(&mut HLine)) -> Self {
        Self::empty().add(HLine::build(f))
    }

    pub fn image(image: &Image, f: impl FnOnce(&mut ImageBox)) -> Self {
        Self::empty().add(PlacedImage::build(image, f))
    }

    pub fn ul(f: impl FnOnce(&mut List) + 'a) -> Self {
        Self::empty().add(ListBlock::bullet(f))
    }

    pub fn ol(numbering: Numbering, f: impl FnOnce(&mut List) + 'a) -> Self {
        Self::empty().add(ListBlock::numbered(numbering, f))
    }

    pub fn spacer(height: impl IntoNonNegative) -> Self {
        Self::empty().add(Spacer::new(height))
    }

    pub fn col_span(self, n: NonZeroU16) -> Self { unimplemented!() }
    pub fn row_span(self, n: NonZeroU16) -> Self { unimplemented!() }
    pub fn fill(self, color: Color) -> Self { unimplemented!() }
}
```

`add` takes `self` by value so it chains with `col_span` / `fill`. `FlowWriter::add` stays `&mut self`: the document is already borrowed for the whole closure; the cell is a temporary. Same verb, different owner. Not one trait.

`row` / `header` keep `I::Item: Into<Cell<'c>>`.

### Shared values

`Paragraph::build`, `HLine::build`, and `PlacedImage::build` stay the constructors. Cell sugar calls them. Body sugar already does.

```rust
/// Vertical space. Height may be zero.
///
/// A non-finite or negative height is stored on the value and reported when
/// the spacer is admitted, with no node. In a body flow a fitting spacer can
/// split across pages. In a cell the row does not split; taller than the cell's
/// inner frame is still [`ErrorKind::UnsplittableTallerThanFrame`].
#[derive(Debug)]
#[must_use]
pub struct Spacer { /* height or one Error */ }

impl Spacer {
    pub fn new(height: impl IntoNonNegative) -> Self { unimplemented!() }

    /// One length check. Callers choose how to report `Err`:
    /// a flow uses [`Flow::push_err`] so a rejecting flow still records
    /// [`ErrorKind::ContentOutsideSection`]; a cell records onto `cell.errors`.
    pub(crate) fn into_result(self) -> Result<Length<NonNegative>, Error> {
        unimplemented!()
    }
}
```

`FlowWriter::spacer` becomes `self.add(Spacer::new(height))`. `IntoFlowItem for Spacer` calls `into_result`, then `Flow::push` or `Flow::push_err`, so a spacer after an explicit section still becomes `ContentOutsideSection` rather than the length error. `IntoCellItem for Spacer` records `Err` onto `cell.errors` at `add` and pushes `Piece::Spacer` only on `Ok`. No zero-height stand-in. A spacer-only failure still blocks the table via `cell.errors`, and no page is painted. Same visible result as today's zero spacer plus error.

```rust
/// A list not yet tied to a parent flow.
///
/// [`FlowWriter::add`] runs the closure immediately on that flow.
/// [`Cell::add`] stores it. The closure runs when the cell is written into
/// a row or header, on the table's parent flow. Locals borrowed by the
/// closure must live until that `add` or `row` / `header` returns.
#[must_use]
pub struct ListBlock<'a> { /* private seed + Box<dyn FnOnce(&mut List) + 'a> */ }

impl<'a> ListBlock<'a> {
    pub fn bullet(f: impl FnOnce(&mut List) + 'a) -> Self { unimplemented!() }
    pub fn numbered(numbering: Numbering, f: impl FnOnce(&mut List) + 'a) -> Self {
        unimplemented!()
    }

    /// `List::build_on(parent)` once, then [`BuiltList::lower`].
    pub(crate) fn resolve(self, parent: &Flow, into: &mut ErrorBag) -> Option<FlowNode> {
        unimplemented!()
    }
}
```

The seed stores `Numbering`, not layout `ListKind`. Kind conversion and the `list.start` overwrite live only in `BuiltList` (the match now in `IntoFlowItem for BuiltList` and again in `assemble_list`).

```rust
pub(crate) struct BuiltList { list: List, kind: ListKind }

impl BuiltList {
    pub(crate) fn bullet(parent: &Flow, f: impl FnOnce(&mut List)) -> Self {
        unimplemented!()
    }
    pub(crate) fn numbered(parent: &Flow, numbering: Numbering, f: impl FnOnce(&mut List)) -> Self {
        unimplemented!()
    }
    /// One start-merge, then `List::lower`.
    pub(crate) fn lower(self, into: &mut ErrorBag) -> Option<FlowNode> {
        unimplemented!()
    }
}
```

`BuiltList` is `pub(crate)` and is not `IntoFlowItem` or `IntoCellItem`. It is already bound to a parent. Admitting it on a cell would accept a list built on the wrong flow. Callers who have a list return `ListBlock`.

`List::build_on` already does `parent.child()`. Store calls `block.resolve(&self.parent, ...)`, which is one snapshot. That matches today's `assemble_list(self.parent.child(), ...)`, which does not snapshot again. Do not child twice.

`assemble_list` is deleted.

`PlacedImage` gains the shared lower used by `IntoFlowItem::add_to` and by cell lower:

```rust
impl PlacedImage {
    pub(crate) fn lower(self, doc: DocToken, into: &mut ErrorBag) -> Option<FlowNode> {
        self.box_.lower(&self.image, doc, into)
    }
}
```

Cell lower passes `table.parent.doc`. A table built with `Table::build` has a fresh token, so an image in that cell can still be `UnknownImage`. Same rule as today. Live `doc.table` copies the document token at `build_on`.
```

### Store and lower

```rust
struct StoredCell {
    pieces: Vec<Piece<Option<FlowNode>>>,
    col_span: NonZeroU16,
    row_span: NonZeroU16,
    fill: Option<Color>,
}

impl Table {
    fn store(&mut self, cell: Cell<'_>) -> StoredCell {
        self.errors.absorb(cell.errors);
        let parent = &self.parent;
        let errors = &mut self.errors;
        StoredCell {
            pieces: cell
                .pieces
                .into_iter()
                .map(|piece| piece.map_list(|block| block.resolve(parent, errors)))
                .collect(),
            col_span: cell.col_span,
            row_span: cell.row_span,
            fill: cell.fill,
        }
    }
}

fn lower_pieces(
    flow: &Flow,
    into: &mut ErrorBag,
    pieces: Vec<Piece<Option<FlowNode>>>,
) -> Option<Vec<FlowNode>> {
    let mut nodes = Vec::with_capacity(pieces.len());
    let mut ok = true;
    for piece in pieces {
        match lower_piece(flow, into, piece) {
            Some(node) => nodes.push(node),
            None => ok = false,
        }
    }
    // Every piece is lowered, including pieces after a failure.
    ok.then_some(nodes)
}

fn lower_piece(
    flow: &Flow,
    into: &mut ErrorBag,
    piece: Piece<Option<FlowNode>>,
) -> Option<FlowNode> {
    match piece {
        Piece::Text(text) => Some(flow.text(text)),
        Piece::Paragraph(paragraph) => lower_paragraph(paragraph, &flow.style, into),
        Piece::Line(line) => line.lower(into),
        Piece::Image(image) => image.lower(flow.doc, into),
        Piece::Spacer(height) => Some(FlowNode::Spacer(SpacerNode { height })),
        Piece::List(node) => node,
    }
}
```

`lower_stored_rows` passes that `Vec<FlowNode>` to `layout_table::Cell::new`, which runs `check_content(Container::TableCell)`. Do not add a document-side copy of the matrix. Do not unwrap a one-node vec. Measure already wraps every cell through `CellBody::into_node` → `FlowNode::Stack`. One child keeps today's height. Several children use the same stack: a child's `spacing_after` is inserted before the next child when it fits. Default paragraph spacing and line `space_before` / `space_after` are zero, so a default paragraph then a default line sit adjacent.

If any piece returns `None`, the row-set returns `None` and `Table::lower` does not build a `TableNode`. That is today's single-item failure. Sibling pieces still lower so their errors are recorded. `Table::lower`'s existing `failed` flag still blocks when `cell.errors` was non-empty at store (bad spacer). Track checks stay after row lowering.

Empty `pieces` and no error is `Some(vec![])`: a real slot, height 0. It is not `InvalidCellSpan::EmptySlot` (that error is a grid hole).

### When each thing runs

| Step | What runs | Parent |
| --- | --- | --- |
| `Cell::add` / sugar | Store the value. `Spacer::into_result` may record an error. Lists do not run. | None. A free cell has no flow. |
| `FlowWriter::add(ListBlock)` | `resolve` immediately. | `flow_mut()` |
| `Table::store` (`row` / `header`) | Absorb `cell.errors`. `ListBlock::resolve(&table.parent)`. | Table parent, one `child()` inside `build_on` |
| `Table::lower` | `lower_paragraph` / `HLine::lower` / `PlacedImage::lower` / spacer node. List arm is already `Option<FlowNode>`. | `table.parent.style` and `table.parent.doc` |

List closures stay deferred until store so `cell_list_can_borrow_a_table_local` keeps working: the borrow only has to outlive `row(...)`, and the style / `DocToken` are the table's, not a detached default. Do not resolve lists in `Table::lower`. That runs after the table closure returns, so the closure would have to be `'static`, and every list error would move from store-time to lower-time (after earlier rows' paragraph errors). Today all stores finish before any paragraph lower, so a later row's list error is recorded before an earlier row's paragraph error. Keep that order.

Paragraphs stay unlowered until `Table::lower` for the same order. Style would match if they lowered at store (`table.parent.style` is fixed at `build_on`); the schedule is what matters.

## Module map

```text
src/document/table.rs
  Cell, Piece, IntoCellItem + private seal
  impl IntoCellItem for Paragraph, HLine, PlacedImage, Spacer, ListBlock
  Table::store / lower_pieces
src/document/list.rs
  ListBlock, BuiltList (pub(crate))
  List::build_on unchanged
src/document/flow_writer.rs
  IntoFlowItem for Spacer and ListBlock (flow seal)
  PlacedImage::lower
  ul / ol / spacer become add(value)
  page_break / keep_together / stack / columns / table unchanged
src/document/flow.rs
  delete assemble_list
src/document/mod.rs, src/lib.rs
  pub use IntoCellItem, ListBlock, Spacer
src/layout/nesting.rs
  no change
```

Call chain for a list in a cell: `Cell::ul` → `ListBlock` → `Table::store` → `ListBlock::resolve` → `BuiltList::lower`. Three hops, two files (`table.rs`, `list.rs`). A paragraph: `Cell::add` → piece → `lower_paragraph` in `Table::lower`. Image: `PlacedImage::build` → `PlacedImage::lower`.

`layout` still does not depend on `document`. `FlowNode` stays crate-private.

## Forbidden-content policy

| Proposal forbid | At the cell call site | When it still fails at runtime |
| --- | --- | --- |
| `columns` | No `Cell::columns`. `Columns: IntoFlowItem` only. | `check_content(TableCell)` if a node is built by hand. Existing layout test. |
| nested `table` | No `Cell::table`. `Table: IntoFlowItem` only. | Same layout arm. |
| `pageBreak` | No method. Not a value. | Layout arm. List items still call `Flow::page_break` and get `container: "list item"`. |
| `keep_together` | No method. Not a value. | Layout arm. Same item path. |
| `stack` | No method. Not a cell child. | Not produced by this DSL. The measure wrapper `FlowNode::Stack` is `CellBody::into_node`, not a caller stack. |

No `InvalidCellContent` arm inside `Cell::add`. There is no enum of rejected kinds to keep in sync. Enabling a nested table later is one cell-seal impl plus the layout arm, not a reject-path edit.

Item forbids stay on `ListItem::new` → `check_content(Container::ListItem)`. Image in a cell is legal; image in an item is not. Do not check item children with the table-cell matrix.

`stack` around `doc.table(...)` still changes the style `Table::build_on` copies. Cell paragraphs inherit that parent. Per-paragraph `color` / `font_size` stay the in-cell style knobs. Whole-table `keep_together` stays outside the table, as §2.6 already says.

## Invariants

- The cell allow list is the `IntoCellItem` impls in `table.rs`. The body allow list is the `IntoFlowItem` impls in `flow_writer.rs`. They overlap; neither is a subtype of the other.
- `Piece<L>` is the only child vector. Order is admission order.
- Span and fill are fields on `Cell`, copied once in `store`.
- A bad spacer never becomes a node. A good spacer is a `Length<NonNegative>` before it is stored.
- After `store`, the list payload is `Option<FlowNode>`. `lower_piece` does not call `resolve`.
- One failed piece fails that row-set (`None`), after the remaining pieces lower.
- `Cell::new` does not go through `Paragraph::build`.
- `BuiltList` is not admissible.
- Empty content with no error is a zero-height slot.

## Migration

Unchanged call sites: `Cell::new`, `From<&str>` / `String` / `&String`, `paragraph`, `ul`, `ol`, `image`, `hline`, `spacer`, `col_span`, `row_span`, `fill`, `t.row([...])`, `t.header([...])`. `examples/hello.rs` and `examples/bill.rs` stay source-compatible. `Cell<'a>` stays.

`tests/document.rs` stays green without edits, including `table_cell_fill_is_painted`, `cell_image_and_last_page_seal_share_coordinates`, `hline_in_list_item_and_cell`, the `cell_spacer_*` tests, the `cell_paragraph_*` tests, `cell_list_can_borrow_a_table_local`, `cell_list_marker_at_cell_x`, `cell_list_item_rejects_forbidden_kids`, `cell_list_narrower_than_marker_is_invalid_geometry`, `a_cell_error_is_kept_when_the_table_also_fails`.

Add:

- Sugar versus value: `Cell::paragraph(f)` and `Cell::empty().add(Paragraph::build(f))` lower to the same node. Same for `HLine::build`, `PlacedImage::build`, `Spacer::new`, `ListBlock::bullet`.
- One cell with a paragraph then a line: glyphs above the stroke; default gap is zero.
- `Cell::ul` still borrows a table-closure local (existing test).
- Doctest `compile_fail` on `Cell::add(Table::build(...))`, `Cell::add(Columns::...)`, and `Cell::empty().page_break()` / `.keep_together(...)` / `.columns(...)` / `.table(...)` / `.stack(...)`.
- One of each `IntoCellItem` in a cell still passes `layout_table::Cell::new` (the layout matrix accepts what the trait admits).

`CHANGELOG.md`: additive `Cell::empty`, `Cell::add`, `IntoCellItem`, `Spacer`, `ListBlock`. Cells may hold more than one child, top to bottom, still one row for §2.6. One-item sugars keep their signatures and now call the shared builders. `FlowWriter::ul` / `ol` / `spacer` admit `ListBlock` / `Spacer`; behavior of those sugars is unchanged.

Mark the cell follow-up done in `docs/flow-writer-spec.md` and `docs/deref-flow-api.md` (the "下一刀" line). Do not edit `docs/提案书.md`.

## Not in this change

- `Cell: FlowWriter`, an internal cell `Flow`, or `flow_mut` on `Cell`.
- A public `ContentWriter` supertrait. `add`'s bound would be either the wide set or the narrow set, not both.
- Runtime `InvalidCellContent` for a method the cell type does not have.
- Nested tables, limits, letter fixtures, splitting `table.rs` / `cell.rs`.
- Changing item `FlowWriter`, row-unit pagination, or the nesting matrix.
- `From<Paragraph> for Cell` or `add("text")`. Those would be a second string/paragraph door.
