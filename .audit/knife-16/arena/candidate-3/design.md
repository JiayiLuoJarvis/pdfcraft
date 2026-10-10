# Knife 16 — `Cell::spacer`

Baseline: `feat/text-uri-link` @ `16749da`.

Authority: `docs/提案书.md` §2.6（单元格孩子允许 spacer；跨页切分单位是整行）and §2.13（非负竖直空白，可为 0，不画内容；格内允许；色块背景不做）.

Autonomy: every open call stays on the frame default. Reverse words not taken: `space`, `reject-zero`, `stack-cell`, `split-in-cell`, `page-count-only`.

This knife is a public-cell arm plus the matching `Table::lower` arm. `FlowNode::Spacer`, `SpacerNode::split`, `natural_height`, and the table-cell allowlist already exist and stay as they are.

## 1. Caller usage

Same shape as `Flow::spacer`: one `Length<NonNegative>` argument, no configure closure. Returns `Cell`, so existing `col_span` / `row_span` chain. `From` stays text-only; a length is not `Into<Cell>`.

```rust
use std::num::NonZeroU16;

use quire_pdf::{document, Cell, NonNegative, PageSize};

let gap = NonNegative::pt(24.0)?;
let two = NonZeroU16::new(2).unwrap();

document(PageSize::A4, |doc| {
    doc.font(font_bytes)?;
    doc.table(|t| {
        t.header([Cell::new("Item"), Cell::new("Gap")]);
        t.row([Cell::new("Pen"), Cell::spacer(gap)]);
        t.row([Cell::spacer(NonNegative::ZERO), Cell::new("Next")]);
        t.row([Cell::spacer(gap).col_span(two)]);
    });
    // Body spacer is unchanged and still splits across pages.
    doc.spacer(gap);
    Ok(())
})?;
```

Negative, NaN, and infinite heights fail in `NonNegative::pt` / `NonNegative::mm` (`NegativeLength` / `NonFiniteLength`) before `Cell::spacer` runs. There is no second check inside `Cell`.

## 2. Types and signatures

`CellContent` stays private. Add one arm beside `Text` / `Image` / `HLine`. `Length<NonNegative>` is already `Eq`, so the existing `Debug + Clone + PartialEq + Eq` derives stay.

```rust
enum CellContent {
    Text(String),
    Image(Image, ImageBox),
    HLine(HLine),
    Spacer(Length<NonNegative>),
}
```

```rust
impl Cell {
    /// A one-by-one cell of vertical blank. Height may be zero. Nothing is painted.
    ///
    /// Taller than the cell's inner full frame is
    /// [`Error::UnsplittableTallerThanFrame`]. The blank does not split
    /// across pages inside the cell.
    pub fn spacer(height: Length<NonNegative>) -> Self {
        Self {
            content: CellContent::Spacer(height),
            col_span: NonZeroU16::MIN,
            row_span: NonZeroU16::MIN,
        }
    }
}
```

`Table::lower` already matches `CellContent` exhaustively and wraps every arm in `vec![one node]`. Add:

```rust
CellContent::Spacer(height) => vec![FlowNode::Spacer(SpacerNode { height })],
```

`SpacerNode` is already imported in `src/document.rs` for `Flow::spacer`. No new import, no new error variant, no `From<Length<NonNegative>> for Cell`.

Header rows and body rows share `lower_rows`, so a header spacer and a body spacer are the same arm.

## 3. Module map

| File | Change |
| --- | --- |
| `src/document.rs` | `CellContent::Spacer`, `Cell::spacer`, one `Table::lower` arm |
| `tests/document.rs` | `document().pages()` geometry and the too-tall error |
| `src/layout/table.rs` | Test only: `cell_allows_spacer` next to `cell_allows_hline` |

Untouched:

- `src/layout/flow.rs` — `SpacerNode`, `SpacerNode::split`, `natural_height`, `split_stack`
- `src/layout/lower.rs` — `check_content` already allows `FlowNode::Spacer` in `Container::TableCell`
- `src/layout/table.rs` production — `measure`, `row_offsets`, `split_measured`
- `src/layout/paginate.rs` — body-spacer split (`spacer_remainder_leads_next_page` must keep passing)
- `src/lib.rs` — `Cell` is already re-exported; no new public type
- `src/geom.rs`, `src/error.rs`, `src/backend/**`
- `Flow::spacer` signature and body-flow split semantics

No `use krilla` outside `backend/`.

## 4. Lower and measure

### Lower

`Flow::table` builds a public `Table`, then `Table::lower`. A spacer cell stores only the branded height. It does not need the parent style (unlike `CellContent::Text` → `flow.text`) and does not need the document image token (unlike `CellContent::Image`).

`table::Cell::new` → `CellBody::new` → `check_content(TableCell)`. Spacer is already allowed, unconditionally, in the same arm as text, paragraph, and horizontal line. Columns, page break, keep-together, and nested table stay rejected. This knife does not edit that matrix.

`CellBody::into_node` still wraps the one-node vector as `FlowNode::Stack([Spacer])`.

### What `natural_height` does with that stack

Table measure (`src/layout/table.rs`) builds, per cell:

```text
inset      = border_width / 2 + padding
inner_full = page_body_full_height - 2 * inset
frame      = { width: inner_width, height: inner_full, full_height: inner_full }
(h, frag)  = natural_height(Stack(cell_children), frame)
cell_h     = h + 2 * inset
```

Public tables still lower with `TableSpec::new` defaults: padding 0, border width 0, so `inset = 0` and `inner_full` equals the page body height. Default margins are 0, so on a custom page that body height is the page height. A future public padding or border shrinks `inner_full`; this knife does not add that API.

`natural_height` keeps only `Split::Fit`. `Split::Partial` and `Split::MoveToNextPage` become `Error::UnsplittableTallerThanFrame`.

`SpacerNode::split` against that full frame:

| Stored height | `SpacerNode::split` | After `Stack` + `natural_height` |
| --- | --- | --- |
| `0` | `Fit`, height 0, empty fragment | Cell contributes 0. Legal. |
| `0 < h <= inner_full` | `Fit`, height `h`, empty commands | Cell height `h` (inset 0). No glyphs, strokes, or fills. |
| `h > inner_full` | `Partial`: head of `inner_full`, tail a shorter `SpacerNode` | `split_stack` returns that `Partial`. `natural_height` rejects it. The document fails before any page. The tail is discarded, not placed on the next page. |

Row height is the max cell height in the physical row, then `row_offsets` assigns any row-span shortfall to the last row of the span. Both rules already exist. A spacer shorter than a text sibling in the same row does not grow the row: one text line is the fixture line box `LINE` (17.244 pt at 12 pt on `tests/fixtures/quire-measure.ttf`), and `max(8, LINE)` stays `LINE`. A spacer-only row grows by exactly the spacer height, including a height below `LINE`.

Content is top-aligned in the cell (`y = row_y + inset`). A same-row text sibling does not move down by the spacer height. The next row's glyph baseline does: `row_height + BASELINE` (`BASELINE` is 13.812 pt on that fixture).

### Pagination after a successful measure

`split_measured` paginates prebuilt row-group fragments. It never calls `SpacerNode::split` again. A spacer row that fit `inner_full` still moves as a whole row when it does not fit the space left on the current page. A spacer taller than `inner_full` never reaches that path.

Body flow is the other rule, and it stays the other rule. An 80 pt `Flow::spacer` on a 50 pt page is `Partial`: empty first page, remainder 30 pt, following text at `30 + baseline` on page 2 (`spacer_remainder_leads_next_page`). The same 80 pt height inside a cell on a 40 pt page is `UnsplittableTallerThanFrame`.

A header spacer uses the same measure. If the cell fits but header height plus the next body row exceeds `full_height`, the existing table guard still returns `UnsplittableTallerThanFrame`. That guard is not retuned here.

## 5. Test plan

Public tests go in `tests/document.rs`, next to `hline_in_list_item_and_cell`. They use `document(...).pages()`, `pt_near`, and the existing `BASELINE` / `LINE` / `FONT` fixtures. Layout assertion is one `unwrap` in `src/layout/table.rs` tests. No PDF content-stream assertions. No new paint opcode for the spacer.

### `cell_spacer_shifts_the_next_row_glyph`

Page `Size::try_pt(200.0, 200.0)`, default margins, fixture font.

```rust
doc.table(|t| {
    t.row([Cell::spacer(NonNegative::pt(24.0).unwrap())]);
    t.row([Cell::new("Below")]);
});
```

One page. The only glyph run is `"Below"`. `pt_near(y, 24.0 + BASELINE)`. `x` is 0. Page ops contain a glyph run and nothing else (no `FillPath`, no `StrokePath`). 24 pt is enough on its own because the spacer row has no text sibling; the shift equals the argument, not `max(argument, LINE)`.

### `short_cell_spacer_does_not_grow_a_text_row`

Same page. Two equal columns (the public table's only width mode).

```rust
doc.table(|t| {
    t.row([
        Cell::spacer(NonNegative::pt(8.0).unwrap()),
        Cell::new("Beside"),
    ]);
    t.row([Cell::new("Below"), Cell::new("Tail")]);
});
```

`Beside.y` and `Tail.y` wait: `Beside.y == BASELINE` (top of row 0, not pushed down by 8 pt). `Below.y` and `Tail.y` are `LINE + BASELINE`, not `8 + BASELINE`. `Beside.x` and `Tail.x` are 100 (half of 200). This locks row height as a max, not a sum.

### `cell_spacer_zero_does_not_move_the_next_glyph`

```rust
doc.table(|t| {
    t.row([Cell::spacer(NonNegative::ZERO)]);
    t.row([Cell::new("Below")]);
});
```

`Below.y == BASELINE`. Zero is a successful `Fit` of height 0, not an error.

### `cell_spacer_alone_records_no_ops`

```rust
doc.table(|t| {
    t.row([Cell::spacer(NonNegative::pt(24.0).unwrap())]);
});
```

One page. `pages()[0].ops()` is empty. No font required.

### `cell_spacer_taller_than_body_fails`

Page `Size::try_pt(200.0, 40.0)`, spacer `NonNegative::pt(80.0)`.

```rust
assert_eq!(result.err(), Some(Error::UnsplittableTallerThanFrame));
```

`document` returns before any page. This is the cell counterpart of `hline_taller_than_body_fails_before_any_page`, and it must not follow `spacer_remainder_leads_next_page` (two pages, glyph at `30 + baseline`).

### `cell_spacer_row_moves_whole`

Page height 40. Spacer row 30 pt, then a text row (`LINE` is about 17.244, so 30 + LINE > 40, and 30 <= 40).

Two pages. Page 0 has no glyph ops (the spacer row filled page 1 and painted nothing). Page 1's `"Below"` is at `y == BASELINE`, not at a leftover such as `10 + BASELINE`. That is the atomic-row observation: the spacer did not split inside the cell, and the next row started at the top of the next page.

### `cell_allows_spacer` (`src/layout/table.rs`)

Mirror `cell_allows_hline`:

```rust
Cell::new(vec![FlowNode::Spacer(SpacerNode {
    height: NonNegative::pt(12.0).unwrap(),
})])
.unwrap();
```

Do not edit `cell_rejects_columns_page_break_keep_together_and_tables`. It must still pass, along with `spacer_remainder_leads_next_page` and `hline_in_list_item_and_cell`.

No new test for `NonNegative::pt(-1.0)`. The brand already rejects it.

## 6. Out of knife

- Rich paragraph, blank, or link inside a cell.
- List inside a cell.
- Multi-child public cell, `Flow` closure on `Cell`, or a stack-cell builder (`stack-cell`).
- A second spacer type, a public `SpacerNode`, or `From<Length<NonNegative>> for Cell`.
- Changing `natural_height` or `SpacerNode::split` so a cell spacer continues on the next page (`split-in-cell`).
- Changing body `Flow::spacer` split semantics.
- Columns, page break, keep-together, or nested table in a cell.
- Canvas, QR, SVG, TOC, named styles, encryption, tags.
- Spacer background / color block (§2.13).
- Public `cell_padding` or border changes. They would shrink `inner_full` later; the fit rule above already accounts for inset without implementing them.
- `page-count-only` as the proof. Page count does not show a blank's height.
- Renaming the method to `space`.
- Rejecting zero (`reject-zero`).
- krilla types outside `backend/`.
