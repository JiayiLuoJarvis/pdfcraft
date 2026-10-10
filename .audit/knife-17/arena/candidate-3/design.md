# Knife 17 — cell paragraph and cell list

Baseline: `feat/cell-spacer` tip (knife 16 included). Public `Cell` stays one payload. Layout already accepts `FlowNode::Text` and `FlowNode::List` in a table cell. This knife only opens those two arms on the document DSL.

Autonomy defaults, all kept: `Cell::paragraph` closure, `Cell::new` plain string, paragraph `text` / `bold` / `blank*` / `link`, `Cell::ul` / `Cell::ol`, list closure deferred to `Table::lower`, one knife (not 17a/17b), no multi-kid cell, too-tall stays `UnsplittableTallerThanFrame`.

## Usage sketch

```rust
doc.font(bytes)?;
doc.bold_font(bytes)?;
doc.table(|t| {
    t.header([Cell::paragraph(|p| {
        p.text("Item");
        p.bold(" name");
    })]);
    t.row([
        Cell::new("plain"), // unchanged: one body-face string
        Cell::paragraph(|p| {
            let label = "see"; // borrowed; the closure runs inside Cell::paragraph
            p.text(label);
            p.blank_em(1);
            p.link("https://example.test/a", |p| p.text("details"));
            p.blank_to_end();
        }),
        Cell::hline(|h| h.color(Color::rgb(9, 9, 9))),
        Cell::spacer(NonNegative::pt(8.0).unwrap()),
    ]);

    let owned = String::from("first");
    t.row([Cell::ul(move |list| {
        list.item(|item| item.paragraph(|p| p.text(&owned)));
        list.item(|item| {
            item.paragraph(|p| p.text("nested parent"));
            item.ul(|inner| inner.item(|n| n.paragraph(|p| p.text("child"))));
        });
    })]);
    t.row([Cell::ol(Numbering::Decimal, |list| {
        list.item(|item| item.paragraph(|p| p.text("one")));
        list.item_marked("(c)", |item| item.paragraph(|p| p.text("custom")));
    })]);
});
```

`Cell::paragraph` runs its closure before it returns, same as `Cell::hline`. Borrows of locals that outlive that call are valid.

`Cell::ul` / `Cell::ol` do not run the closure. They store it and `Table::lower` runs it after the `table` closure has returned. The stored closure is `'static`: move owned data in, or use string literals. A borrow of a local declared inside the `table` closure will not compile, and neither will a borrow of an outer local unless that borrow is `'static`. That is the cost of keeping `Cell` lifetime-free (see seam).

Spans stay as they are: `Cell::paragraph(...).col_span(n)`.

## Types and signatures

No new public type. `Paragraph`, `List`, `Numbering`, `Blank` stay the callers' types. `lib.rs` re-exports do not change.

```rust
impl Cell {
    pub fn new(text: impl Into<String>) -> Self; // unchanged; CellContent::Text

    pub fn paragraph(f: impl FnOnce(&mut Paragraph)) -> Self;

    pub fn ul(f: impl FnOnce(&mut List) + 'static) -> Self;

    pub fn ol(numbering: Numbering, f: impl FnOnce(&mut List) + 'static) -> Self;

    pub fn col_span(self, n: NonZeroU16) -> Self;
    pub fn row_span(self, n: NonZeroU16) -> Self;
}
```

`From<&str> for Cell`, `From<String>`, and `From<&String>` still call `Cell::new`. `Table::row` / `Table::header` still take `I::Item: Into<Cell>`.

Private storage:

```rust
enum CellContent {
    Text(String),
    Image(Image, ImageBox),
    HLine(HLine),
    Spacer(Length<NonNegative>),
    Paragraph(Paragraph),
    List(DeferredList),
}

enum DeferredList {
    Bullet(Box<dyn FnOnce(&mut List) + 'static>),
    Ordered(Numbering, Box<dyn FnOnce(&mut List) + 'static>),
}
```

`Cell` today is `Debug + Clone + PartialEq + Eq`. A `FnOnce` box is none of those. Drop `Clone + PartialEq + Eq` on `Cell` and `CellContent`. Keep `#[must_use]`. Implement `Debug` by hand: paragraph arm delegates to `Paragraph`; list arm prints `Bullet` or `Ordered(Decimal)` and does not print the closure. Add `#[derive(Debug)]` on `Paragraph` so that arm can print items, indent, and the stored error. Do not add `Clone` / `Eq` to `Paragraph` (`Error` and `InlineItem` are not `Eq`). Nothing in this repo clones or `==` a document `Cell`; `col_span` / `row_span` take `self` by value.

`Cell::paragraph` builds `Paragraph::default()`, runs `f`, stores the whole `Paragraph` (items, `first_line_indent`, `error`). `Blank` is only a borrow during `f`. Do not store it.

Do not genericize `Cell` over the closure type. A row array mixes `Cell::new` and `Cell::ul`, so the list arm is one `dyn` type.

### Shared lower helpers

Both live as private functions in `document.rs`. `Flow::paragraph` and `Table::lower` call the same paragraph helper. `Flow::ul` / `Flow::ol` and `Table::lower` call the same list helper.

```rust
fn lower_paragraph(paragraph: Paragraph, parent: &Style) -> Result<FlowNode> {
    // paragraph.error → Err, before any node
    // clone parent, set first_line_indent, TextNode::from_items
}

fn numbering_kind(numbering: Numbering) -> MarkerKind { /* same five arms as Flow::ol */ }

fn assemble_list(
    parent: Flow,
    kind: ListKind,
    f: impl FnOnce(&mut List),
) -> Result<FlowNode> {
    // List { parent, items: vec![] }; f(&mut list); list.lower(kind)
}
```

`Flow::ol` and the ordered cell arm both use `numbering_kind` and `start: NonZeroU32::MIN`. No public `start`.

`Table::lower` still emits `vec![one FlowNode]` per cell:

| Arm | Node |
| --- | --- |
| `Text` | `flow.text(text)` → `TextNode::new` (body face, no blank, no link) |
| `Paragraph` | `lower_paragraph(paragraph, &flow.style)?` → `FlowNode::Text` |
| `List::Bullet(f)` | `assemble_list(flow.child(), ListKind::Bullet, f)?` |
| `List::Ordered(n, f)` | `assemble_list(flow.child(), ListKind::Ordered { style: numbering_kind(n), start: MIN }, f)?` |
| image / hline / spacer | unchanged |

`flow.child()` copies the enclosing flow's `Style` and `DocToken`. Do not build the list from `Flow::new(default_style(), ...)`.

The paragraph node is `FlowNode::Text`, not `FlowNode::Paragraph`. `TextNode::from_items` stays unwrapped until `measure` knows `inner_w`. Emitting `FlowNode::Paragraph` would wrap at the wrong width. `check_content` allows both names as `"paragraph"`; the DSL still must emit `Text`.

`CellBody::new` / `table::Cell::new` stay the only allowlist gate. One public payload does not grow `Vec` exposure.

## Module map

| Location | This knife |
| --- | --- |
| `src/document.rs` | `Cell::paragraph` / `ul` / `ol`; `CellContent` arms; `DeferredList`; `Debug`; `lower_paragraph`, `numbering_kind`, `assemble_list`; `Flow::paragraph` and `Flow::list` call those helpers; `Table::lower` match arms |
| `src/lib.rs` | no new export |
| `src/layout/lower.rs` | no edit. `TableCell` already allows text, paragraph, list, spacer, hline, image. `ListItem` already rejects table, columns, page break, keep-together, image |
| `src/layout/paragraph.rs` | no edit. Bold face, blanks, link rects, wrap-at-`avail.width` |
| `src/layout/list.rs` | no edit. Marker column 18 pt, hanging indent, nested lists, item `Partial` in body flow |
| `src/layout/table.rs` | no edit. `measure` → `natural_height` |
| `src/layout/flow.rs` | no edit. `natural_height` stays Fit-only |
| `src/error.rs` | no new variant |
| `tests/document.rs` | recording tests below |

Dependency direction stays `document → layout`. No `krilla` outside `backend/`.

## List ↔ Flow seam

**Choice: defer the builder closure until `Table::lower(&Flow)`.** Not `table-child-flow`.

`List::item` calls `parent.nested` immediately, and `List` holds a `Flow`. A free `Cell::ul` has no `Flow`. `Flow::table` builds a `Table` with no parent flow, runs the caller, then `table.lower(self)`. That `&Flow` is the first moment a cell list can legally call `flow.child()`.

Sequence at lower, same as `Flow::list`:

`assemble_list` → `List::item` / `item_marked` → item `Flow::nested` → `ListItem::new` → `List::lower` → `FlowNode::List` → `vec![node]` → `table::Cell::new` → `check_content(TableCell)`.

Nested `item.ul` / `item.ol` run during that closure against the item `Flow`. They are not deferred again. `item.paragraph` is the body paragraph path, so bold, blank, and link inside an item need no cell-specific text API.

`'static` rather than `Cell<'a>` / `Table<'a>`: `Cell::new` is a free value (`let c = Cell::new("Total"); c.col_span(...)`). A lifetime parameter is not determined by `new`'s arguments, so those bindings stop inferring, and `Into<Cell<'a>>` no longer unifies a `'static` text cell with a borrowing list cell. `'static` keeps every existing `Cell::new` / `From<&str>` signature. Outer data moves into the closure. Locals born inside the `table` closure cannot be borrowed either way, because `lower` runs after that closure returns.

Item errors (`InvalidCellContent`, empty `item_marked`) and paragraph errors (`EmptyLink`, `RunAfterFillBlank`) surface from `Table::lower`, still inside `Flow::table`, before any page is painted. Same observable moment as `Cell::image`.

## natural_height

Do not change `natural_height` or `measure`.

Public tables lower through `TableSpec::new`: padding 0, border width 0, so inset is 0. For each placed cell, `measure` sets

```text
inner_w     = spanned column width
frame.height = frame.full_height = page body height
```

and calls `natural_height(body.into_node(), ...)`. `CellBody::into_node` wraps the one child in `FlowNode::Stack`. `natural_height` keeps only `Split::Fit`. `Partial` and `MoveToNextPage` become `Error::UnsplittableTallerThanFrame`. The partial fragment is dropped. A cell does not resume on the next page.

What still fits:

- A paragraph wraps at `inner_w` inside `TextNode::split`. Wrapping is not a split. Several lines whose total height is ≤ the inner full frame are `Fit`. A fill blank ends at `inner_w`, then the table offsets the fragment by the cell origin.
- A list whose items all `Fit` in that same full-height frame is `Fit`. Markers and hanging indent are the body list's. The table then offsets the whole list fragment by the cell origin, so marker x is `cell_x` and item text x is `cell_x + 18`.
- A finished row that fits the full page body but not the leftover space on the current page still moves as a whole row in `split_measured`. That path is unchanged and is not mid-cell `Partial`.

What fails:

- A paragraph or list taller than the inner full frame (the case that would be `Partial` or `MoveToNextPage` in body flow) is `UnsplittableTallerThanFrame`. `document()` returns that error and records no pages.
- `marker_width` (18 pt) `>= inner_w`, or a non-positive body width, is `InvalidGeometry` from `ListNode::split`.
- `first_line_indent` that leaves `inner_w - indent <= 0` is `InvalidGeometry` from `layout_runs`. The width in that check is the cell's `inner_w`, because the node is still `Text` until split.

§2.8 item splitting applies to body lists only. A list in a cell follows §2.6: the row is the split unit.

## Test plan

All new tests in `tests/document.rs`. Assert `document(...).pages()` recording (`runs`, `glyph`, `stroke_samples`, `links`, `texts`). No PDF content-stream asserts. Reuse `FONT`, `BASELINE` (13.812), `LINE` (17.244). Default margins are 0. Do not edit `src/layout`.

`paragraph_enters_the_flow_unwrapped` stays the lock that the shared helper still emits `TextNode`, not `FlowNode::Paragraph`.

1. **`cell_paragraph_pins_bold_blank_and_link`**  
   Page `200×400`. Two equal columns (each `inner_w` 100).  
   Column 0: `Cell::paragraph` with `text("A")`, `bold("B")`, `blank_to_end`. Register `font` and `bold_font`.  
   Column 1: `Cell::paragraph` with `link("https://example.test/a", |p| p.text("Go"))`.  
   Assert font ids `(A, 0), (B, 1)` on the glyph runs. Blank stroke `x0` is at the end of `B`, `x1` is `100` (column edge), not `200` (page edge). Link target is that URI; rect `x` is `100` (cell origin after offset), height `LINE`, y `0`.

2. **`cell_paragraph_errors_record_no_pages`**  
   `link("", ...)` → `Error::EmptyLink`. `blank_to_end` then `text("x")` → `Error::RunAfterFillBlank`. Both via `result.err()`, no successful `pages()`.

3. **`cell_paragraph_indent_uses_cell_width`**  
   Page width 40, two equal columns (`inner_w` 20). `first_line_indent` 24 pt in column 0. `Error::InvalidGeometry`. An implementation that indented against the page width (40) would accept 24 pt; this fixture rejects that.

4. **`cell_list_marker_at_cell_x`**  
   Page `200×400`, two equal columns. Column 1 is `Cell::ul`: one item `"dot"`, one item that nests `ul` with `"child"`, and the row's other tests can live beside it or this cell also covers `Cell::ol(Numbering::Decimal)` in a second row with `"one"` and `item_marked("(c)", ...)`.  
   Expected x on the ul row, column origin `100`: bullet at `100`, `"dot"` at `118`, nested bullet at `118`, `"child"` at `136`. ol row: `"1."` at the cell origin, item text at origin+18, `"(c)"` as its own run. Texts include `"\u{2022}"`, `"1."`, `"(c)"`.

5. **`cell_list_item_rejects_forbidden_kids`**  
   Inside `Cell::ul` → `item`, each of these fails with `InvalidCellContent { container: "list item", .. }`: `image` (`"image"`), `table` (`"table"`), `columns` (`"columns"`), `page_break` (`"page break"`), `keep_together` (`"keep together"`). Image is the discriminating case: `Cell::image` is legal, an image in an item is not. No new `Cell::columns` just to reject it. Layout test `cell_rejects_columns_page_break_keep_together_and_tables` remains the `"table cell"` container proof.

6. **`cell_rich_taller_than_frame_does_not_continue`**  
   Page height `LINE`, width 200. `Cell::paragraph` of `"A\nB"` → `UnsplittableTallerThanFrame`, not two pages. The same string in `Flow::paragraph` already paginates (`blank_stays_on_its_line` is the body witness).  
   Second build: `Cell::ul` with two single-line items on that same page height → the same error. A body `ul` would `Partial`; the cell must not.

7. **`cell_list_narrower_than_marker_is_invalid_geometry`**  
   Page width 18, one column. `Cell::ul` with one item. `marker_width` 18 `>= inner_w` 18 → `InvalidGeometry`.

Header rows use the same `lower_rows` closure as body rows. Test 1's bold/link assertions on `t.row` cover the arm. No separate header fixture.

No `to_pdf` test. No new layout unit test: the allowlist is already true, and these tests pin the public path.

## Out of knife

- Multi-kid public cell (`stack-cell`): no paragraph + spacer + list in one `Cell`, no `Cell` flow closure, no public `Vec<FlowNode>`.
- Changing `natural_height` to keep mid-cell `Partial` (`split-in-cell`).
- Cell `columns`, `page_break`, `keep_together`, nested `table`. List items stay on the existing item matrix.
- Named styles, flow `canvas`, qr / svg / toc, encryption, tags.
- `ol` start other than 1, reverse numbering, `1.1.1` outline numbers. `item_marked` is already on `List`; this knife does not add a cell marker API.
- `marker_width` / `nest_indent` configuration. They stay 18 pt and `None`.
- Padding, border, and column-width DSL. Public tables stay equal-ratio, inset 0. Tests use that geometry; they do not invent padding.
- A second PDF backend, or `krilla` outside `backend/`.
- `Send` on the list closure. Page build stays single-threaded.

## Split 17a / 17b

**Do not split.** Not `force-both`: the design is one evening, so the default is one knife.

Counted work: two factories, one private enum, three private helpers extracted from `Flow::paragraph` / `Flow::list` / `Flow::ol`, two `Table::lower` arms, a manual `Debug`, and the seven recording tests above. No layout edits, no new error variant, no new public type.

The implementer can still land it as two commits on one branch (paragraph arm, then list arm) so the `Clone` / `PartialEq` / `Eq` drop sits on the list commit. Those commits are not knives 17a and 17b. The knife is done only when both arms are in.
