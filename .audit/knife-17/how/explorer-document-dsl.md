# Explorer: public cell paragraph and list (`Cell` / `Paragraph` / `List` / `Table::lower`)

Angle: after knife 16, how a caller can put a rich paragraph (text / bold / blank / link) or a list into a public table cell. Proposal §2.6 allows both as cell children. `check_content(Container::TableCell)` already allows `FlowNode::Text`, `FlowNode::Paragraph`, and `FlowNode::List`. The public cell still stores one of four data arms and lowers each to a one-element `Vec<FlowNode>`.

Two different `Cell` types stay in place:

| Type | Visibility | File |
| --- | --- | --- |
| `quire::document::Cell` (re-exported as `quire::Cell`) | public | `src/document.rs` 1033–1110, `src/lib.rs` 21–24 |
| `layout::table::Cell` | `pub(crate)` | `src/layout/table.rs` 50–73 |

`src/document.rs` imports the layout module as `table` (`src/document.rs` 29). `Table::lower` writes `table::Cell::new`. The `std::cell::Cell` at `src/document.rs` 1902 is unrelated.

## Components Found

### Public cell is still one payload

`Table` (`src/document.rs` 962–967) holds `header: Vec<Vec<Cell>>`, `body: Vec<Vec<Cell>>`, `repeat_header: bool`. `Flow::table` (`706–715`) builds that struct with empty rows and `repeat_header: true`, runs the caller closure, then `table.lower(self)`.

`Table::header` (`971–978`) and `Table::row` (`981–987`) collect `I::Item: Into<Cell>`. They do not take `FnOnce(&mut Flow)`.

`Cell` (`1033–1040`) is `Debug + Clone + PartialEq + Eq + must_use`. Private fields: `content: CellContent`, `col_span`, `row_span` (both default `NonZeroU16::MIN`).

`CellContent` (`1042–1048`) is a private enum. Four arms, all owned data:

| Arm | Stored | Built by | `Table::lower` (`1007–1016`) |
| --- | --- | --- | --- |
| `Text(String)` | owned string | `Cell::new` (`1051–1057`), `From<&str/String/&String>` (`1112–1128`) | `vec![flow.text(text)]` |
| `Image(Image, ImageBox)` | handle plus box | `Cell::image` (`1060–1068`) | `vec![image_box.lower(&image, flow.doc)?]` |
| `HLine(HLine)` | configured line | `Cell::hline` (`1071–1079`) | `vec![line.lower()]` |
| `Spacer(Length<NonNegative>)` | branded height | `Cell::spacer` (`1087–1093`) | `vec![FlowNode::Spacer(SpacerNode { height })]` |

Every arm is `vec![one node]`, then `table::Cell::new(content)?.with_span(...)` (`1017`). `col_span` / `row_span` (`1096–1109`) keep `content`.

`Cell::hline` and `Cell::image` run the configurator immediately and store the value. `Cell::spacer` stores the length with no closure. None of them receive a `Flow`.

### `Paragraph` is already the rich-text builder, and it does not need a `Flow`

`Paragraph` (`src/document.rs` 778–784`) is public (`src/lib.rs` 21–24`) and `Default` (`786–795`). Fields are private: `items: Vec<InlineItem>`, `first_line_indent`, `active_link`, `error`. It has no `Clone` / `Debug` / `PartialEq` / `Eq` derive.

Public methods, all on this one type:

| Method | Lines | What it stores |
| --- | --- | --- |
| `text` | `799–801` | `Face::Body` via `append` |
| `bold` | `804–806` | `Face::Bold` via `append` |
| `blank` / `blank_em` / `blank_to_end` | `809–823` | `InlineItem::Blank`; returns `Blank<'_>` (`897–933`) for thickness, color, baseline offset |
| `first_line_indent` | `826–828` | paragraph field, separate from items |
| `link` | `834–845` | `LinkTarget` on the runs appended inside `f`; empty URI sets `Error::EmptyLink` and skips `f` |

`append` (`847–864`) drops empty strings, coalesces adjacent runs with the same face and link, and records `Error::RunAfterFillBlank` after a fill-to-end blank. `note` (`889–893`) keeps the first error.

`Flow::paragraph` (`678–690`) is the only producer of a public paragraph node:

1. `Paragraph::default()`, run `f`.
2. If `paragraph.error` is set, `push(Err(error))` and return. No node.
3. Clone `self.style`, overwrite `style.first_line_indent`.
4. `push(Ok(FlowNode::Text(TextNode::from_items(paragraph.items, style))))`.

That node is `FlowNode::Text`, not `FlowNode::Paragraph`. The unit test `paragraph_enters_the_flow_unwrapped` (`1565–1575`) and `paragraph_coalesces_adjacent_faces_and_ignores_empty_appends` (`1578–1603`) assert `TextNode` / `TextNode::from_runs`. Bold, blank, and link tests in the same module (`1616+`, `1724+`, `1764+`, `1801+`) read that same `TextNode`.

`TextNode` (`src/layout/paragraph.rs` 101–130`) is `pub(crate)`. `from_items` stores the inline items unwrapped. `split` (`127–129`) calls `ParagraphNode::layout_runs(..., avail.width)` and only then splits lines. Wrapping uses the frame width of the first split. `FlowNode::Paragraph` (`src/layout/flow.rs` 147, `165`) is the already-wrapped line list. The public DSL never builds it. Layout tests and `paginate.rs` do.

`Flow::text` (`665–667`) is private and is what `CellContent::Text` uses: one `Face::Body` run, parent style, no blank, no bold, no link (`TextNode::new`, `paragraph.rs` 109–117`). `Cell::new("Total")` stays that path.

`Blank` (`897–933`) only borrows a `Paragraph` during the closure. After `f` returns, the stroke settings sit in `InlineItem::Blank` (`paragraph.rs` 43–73`). A cell does not need to store `Blank`.

### `List` is already the list builder, and each item is a nested `Flow`

`List` (`src/document.rs` 1145–1149`) is public. Fields are private and same-module: `parent: Flow`, `items: Vec<Result<ListItem>>`. No public constructor. No `Clone` / `Eq`.

`Flow::ul` (`745–747`) and `Flow::ol` (`750–765`) are the only callers. `ol` maps public `Numbering` (`1131–1143`) onto `MarkerKind` and always uses `start: NonZeroU32::MIN`. There is no public start, reverse, or outline numbering.

Both call `Flow::list` (`767–775`):

```rust
let mut list = List { parent: self.child(), items: Vec::new() };
f(&mut list);
let node = list.lower(kind);
self.push(node);
```

`Flow::child` (`634–636`) clones `style` and copies `DocToken`. `Flow::new` (`613–621`) is private. `Flow` (`603–610`) is documented as body, column, keep-together, or list item. A table cell is not in that list.

`List::item` (`1153–1156`) and `List::item_marked` (`1159–1166`) run `parent.nested(f)` (`652–656`): a child `Flow`, the closure, then `into_nodes`. The nodes go to `ListItem::new` / `ListItem::custom` (`src/layout/list.rs` 106–122`). An empty custom marker is `Error::InvalidGeometry`.

Inside that item `Flow`, the existing methods are the item body. The body-list call site is `tests/document.rs` 277–285: `l.item(|i| i.paragraph(|p| p.text(...)))`. The same closure can call `spacer`, `hline`, `ul`, `ol`. It can also call `columns`, `table`, `page_break`, `keep_together`, and `image`; those fail in `ListItem::new` → `check_content(Container::ListItem)`, not by a second item API. Public proof for image: `tests/document.rs` 718–731, `InvalidCellContent { container: "list item", node: "image" }`. Layout proof: `src/layout/list.rs` 498–520 (table, page break, image).

`List::lower` (`1168–1177`) builds `ListSpec` with `marker_width: Positive::pt(18.0)`, `nest_indent: None`, `marker_style: self.parent.style`, then `FlowNode::List(ListNode::new(spec)?)`. `ListNode::new` (`src/layout/list.rs` 140–169`) turns each `ItemBody` into `FlowNode::Stack` via `into_node` (`src/layout/lower.rs` 110–112`).

`Columns` (`src/document.rs` 935–946`) is the other builder that stores `parent: Flow` and takes `FnOnce(&mut Flow)`. `Flow::columns` (`693–701`) has `&mut self`, so it can call `self.child()` while the closure runs. `Flow::table` does not put a `Flow` on `Table`.

### Layout already accepts the nodes a reused builder would emit

`check_content` (`src/layout/lower.rs` 30–68`):

| Node | `TableCell` | `ListItem` |
| --- | --- | --- |
| `Text`, `Paragraph`, `Spacer`, `HLine` | allowed (`33–36`) | allowed |
| `Stack` | recurse (`37–39`) | recurse |
| `List` | allowed (`56`, `container != Band`) | allowed (nested list) |
| `Image` | allowed (`57`) | rejected |
| `Columns` | rejected (`53`) | rejected |
| `Table` | rejected (`55`, column only) | rejected |
| `KeepTogether` | rejected (`41–45`, column only) | rejected |
| `PageBreak` | rejected (`58`) | rejected |

`node_name` maps both `Paragraph` and `Text` to `"paragraph"` (`72`). A stack fails with the child’s name (`table.rs` 646–656`).

`CellBody::new` (`lower.rs` 90–93`) runs that check. `into_node` (`95–97`) is always `FlowNode::Stack(self.0)`, including a one-element vec. `table::Cell::new` (`table.rs` 58–63`) is that wrapper.

Dedicated layout tests: `cell_rejects_columns_page_break_keep_together_and_tables` (`table.rs` 626–657), `cell_allows_hline` (`660–670`), `cell_allows_spacer` (`673–678`). No `cell_allows_list` and no `cell_allows` for `Text` / `Paragraph`. Text cells are built in layout tests by the private helper `cell()` (`table.rs` 415): `Cell::new(vec![text(s, style)])`, which is `FlowNode::Text`.

`measure` (`table.rs` 191–222`) resolves column widths, then for each placed cell:

- `inner_w` = spanned width minus `2 * inset`
- frame `height` and `full_height` are both `avail.full_height - 2 * inset`
- `natural_height(body.into_node(), ctx, frame)` (`flow.rs` 341–350`)

`inset = border/2 + padding` (`201–202`). Public `Table::lower` uses `TableSpec::new` (`table.rs` 35–47`), so padding and border stay 0 and inset is 0. `natural_height` keeps only `Split::Fit`. `Partial` and `MoveToNextPage` become `Error::UnsplittableTallerThanFrame`. The cell is a `Fragment` after that. `split_measured` moves whole row groups; it does not re-split cell nodes.

`ListNode::split` (`list.rs` 172–244`) can return `Partial` or `MoveToNextPage` when an item does not fit. In a cell that outcome dies in `natural_height`. A list whose total height fits the inner full frame is `Fit`. A list taller than that frame fails the table. Body lists still split (`list.rs` 190–204).

`TextNode::split` wraps at `avail.width`, which in a cell is `inner_w`. A `ParagraphNode` built before measure would have been wrapped at some other width. The public paragraph path avoids that by emitting `Text`.

### Proposal text this seam sits on

`docs/提案书.md` §2.6 (`338–384`):

- Cell children allowed, as each element lands: 段落（含填空）、分隔线、spacer；图片/列表随对应 PR (`377–381`).
- Forbidden in a cell: `columns`, pageBreak, keep_together, nested table.
- Cell content width is column width minus padding and border (`366`). Paragraphs in the cell break at that width.
- DSL signatures are not pinned (`384`): `t.header`, `t.row`, `col_span` / `row_span`, `repeat_header(true)`.

§2.8 (`421–454`): lists are block-level; an item may split across pages; marker only on the item’s first fragment; hanging indent; nest by another indent. Item contents: 段落（含填空）、线、spacer、嵌套列表; no table or columns in v1. 格内列表 is explicitly allowed (`454`). `ol` start defaults to 1; reverse and `1.1.1` are out (`432`).

Image and hline already have `Cell::image` / `Cell::hline`. Spacer has `Cell::spacer`. Paragraph-with-blank/bold/link and list do not.

## Flow

What a caller can do today (`tests/document.rs` 1207–1209, `1253`, `1307–1308`):

1. `Document` derefs to `Flow` (`src/document.rs` 571–588).
2. `Flow::table` (`706–715`) builds a `Table` with no parent `Flow`.
3. `t.row` / `t.header` store `Cell` values.
4. `Cell::hline` / `Cell::image` / `Cell::spacer` / `Cell::new` store one `CellContent` arm. Closures run now.
5. `Table::lower(&Flow)` (`994–1030`) maps that arm to one `FlowNode`, using `flow.style` for text and `flow.doc` for images.
6. `table::Cell::new` → `check_content(TableCell)` → `TableNode`.
7. Later, `measure` bakes each cell with `natural_height` against the full inner frame (`table.rs` 217–222`).

Body rich text and lists never enter that match. They are:

- `doc.paragraph(|p| { p.text; p.bold; p.blank; p.link })` → `Flow::paragraph` → one `FlowNode::Text` on the root flow (`678–690`).
- `doc.ul(|l| l.item(|i| i.paragraph(...)))` → `Flow::list` → `List` whose `parent` is `flow.child()` → `FlowNode::List` (`745–775`, `1153–1177`).

A cell paragraph that reuses `Paragraph` slots into step 4–5 the way `Cell::hline` does:

- Constructor: `Paragraph::default()`, run `FnOnce(&mut Paragraph)`, store the `Paragraph` (or a same-module snapshot of its items, indent, and error).
- Lower arm, same three steps as `Flow::paragraph` (`682–688`): surface `paragraph.error`; clone `flow.style`; set `first_line_indent`; `vec![FlowNode::Text(TextNode::from_items(...))]`.
- `check_content` already allows `Text`. `TextNode::split` then wraps at `inner_w`. Bold is `Face::Bold` on the run (`paragraph.rs` 14–18`); the bold font is `Document::bold_font` (`document.rs` 246–258`), resolved at layout, not a second builder. Blanks and links are already `InlineItem` / `TextRun.link`.

A cell list that reuses `List` and the item `Flow` is the same sequence as `Flow::list` (`767–774`), then `vec![node]` in the lower match. `List::item` keeps calling `Flow::paragraph`, so a list cell gets text/bold/blank/link without a cell-specific paragraph type. Nested `ul` / `ol` keep working because the item `Flow` is a real `Flow`. `ListItem::new` still rejects table, columns, page break, keep-together, and image.

The missing input for that sequence is a `Flow` at the moment `List::item` runs. `Flow::list` has `self`. `Cell::ul` does not. `Table::lower` has `&Flow` and can call `flow.child()` (`634–636`), but only when it runs, after `t.row` has already returned. `Table` does not store a `Flow` today (`962–967`), unlike `Columns` (`936–938`) and `List` (`1146–1148`).

`Flow.style` is never assigned after `Flow::new` (no `.style =` under `src/`). Every flow style is `default_style()` (`591–600`) plus a per-paragraph `first_line_indent` applied in `Flow::paragraph` only. A list built from `flow.child()` and a list built from `Flow::new(default_style(), same_doc)` would match today’s font size, color, and body font id. They diverge as soon as something writes `Flow.style`. `Cell::new` already takes style at lower time (`1008`), so a list lowered through the same `&Flow` stays on that path. `DocToken` (`405–417`) only matters for images; list items reject images (`lower.rs` 57, `tests/document.rs` 728–731`).

## Files Read

- `src/document.rs` — imports `22–29`; `DocToken` / `font` / `bold_font` `405–258`; `Flow` `603–776` (`child`, `nested`, `text`, `paragraph`, `table`, `ul`, `ol`, `list`); `Paragraph` / `Blank` `778–933`; `Columns` `935–960`; `Table::lower` `962–1031`; `Cell` / `CellContent` `1033–1128`; `Numbering` / `List` `1130–1178`; paragraph node tests `1565–1603`; `std::cell::Cell` `1902`.
- `src/lib.rs` `21–24` — re-exports `Cell`, `Flow`, `List`, `Numbering`, `Paragraph`, `Blank`, `Table`. No `FlowNode`.
- `src/layout/lower.rs` — full file. `Container::TableCell`, `check_content`, `node_name`, `CellBody`, `ItemBody`.
- `src/layout/table.rs` `50–73` (`Cell::new`, `with_span`); `191–222` (`measure`, `natural_height`); tests `415`, `626–678`.
- `src/layout/paragraph.rs` `14–73` (`Face`, `TextRun`, `InlineItem`, `InlineBlank`); `101–130` (`TextNode::new`, `from_items`, `split`); `157–171` (wrap width and first-line indent).
- `src/layout/flow.rs` `145–176` (`FlowNode` split); `341–350` (`natural_height`).
- `src/layout/list.rs` `77–84` (`ListKind`); `100–122` (`ListItem`); `140–176` and `190–204` (`ListNode::new`, split / `Partial`).
- `src/error.rs` `14`, `32`, `81–95` — `Error` is `Clone + PartialEq` and not `Eq`. `EmptyLink`, `RunAfterFillBlank`, `UnsplittableTallerThanFrame`, `InvalidCellContent`.
- `src/geom.rs` `21–38` — `Length<C>: Eq`.
- `docs/提案书.md` `338–384` (§2.6), `421–454` (§2.8).
- `tests/document.rs` `277–285` (body ul/ol + `item` + `paragraph`); `718–731` (image in a list item); cell call sites `1207–1209`, `1253`, `1307–1308`. No `Cell::paragraph`, `Cell::ul`, or `Cell::ol`.
- `.audit/handoff/knives-done.md` `22–32` — knife 16 is one spacer payload per cell; knife 17 is cell paragraph and cell list, still forbidding columns / pageBreak / keep_together / nested table.

## Boundaries

- **One public cell, one arm, one node.** `Table::lower` (`1007–1017`) always builds `vec![one FlowNode]`. `CellBody` can hold several nodes. A paragraph and a list in the same public cell, or two paragraphs in one cell, needs a different `CellContent` shape than `hline` / `spacer`. List items already store `Vec<FlowNode>` (`List::item` `1153–1155`).
- **Reuse `Paragraph` for a paragraph cell.** The constructor matches `Cell::hline` (`1071–1079`): associated function, `FnOnce(&mut Paragraph)`, store the result, spans stay 1. Lower matches `Flow::paragraph` (`682–688`) and emits `FlowNode::Text`. `FlowNode::Paragraph` is the post-wrap node (`paragraph.rs` 127–129`). Emitting it from the DSL would wrap before `inner_w` exists (`table.rs` 211–216`).
- **`Cell::new` stays a plain string.** It lowers through `Flow::text` (`665–667`, `1008`). Bold, blank, and link live on `Paragraph`. A rich cell is a new arm, not a change to `From<&str>`.
- **Reuse `List` and the item `Flow` for a list cell.** `List::item` / `item_marked` and `List::lower` (`1153–1177`) are the item API and the node builder. `Flow::ol`’s `Numbering` match (`750–764`) is the numbering map, including `start: NonZeroU32::MIN`. A second list-item builder would duplicate `nested` + `ListItem::new`. Item bodies keep calling `Flow::paragraph`, so link/blank/bold inside an item are the body pipeline.
- **`Cell::ul` / `Cell::ol` cannot copy `Cell::hline`’s closure blindly.** `hline` stores an `HLine` value. `List::item` runs `FnOnce(&mut Flow)` immediately and bakes `FlowNode`s with that `Flow`’s style (`Flow::paragraph` `686–688`). Building the `List` requires `Flow::child` (`634–636`). That method needs an existing `Flow`. The enclosing one is in `Flow::table` / `Table::lower` (`713`, `994`), not in a free `Cell::ul`.
- **Where a reused `List` can see that `Flow`.** Same module, so either site can write `List { parent: flow.child(), items: Vec::new() }` and call `list.lower(kind)`:
  - During `Table::lower`, if the cell stored the caller’s `FnOnce(&mut List)` and lower invokes it. That is `Flow::list` (`767–774`) moved to the match arm. `flow.child()` supplies style and `DocToken`.
  - During the table closure, if `Table` held a child `Flow` the way `Columns` holds `parent` (`936–938`). The closure still has to run on a method that can see that field. `Cell::ul` as a free associated function cannot.
- **Forbidden nodes stay rejected by `check_content`, not by a new cell method.** A list lowered into a cell is a `FlowNode::List`, which `TableCell` allows (`lower.rs` 56`). Columns, page break, keep-together, and nested table inside an item fail as `container: "list item"` (`list.rs` 498–520`, `tests/document.rs` 728–731`). The same four fail as `container: "table cell"` if they are the cell node itself (`table.rs` 626–657`). Image is legal in a cell (`Cell::image`) and illegal in a list item (`lower.rs` 57`). §2.8’s item allow-list matches that.
- **`Cell: Eq` does not survive every storage choice.** Current arms are `Eq` (`String`, `Image`, `ImageBox`, `HLine`, `Length`). `Error` is not `Eq` (`error.rs` 14). `InlineItem` is `PartialEq` only (`paragraph.rs` 43–62`). `Style` is `PartialEq` only because `line_height_factor` is `f32` (`style.rs` 16–26`). `FlowNode` and `List` are not `Eq`. Storing a `Paragraph` that contains `Option<Error>`, a `FlowNode::List`, or a `FnOnce` drops `Cell`’s `Eq` derive (`1034`). Nothing in-tree compares `Cell` with `==`. `Clone + Debug` also fail for a boxed closure.
- **Pagination.** Cell innards must `Fit` the inner full frame (`table.rs` 217–222`, `flow.rs` 347–349`). A wrapping paragraph or a multi-item list that fits that frame lays out as one fragment. One that does not fit fails `UnsplittableTallerThanFrame` instead of continuing on the next page. §2.6 cuts tables on physical rows. §2.8 lets list items split in body flow. Both behaviors are already implemented; a public cell list hits the table rule.
- **Marker column vs cell width.** `List::lower` fixes `marker_width` at 18 pt (`1171`). `ListNode::split` returns `InvalidGeometry` when `body_w <= 0` or `marker_width >= avail.width` (`list.rs` 174–176`). In a cell, `avail.width` is `inner_w`. Public tables are equal-ratio columns and zero inset, so a narrow page or many columns can fail a cell list at measure time.
- **Errors a paragraph cell must keep.** `EmptyLink` and `RunAfterFillBlank` are stored on `Paragraph` and returned by `Flow::paragraph` before a node exists (`682–684`). A lower arm that reads only `items` drops them. Bold without `bold_font` fails later as `UnknownFont` (`document.rs` 1826–1830`), same as body text. Indent that consumes the line width fails as `InvalidGeometry` at layout (`1843–1849`, `paragraph.rs` 167–171`), using the cell’s `inner_w` once the node is `Text`.

## Non-Obvious Things

- **The hline-shaped constructor fits a paragraph and does not by itself fit a list.** `Cell::hline` (`1071–1079`) configures a value with no `Flow`, stores it, and `HLine::lower` (`559–568`) needs no style. `Paragraph` is the same kind of value: `Default`, methods, private items. `List::item` is not: it demands a live `Flow` and lowers children before `Table::lower`. Spacer (`1087–1093`) is the single-value precedent; hline is the closure precedent; list items are the `Columns::col` precedent (`943–946`).
- **Rich text in a cell is `TextNode::from_items`, which `Flow::paragraph` already calls.** `check_content` allows `FlowNode::Paragraph` (`lower.rs` 34`) as well as `Text`. The public pipeline’s rich text is `Text` (`document.rs` 688`). `from_items` (`paragraph.rs` 123–125`) is what preserves bold, blanks, and links. `TextNode::new` (`109–117`) is the plain `Cell::new` path and cannot represent those.
- **Style for a paragraph cell belongs at lower, next to `CellContent::Text`.** `flow.text` clones `flow.style` at lower (`665–667`, `1008`). `Flow::paragraph` does the same and then writes `first_line_indent` (`686–688`). A `Cell::paragraph` that baked `default_style()` inside the constructor would ignore a later change to `Flow.style`, while `Cell::new` would not. Today those styles match only because `Flow.style` is immutable after `new`.
- **A list cell can reuse `Paragraph` without `Cell::paragraph`.** The item closure receives `&mut Flow` (`1153`). `Flow::paragraph` is already public on that `Flow`. `Cell::paragraph` is only the whole-cell factory, analogous to `Cell::hline` versus `item.hline` (`Flow::hline` `733–737`).
- **Deferring the list closure until `Table::lower` reuses `List` without a parallel builder.** Lower already has `&Flow`. `flow.child()` is the `parent` `Flow::list` uses. The stored value is the closure, not a `FlowNode` and not a second item type. Cost: `Cell` gains a lifetime (the closure borrows the `table` closure’s environment, and `lower` runs before `Flow::table` returns, `713–714`), and that arm is not `Clone` / `Eq` / `Debug`. An eager `Table` method that owns a child `Flow` also reuses `List`, and the call is not spelled `Cell::ul` unless the method can see `Table`’s flow.
- **Synthesizing `Flow::new(default_style(), DocToken::next())` inside `Cell::ul` is a third `Flow`, not the document’s.** Style happens to be identical today. The token would not be the document’s (`410–416`). Legal list items do not call `image`, so the token is unused. It becomes a real bug if item content later includes an image, and it splits from `Cell::new` if `Flow.style` ever changes.
- **Putting a raw `Flow` closure on the cell (`Cell::stack`) is a different API.** It would let one cell hold several nodes, which `CellBody` can already store, and it would admit `columns` / `page_break` / `keep_together` / `table` into the cell for `check_content` to reject (`table.rs` 626–657`). Knife 16 left that shape alone so spacer would not drag paragraph and list in with it. Knife 17 can add a paragraph arm and a list arm and still leave the cell as one payload.
- **Nested lists inside a cell list are already legal at both layers.** `List` in a `ListItem` passes `check_content` (`lower.rs` 56`). The item `Flow`’s `ul` / `ol` build another `List` via `Flow::list`. §2.8 asks for that nesting (`445–453`). No cell-specific nest type.
- **`ol` in a cell inherits “start at 1”.** `Flow::ol` hardcodes `NonZeroU32::MIN` (`761`). §2.8’s configurable `start` is not on the public method. A cell `ol` that adds `start` would be ahead of the body API.
- **Public re-exports already include the builders.** `Paragraph`, `Blank`, `List`, `Numbering`, and `Flow` are in `src/lib.rs` 21–24. The gap is `CellContent` plus constructors plus the `Table::lower` match, not a new public type.

## Open Questions

- §2.6 `384` still does not pin the cell signature. The in-tree whole-cell pattern is `Cell::paragraph(f)` beside `Cell::hline` / `Cell::spacer`. The in-tree list pattern is `Flow::ul` / `Flow::ol` plus `List::item`. Whether the list factory is spelled `Cell::ul` (closure stored, run at lower) or a `Table` method that already holds a child `Flow` is open. Both can call the existing `List` and `Flow::list` sequence. A free `Cell::ul` that runs the closure immediately has no `Flow` unless `Table` or `Flow::table` is changed to provide one.
- §2.6 lists 段落 and 列表 as allowed child kinds. It does not say whether one cell holds one of them or a sequence. Layout `CellBody(Vec<FlowNode>)` (`lower.rs` 87–97`) can hold a sequence. Public `CellContent` cannot. Knife 16 kept one payload per cell (`.audit/handoff/knives-done.md` 24).
- Storing `Paragraph` in `CellContent` conflicts with `Cell: Eq` because `paragraph.error` is `Error` (`error.rs` 14, not `Eq`) and `InlineItem` is only `PartialEq` (`paragraph.rs` 43). A same-module snapshot can avoid storing `Error` by using a small `Eq` code for `EmptyLink` / `RunAfterFillBlank`, or `Cell` can drop `Eq`. No caller relies on `Eq` in this repo. A deferred list closure cannot be `Eq` or `Clone`.
- §2.8 says a list item may split across pages. §2.6 says a table splits on rows. `natural_height` (`flow.rs` 347–349`) implements the table rule for every cell child, including a future `FlowNode::List` or `FlowNode::Text`. A cell list taller than the inner full frame fails. Whether that stays is the same question knife 16 closed for spacer by keeping `natural_height`.
- No test calls `table::Cell::new` with `FlowNode::List` or with a `TextNode::from_items` paragraph. The allow is the shared match in `lower.rs` 33–36 and `56`. Body coverage is `tests/document.rs` 277–285 and the `document.rs` unit tests at `1565–1603`.
- Public tables still cannot set padding or border (§2.6 `360–366`). `Table::lower` leaves them at zero (`1023–1027`, `table.rs` 41–46`). Cell wrap width is the full column width until that changes. A 18 pt marker (`document.rs` 1171`) is compared to that width in `ListNode::split` (`list.rs` 174–176`).
