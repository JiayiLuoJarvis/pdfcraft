# Explorer: public cell content (`Cell` / `CellContent` / `Table::lower`)

Angle: how a caller builds a table cell today, how `Table::lower` turns that into `layout::table::Cell` / `FlowNode`s, and what is absent so a caller can put a spacer in a cell. Proposal §2.6 and §2.13 already allow a spacer in a cell. Layout `check_content` already allows `FlowNode::Spacer` in `Container::TableCell`. The public DSL cannot construct one.

Two different `Cell` types:

| Type | Visibility | File |
| --- | --- | --- |
| `quire::document::Cell` (re-exported as `quire::Cell`) | public | `src/document.rs` 1030–1093, `src/lib.rs` 21–24 |
| `layout::table::Cell` | `pub(crate)` | `src/layout/table.rs` 50–73 |

`src/document.rs` imports the layout module as `table` (`src/document.rs` 29), so `Table::lower` writes `table::Cell::new`. The test at `src/document.rs` 1885–1887 is `std::cell::Cell::new(false)` inside one function; it is not the table cell.

## Components Found

### Public builders (inert until lower)

`Table` (`src/document.rs` 962–967) holds `header: Vec<Vec<Cell>>`, `body: Vec<Vec<Cell>>`, `repeat_header: bool` (default `true`, set in `Flow::table` at 707–711 and overridable by `Table::repeat_header` at 989–992).

`Table::header` (971–978) and `Table::row` (981–987) share one signature:

```rust
pub fn row<I>(&mut self, cells: I)
where
    I: IntoIterator,
    I::Item: Into<Cell>,
```

Each item is `Into::into` then pushed as one row. No content check here. No `FnOnce(&mut Flow)`.

`Cell` (`1030–1037`) is `Debug + Clone + PartialEq + Eq + must_use`. Fields are private:

- `content: CellContent`
- `col_span: NonZeroU16` (default `NonZeroU16::MIN`, i.e. 1)
- `row_span: NonZeroU16` (default 1)

`CellContent` (`1039–1044`) is a private enum (not `pub`). Three arms, all owned data, no closures, no `FlowNode`:

| Arm | Stored | Built by |
| --- | --- | --- |
| `Text(String)` | owned string | `Cell::new` |
| `Image(Image, ImageBox)` | copy of the handle plus the configured box | `Cell::image` |
| `HLine(HLine)` | configured line | `Cell::hline` |

No `Spacer` arm. `SpacerNode` is `pub(crate)` (`src/layout/flow.rs` 127–130) and is not re-exported from `src/lib.rs`.

Constructors, all return a 1×1 `Cell` (`col_span` / `row_span` = `NonZeroU16::MIN`):

- `Cell::new(text: impl Into<String>)` (`1048–1054`). Stores `CellContent::Text(text.into())`. Does not touch style, fonts, or `Flow`.
- `Cell::image(image: &Image, f: impl FnOnce(&mut ImageBox))` (`1057–1065`). `ImageBox::default()`, run `f`, store `CellContent::Image(*image, image_box)`. `Image` is `Copy` (`419–425`), so the handle is copied. The closure runs immediately, not at paint time.
- `Cell::hline(f: impl FnOnce(&mut HLine))` (`1068–1076`). `HLine::default()`, run `f`, store `CellContent::HLine(line)`. Doc comment: “a horizontal line as wide as the cell content.” Closure runs immediately.

Span setters consume `self` and keep `content` via struct update:

- `Cell::col_span(self, n: NonZeroU16)` (`1079–1084`)
- `Cell::row_span(self, n: NonZeroU16)` (`1087–1092`). Doc: a header cell cannot span into the body. That check is not here; it is `TableNode::new` (`src/layout/table.rs` 136–146, `row_limit`).

`Into<Cell>` exists only for text (`1095–1111`):

- `From<&str>` → `Cell::new(text)`
- `From<String>` → `Cell::new(text)`
- `From<&String>` → `Cell::new(text.as_str())`

`Cell` itself is `Into<Cell>` via the blanket impl, so a row can mix `Cell::new` / `Cell::image` / `Cell::hline` only when every element is already a `Cell` (see `tests/document.rs` 632–635). A `[&str; N]` row works because of `From<&str>` (`tests/document.rs` 91, 404). There is no `From<Length<NonNegative>>`. Image and hline also have no `From`; callers name `Cell::image` / `Cell::hline`.

### What those stored values lower into

`HLine` (`src/document.rs` 494–569) is the public config. `HLine::lower(self) -> FlowNode` (`559–568`) is private and infallible: `FlowNode::HLine(HLineNode::new(...))`. `HLineNode` is `pub(crate)` (`src/layout/hline.rs` 9–37).

`ImageBox::lower(self, image: &Image, doc: DocToken) -> Result<FlowNode>` (`484–491`) checks `image.doc != doc` → `Error::UnknownImage`, else `ImageNode::new(...).map(FlowNode::Image)`. `ImageNode::new` returns `Result` (`src/layout/image.rs` 19–25`).

`Flow::text(&self, text: String) -> FlowNode` (`665–667`) is private: `FlowNode::Text(TextNode::new(text, self.style.clone()))`. `TextNode::new` (`src/layout/paragraph.rs` 109–117) stores one `InlineItem::Text` run with `Face::Body`, or an empty item vec when the string is empty. This is not `FlowNode::Paragraph`. There is no public `Flow::text`. Body prose is `Flow::paragraph` (`678–690`), which builds `FlowNode::Text(TextNode::from_items(...))` — a different constructor, unused by cells.

`Flow::spacer(&mut self, height: Length<NonNegative>)` (`726–728`) pushes `Ok(FlowNode::Spacer(SpacerNode { height }))`. One branded length, no configurator closure, no `lower` method. Negative heights fail in `NonNegative::pt` (`src/geom.rs` 107–112) before the push (`tests/document.rs` 417–423).

### Lower bridge

`Flow::table(&mut self, f: impl FnOnce(&mut Table))` (`706–715`):

1. `Table { header: empty, body: empty, repeat_header: true }`.
2. Run `f` (callers push `Cell`s).
3. `table.lower(self)` where `self` is the parent `&mut Flow`, coerced to `&Flow`.
4. `self.push(node)` (`638–650`). `Ok` appends the `FlowNode::Table`. `Err` stores the first error and later `push`es are dropped.

`Table::lower(self, flow: &Flow) -> Result<FlowNode>` (`994–1027`):

- Column count (`995–1000`): span sum of the first header row if any header row exists, otherwise the first body row. Empty table → `cols = 0`.
- Every column is `ColumnWidth::Ratio(Ratio::new(1.0)?)` (`1001`, and `vec![even; cols]` at 1024). No public fixed/auto widths on this `Table`.
- `lower_rows` maps every header row and every body row through the same closure (`1002–1018`).
- Per cell, exhaustive `match cell.content`:
  - `Text(text)` → `vec![flow.text(text)]` — style cloned from the parent `Flow` **at lower time**, not at `Cell::new` time. Infallible.
  - `Image(image, image_box)` → `vec![image_box.lower(&image, flow.doc)?]` — only arm that uses `?` inside the match. Needs `flow.doc` (`DocToken`, `src/document.rs` 405–417`).
  - `HLine(line)` → `vec![line.lower()]` — does not use `flow`. Infallible.
- Then `table::Cell::new(content)?.with_span(cell.col_span, cell.row_span)`.
- `TableSpec { header, body, repeat_header, ..TableSpec::new(vec![even; cols]) }` (`1020–1025`). `TableSpec::new` (`src/layout/table.rs` 35–47`) sets `padding: NonNegative::ZERO`, `border.width: NonNegative::ZERO`, `repeat_header: true` (overwritten), empty header/body (overwritten). Public `Table` has no padding or border setter, so those stay zero.
- `Ok(FlowNode::Table(TableNode::new(spec)?))`.

The match is exhaustive on the three `CellContent` arms. A fourth arm does not compile until this match grows an arm.

### Layout cell (already spacer-capable)

`table::Cell::new(content: Vec<FlowNode>) -> Result<Self>` (`src/layout/table.rs` 57–64) wraps `CellBody::new(content)?` and sets spans to 1. `with_span` (`66–72`) replaces spans.

`CellBody` (`src/layout/lower.rs` 85–98`):

- `new` calls `check_content(Container::TableCell, &nodes)`.
- `into_node(self) -> FlowNode::Stack(self.0)` — always a stack, including a one-element vec.

`check_content` (`src/layout/lower.rs` 30–68) allows, for every container including `TableCell` and `Band`:

```rust
FlowNode::Text(_) | FlowNode::Paragraph(_) | FlowNode::Spacer(_) | FlowNode::HLine(_) => true
```

(`33–36`). `node_name` of spacer is `"spacer"` (`73`).

Rejected in a table cell (`47–58`, test `cell_rejects_columns_page_break_keep_together_and_tables` at `src/layout/table.rs` 626–657):

| Node | Allowed in `TableCell`? | Error `node` string |
| --- | --- | --- |
| `Columns` | no (`53`) | `"columns"` |
| `PageBreak` | no (`58`) | `"page break"` |
| `KeepTogether` | only when container is `Column` (`41–45`) | `"keep together"` |
| `Table` | only `Column` (`55`) | `"table"` |
| `Stack` | recurse; a stack whose child is illegal fails with the **child** name (`37–39`, test uses `Stack([PageBreak])` → `"page break"`) | child’s name |
| `List` | yes unless `Band` (`56`) | — |
| `Image` | `Column` or `TableCell` only (`57`) | `"image"` elsewhere |

`cell_allows_hline` (`src/layout/table.rs` 659–670) asserts `Cell::new(vec![FlowNode::HLine(...)])` is `Ok`. There is no `cell_allows_spacer` test. `src/layout/table.rs` does not mention `Spacer` at all. The allow is only the shared pattern in `lower.rs`.

Measure (`src/layout/table.rs` 210–222): each placed cell becomes `natural_height(body.into_node(), ctx, frame)`. Frame width is the spanned columns minus `2 * inset`. Frame `height` and `full_height` are both `avail.full_height - 2 * inset`, with `inset = border/2 + padding` (`201–206`). Public tables: inset is 0. `natural_height` (`src/layout/flow.rs` 341–350) returns `(height, fragment)` only for `Split::Fit`. `Partial` and `MoveToNextPage` become `Error::UnsplittableTallerThanFrame`. Cell content is baked into a `Fragment` once. Later `split_measured` (`340+`) splits prebuilt row-group fragments; it does not re-split the cell’s `FlowNode`s.

`SpacerNode::split` (`src/layout/flow.rs` 179–208`): height 0 → `Fit` height 0, empty commands. Otherwise take `min(h, avail.height)` and, if remainder > 0, `Partial` with a shorter `SpacerNode` tail and **empty** `commands`. In a cell, that `Partial` is rejected by `natural_height`. A body-flow spacer of the same height does split (`src/layout/paginate.rs` 526–539, `tests/document.rs` 1141).

### Contrast: list items and columns already call `Flow::spacer`

`List::item` (`src/document.rs` 1136–1138) and `Columns::col` (`943–946`) take `FnOnce(&mut Flow)`, run `Flow::nested` (`652–656`), and pass the resulting `Vec<FlowNode>` to `ListItem::new` / a column stack. `Flow::spacer` is therefore callable as `item.spacer(...)` or `c.spacer(...)`. `ItemBody::new` (`src/layout/lower.rs` 104–107) runs `check_content(Container::ListItem)`, which allows `Spacer` on the same arm as cells. `ListItem::new` (`src/layout/list.rs` 106–110) stores that vec; `into_node` is also `FlowNode::Stack` (`110–112`).

A cell never receives a `Flow`. `Cell` methods do not call `Flow::spacer`.

### Proposal constraints already written (not new API)

`docs/提案书.md`:

- §2.6 cell children (`377–382`): allow 段落（含填空）、分隔线、spacer；图片/列表随对应 PR. Forbid `columns`, pageBreak, keep_together, nested table.
- §2.6 DSL (`384`): signatures not pinned. `t.header` / `t.row` / `col_span` / `row_span` / `repeat_header(true)` default true. No `Cell::spacer` signature.
- §2.13 (`545–554`): vertical blank, non-negative height (0 allowed), independent of paragraph spacing, **splittable** across pages, **allowed in a cell**, no color block.

Image and hline “随对应 PR” are already public (`Cell::image`, `Cell::hline`). Spacer in a cell is the remaining allow-list item with no public constructor. Paragraph-with-blanks is also not a cell constructor (`Cell::new` is a plain string → `TextNode`, not `Paragraph` items / `Blank`). Out of this slice except as a sibling gap.

## Flow

Caller builds a cell (example: `doc.table(|t| t.row([Cell::hline(|h| ...)]))`, `tests/document.rs` 1207–1209):

1. `document()` (`src/document.rs` 42–48) runs the closure once. `Document` derefs to `Flow` (`571–588`).
2. `Flow::table` (`706–715`) builds an empty `Table` and runs the closure.
3. `Table::row` / `header` (`971–987`) collect `Into<Cell>`.
4. Factory stores one `CellContent` arm. `Cell::image` / `Cell::hline` run their configurator now. Spans stay 1 until `col_span` / `row_span`.
5. Closure returns. `Table::lower(&Flow)` (`994–1027`) maps each arm to `Vec<FlowNode>` of length 1.
6. `table::Cell::new` (`src/layout/table.rs` 58–63) → `CellBody::new` → `check_content(TableCell)`. Text, image, and hline all pass. Spacer would pass too, but step 5 cannot emit it.
7. `with_span` copies the public spans.
8. `TableNode::new` (`125–180`) checks column count > 0, span overlap, grid fill, header rowspan not entering the body. Failure is `Error::InvalidCellSpan` or `ColumnWidths { reason: "column-count" }`.
9. `Flow::push` stores `FlowNode::Table` or the first `Error`.
10. Later pagination: `TableNode::split` → `measure` (`191–222`) calls `natural_height` on `CellBody::into_node()` (`Stack` of that one node). Commands are offset into the row-group fragment (`246–259`). A spacer’s fragment has no commands, so it would only add height (`row_h[row] = max(...)`, `224–226`).

Body spacer path, for comparison (`Flow::spacer` 726–728): push `FlowNode::Spacer` onto the root stack. No `CellContent`, no `check_content`. `SpacerNode::split` may return `Partial`.

List-item spacer path, for comparison: `item.spacer(h)` is `Flow::spacer` inside `Flow::nested`, then `ListItem::new` / `ItemBody` / `check_content(ListItem)`. Several nodes per item are normal. One node per cell is the cell rule.

## Files Read

- `src/document.rs` — imports `22–29`; `DocToken` / `Image` `405–425`; `ImageBox` + `lower` `441–491`; `HLine` + `lower` `494–569`; `Flow` push/nested/text/table/spacer/hline `603–775`; `Table` / `Cell` / `CellContent` / `From` `935–1111`; `List::item` `1129–1160`. `std::cell::Cell` only at `1885`.
- `src/lib.rs` `21–24` — public re-export of `Cell`, `HLine`, `ImageBox`, `Table`, `Flow`. No spacer type.
- `src/layout/lower.rs` — full file. `Container`, `check_content`, `node_name`, `CellBody`, `ItemBody`.
- `src/layout/table.rs` `1–189` (`Cell`, `TableSpec::new`, `TableNode::new`); `191–295` (`measure`, `natural_height`); `297–391` (row groups, `split_measured` start); tests `414–416`, `626–670`.
- `src/layout/flow.rs` `127–209` (`SpacerNode`, `FlowNode`, `split`); `341–350` (`natural_height`).
- `src/layout/paragraph.rs` `101–130` (`TextNode::new` / `split`).
- `src/layout/hline.rs` `1–37` (`HLineNode::new`).
- `src/layout/image.rs` `1–25` (`ImageNode::new` → `Result`).
- `src/layout/list.rs` `99–123` (`ListItem::new`), `159–161` (`into_node`).
- `src/layout/columns.rs` `102–123` (`ColumnsNode::new` calls `check_content(Column)`).
- `src/error.rs` `89–95`, `168–169` (`InvalidCellContent` display: `"{node} is not allowed in a {container}"`).
- `src/geom.rs` `14–38`, `101–112` (`Length<NonNegative>: Eq`, `NonNegative::pt`).
- `docs/提案书.md` `338–384` (§2.6), `545–554` (§2.13).
- `tests/document.rs` — cell call sites `91–93`, `306–310`, `403–405`, `630–635`, `1207–1209`; body `spacer` `332`, `420`, `1141`. No `Cell::spacer`.

## Boundaries

- **Public vs layout cell.** Callers only see `document::Cell`. They cannot pass a `Vec<FlowNode>` or a `SpacerNode`. `table::Cell::new` is `pub(crate)` and is reached only from `Table::lower` on the public path (tests in `table.rs` call it directly).
- **One arm, one node.** A public cell is text **or** image **or** hline. `lower` always builds a one-element `Vec`. `CellBody` can hold many nodes; the DSL never builds more than one. A spacer beside a paragraph in the same cell is impossible without a different `Cell` shape (list items already have that shape).
- **`check_content` is not the gap.** `FlowNode::Spacer` is already legal in `TableCell`, `ListItem`, `Column`, and `Band` (`lower.rs` 33–36). Adding a public cell spacer must not add a new allow arm and must not special-case `Band`. `Band`’s public methods still cannot emit a spacer; that is a separate surface (`Band::left` / `right` only, `src/document.rs` 1222–1243).
- **Style and document token.** Text cells pick up `Flow.style` at lower. Image cells check `DocToken` at lower. HLine and a spacer height need neither. `Table::lower`’s `flow: &Flow` parameter stays for the text and image arms.
- **Errors a public cell can produce today.** `Cell::new` / `hline` / span setters do not return `Result`. `Cell::image`’s closure cannot report `Result` (same as `Flow::image`). Failures show up from `Table::lower` / `TableNode::new`: `UnknownImage`, `InvalidCellSpan`, `ColumnWidths { reason: "column-count" }` (empty table or `Ratio::new`). `InvalidCellContent` is not reachable from the three public arms, because all three map to nodes `TableCell` allows. `CellBody::new` still returns that error for illegal nodes; only in-crate callers hit it (`table.rs` 626–657).
- **Pagination.** Table split unit is the row group (`src/layout/table.rs` 317–338, `split_measured`). Cell innards are not split. A spacer taller than the cell’s inner full frame fails `natural_height` with `UnsplittableTallerThanFrame` instead of continuing on the next page. Flow spacers do the opposite (`SpacerNode::split`). Public padding and border are 0, so the inner frame height equals the table’s `avail.full_height`.
- **Paint.** `SpacerNode` fragments have `commands: Vec::new()` (`flow.rs` 190–194). Height still participates in `row_h` (`table.rs` 222–226). No stroke, no glyphs.
- **Root flow is not `check_content`.** `Flow::spacer` on the document body does not go through `CellBody`. Nested containers do: columns (`columns.rs` 115), cells (`lower.rs` 91), list items (`lower.rs` 106).

## Non-Obvious Things

- **Mirror for knife 16 is `Cell::hline`’s storage plus `Flow::spacer`’s argument, not the list-item `Flow` closure.**
  - Add a private `CellContent` arm that stores data, not a `FlowNode` and not a closure. Hline stores `HLine`; image stores `(Image, ImageBox)`; text stores `String`. The spacer equivalent is `Length<NonNegative>` (there is no public spacer config type and no `lower` on one). `Length<C>: Eq` (`geom.rs` 26–38), so `Cell: Eq` stays valid. `HLine` and `Image` are `Copy`; `Length` is `Copy`; `String` is not, which is why `Cell` is `Clone` but not `Copy`.
  - Constructor shape of `Flow::spacer` (`726–728`): `pub fn spacer(&mut self, height: Length<NonNegative>)`, immediate `SpacerNode { height }`, no `FnOnce`. `Cell::hline` / `Cell::image` use a closure only because `HLine` / `ImageBox` have setters. A spacer has a single already-branded height.
  - Place that constructor on `Cell` like `Cell::hline` (`1068–1076`): associated function returning `Self`, spans `NonZeroU16::MIN`. Suggested call shape already used by tests: `t.row([Cell::hline(...)])` → `t.row([Cell::spacer(height)])`. Do not add `From<Length<_>>` unless image/hline gain `From` too; they have not.
  - `Table::lower` arm should match hline’s infallible `vec![...]` (`1012`), with the expression `Flow::spacer` already uses: `vec![FlowNode::Spacer(SpacerNode { height })]`. No new `lower` helper is required. The image arm is the one that uses `?` and `flow.doc`; spacer should not.
  - `col_span` / `row_span` already keep `content`. No change if the new arm lives in `content`.
  - Do not change `check_content`. `table::Cell::new` will accept the node. `node_name` already returns `"spacer"`.
- **Do not mirror `List::item` / `Columns::col` if the goal is only “a spacer in a cell”.** Those take `FnOnce(&mut Flow)` and a `Vec<FlowNode>`. That would also admit every other `Flow` method into the cell (`paragraph`, `ul`, `image`, `page_break`, …), with rejection deferred to `check_content`. Today’s cell factories never do that. `Cell::hline` was added as a whole-cell factory, not a nested flow (call site `tests/document.rs` 1208 vs list `1200–1204`, where the list uses `item.hline` on `Flow`).
- **Lower wraps one node in a `Stack`.** `vec![Spacer]` becomes `FlowNode::Stack([Spacer])` via `CellBody::into_node` (`lower.rs` 95–97`) before `natural_height`. Same for text, image, and hline. Height is the stack’s fit height.
- **A cell spacer does not inherit flow splitting.** `natural_height` (`flow.rs` 347–349) turns the spacer’s `Partial` into `UnsplittableTallerThanFrame`. §2.13 “可拆” is implemented for a **body** spacer. §2.6 says the table cuts on **rows**, not through a row. Both are already implemented. A public cell spacer would hit the table rule, not `SpacerNode`’s page split. A spacer that fits only increases that row’s height (empty commands). A spacer taller than the inner frame fails the whole table at measure time, before pages (same error class as `hline_taller_than_body_fails_before_any_page`, `tests/document.rs` 1176–1189, but that test is a body hline, not a cell).
- **Zero height is already legal** for `NonNegative` (`geom.rs` 103–104`) and for `SpacerNode::split` (`flow.rs` 182–186`, `Fit` height 0). With public padding/border 0, a row of only zero spacers would measure row height 0. No public test.
- **Text cells are not paragraphs.** `Cell::new("Total")` (`tests/document.rs` 310) becomes `FlowNode::Text`, one `Face::Body` run, parent style including 12 pt / line height 1.2 / black (`default_style`, `document.rs` 591–600`). No `blank`, no `bold`, no link. Filling a cell with a paragraph is a separate missing factory, not required to store a spacer.
- **First row defines columns, header wins.** `header.first().or(body.first())` (`995–1000`). A header row exists even if a later body row has a different span sum; mismatch becomes `InvalidCellSpan` inside `TableNode::new`, not at `row()`. `tests/document.rs` 403–414: one-column second row after a two-column row surfaces `ColumnWidths { reason: "column-count" }` — that message is from `ColumnsNode` when a **previous** `columns` call had one column (`401–402`); the table error is swallowed because `Flow::push` keeps the first error (`638–640`). A lone bad table still fails in `TableNode::new`.
- **`repeat_header` default true** matches §2.6 (`document.rs` 710, `989–992`). Unrelated to cell content, but `lower` copies it onto `TableSpec` (`1023`).
- **Image in a cell is the other non-text precedent** (`tests/document.rs` 630–635): `Cell::image(&seal, |image| image.width(side))` next to `Cell::new("A")`. Closure configures; lower resolves the node. Spacer has nothing analogous to `width` / `center` / `color`.
- **Hline in a cell is tested on the public path** (`tests/document.rs` 1193–1231) and on the layout allow path (`table.rs` 659–670). Spacer is tested only as a body node (`tests/document.rs` 328–332, 1141; `paginate.rs` 526). No test that `table::Cell::new(vec![FlowNode::Spacer(...)])` succeeds, and no test that a public row contains a spacer.

## Open Questions

- §2.6 `384` says cell DSL signatures are not pinned. The code pattern for “one whole-cell block” is `Cell::hline` / `Cell::image`. The code pattern for “several blocks, including spacer” is `List::item`’s `Flow` closure. Knife 16’s sentence is “put a spacer in a cell”, which matches the whole-cell factory. Combining spacer **and** text in one cell is still missing either way, and §2.6’s child list does not say whether one cell may hold both. Layout `CellBody(Vec<FlowNode>)` already could.
- §2.13 says the spacer **node** splits across pages, and also says it is allowed in a cell. Table measure refuses `Partial`. Should a cell spacer taller than the body fail (what `natural_height` does today), or should that interaction be redesigned? Not specified in code comments. Public cells cannot hit it yet.
- No in-tree call of `table::Cell::new` with `FlowNode::Spacer`. The `check_content` arm says it would succeed. That is unread by a test.
- Public tables cannot set `cell_padding` or border (§2.6 `360–366` says they exist). `Table::lower` leaves `TableSpec` padding and border at zero (`document.rs` 1020–1025, `table.rs` 41–46`). Inset is 0, so a cell spacer’s frame height is the table frame’s `full_height`. If padding lands later, `natural_height`’s frame shrinks by `2 * inset` and the same spacer can start failing. Out of scope for the missing constructor, but it is the frame a cell spacer would be measured against.
