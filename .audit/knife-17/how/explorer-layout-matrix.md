### Components Found

Two cell types, one content matrix. Callers see `document::Cell` (`src/document.rs` 1033–1110). Layout sees `table::Cell` (`src/layout/table.rs` 50–73), whose body is `CellBody` (`src/layout/lower.rs` 85–98). `Flow` is documented as the body, a column, a keep-together group, or a list item (`src/document.rs` 603–604). A table cell is not one of those.

**What a public cell can lower today.** `CellContent` (`src/document.rs` 1042–1048) is one payload: `Text(String)`, `Image`, `HLine`, or `Spacer`. `Table::lower` (`994–1029`) always builds a one-element `Vec` and passes it to `table::Cell::new`. Plain text becomes `FlowNode::Text` via private `Flow::text` (`665–667`), which is `TextNode::new` — one body run, no blank, no link, no bold, no first-line indent. There is no `Cell` constructor that calls `Flow::paragraph` (`679–689`) or `Flow::ul` / `Flow::ol` (`745–775`).

**What `CellBody` already accepts.** `Cell::new` (`src/layout/table.rs` 58–63) calls `CellBody::new` → `check_content(Container::TableCell, &nodes)` (`src/layout/lower.rs` 90–92`). The same function serves columns, cells, list items, and header/footer bands (`Container`, 10–27). One `match` (`30–58`):

| Node | Column | Table cell | List item | Band |
| --- | --- | --- | --- | --- |
| `Text`, `Paragraph`, `Spacer`, `HLine` | allow | allow | allow | allow |
| `Stack` | recurse, same container | recurse | recurse | recurse |
| `KeepTogether` | allow; `Open` kids are rechecked as a column (`41–45`) | deny | deny | deny |
| `Columns` | allow, kids not rechecked here | deny | deny | allow, then recheck each column stack (`49–51`) |
| `Table` | allow (`55`) | deny | deny | deny |
| `List` | allow (`56`, `container != Band`) | allow | allow | deny |
| `Image` | allow (`57`) | allow | deny | deny |
| `PageBreak` | deny | deny | deny | deny |

A `Vec` of several of the allowed nodes is legal. `check_content` walks that vec directly; it does not require the caller to wrap it in `Stack` first. `CellBody::into_node` (`95–97`) always wraps the vec as `FlowNode::Stack`, including a one-node vec. `ItemBody` (`100–113`) is the same wrapper with `Container::ListItem`.

**Rich paragraph is not a new node.** `Flow::paragraph` builds `FlowNode::Text(TextNode::from_items(paragraph.items, style))` and copies `first_line_indent` onto that style (`src/document.rs` 686–688`). Blanks and links are `InlineItem` / `TextRun.link` (`src/layout/paragraph.rs` 21–47, 44–47). `node_name` folds `Paragraph` and `Text` into the single label `"paragraph"` (`src/layout/lower.rs` 72). The matrix never sees a blank or a link. `TextNode::split` (`128–130`) is the entry: it calls `ParagraphNode::layout_runs` at the frame width, then `ParagraphNode::split`. `FlowNode::Paragraph` is only the already-wrapped remainder (`paragraph.rs` 232, 259).

**List entry.** `List::item` / `item_marked` (`src/document.rs` 1153–1165) run the closure on a child `Flow`, then `ListItem::new` / `custom` (`src/layout/list.rs` 105–122`), which run `ItemBody::new`. `List::lower` (`1168–1176`) builds `FlowNode::List(ListNode::new(spec)?)` with `marker_width` 18 pt and `nest_indent: None`. `ListNode::new` (`140–169`) turns each item body into a `Stack` via `into_node` and stores a pending marker string. `ListNode::split` (`172–243`) is the measure entry.

**Proposal.** §2.6 (`docs/提案书.md` 338–384): a cell may hold 段落（含填空）、分隔线、spacer, and 图片/列表 as those elements land; it may not hold `columns`, pageBreak, keep_together, or a nested table. The split unit is a whole physical row. §2.1 (201): a paragraph in a cell wraps at the cell content width. §2.8 (453–454): a list item holds 段落（含填空）、线、spacer、嵌套列表, and not table/columns; a list inside a cell is allowed. §2.9 (458): pageBreak inside a cell or a list item is `InvalidCellContent`.

### Flow

A rich paragraph or a list reaches a cell only if `Table::lower` emits nodes the matrix already allows. Measure then refuses to split those nodes inside the cell.

1. Assembly. A paragraph closure fills `Paragraph.items` (`text` / `bold` / `blank*` / `link`, `src/document.rs` 797–845). `Flow::paragraph` turns that into one `FlowNode::Text`. A list closure fills `ListItem`s; each item is checked as `ListItem` before the list node exists. `List::lower` returns one `FlowNode::List`.
2. Cell check. `table::Cell::new(vec![…])` runs `check_content(TableCell)`. `Text` is on the unconditional arm (`lower.rs` 33–36`). `List` is allowed because the container is not `Band` (`56`). `Stack` of those same nodes recurses (`37–39`). Columns, page break, keep-together, and nested table return `Error::InvalidCellContent { container: "table cell", node }` (`61–64`). Display text is `"{node} is not allowed in a {container}"` (`src/error.rs` 168–169`).
3. Grid. `TableNode::new` (`src/layout/table.rs` 124–179`) stores the `CellBody` on a `PlacedCell`. Nothing in that pass measures height.
4. First split measures every cell against the **full** inner frame, not the leftover page. `measure` (`191–222`): `inset = border/2 + padding`, `inner_full = avail.full_height - 2*inset`, content width `inner_w = span width - 2*inset`. The cell frame is `{ width: inner_w, height: inner_full, full_height: inner_full }`. Public tables spread `TableSpec::new` (`document.rs` 1023–1028`), whose padding and border are zero (`table.rs` 41–46), so `inner_full` is the page body height and `inner_w` is the column width.
5. `natural_height(body.into_node(), …)` (`table.rs` 222`, `flow.rs` 341–350`) keeps only `Split::Fit`. `Partial` and `MoveToNextPage` become `Error::UnsplittableTallerThanFrame`. That aborts `TableNode::split` (`182–187`) before `split_measured`. A cell stack that would paginate in the body does not become a split row.
6. Row boxes are placed later as whole fragments (`split_measured`, `340–390`). A row group taller than `avail.full_height` already failed in `measure` (`278–287`). A group that fits the full body but not the leftover page moves whole (`MoveToNextPage` or `Partial` of the **table**, not of the cell).

**Paragraph inside that frame.** `layout_runs` (`paragraph.rs` 157–214`) wraps at `width = inner_w`. First line budget is `inner_w - first_line_indent` (`167–170`); a non-positive budget is `InvalidGeometry`. `prepare_inline` (`393–437`) groups runs; `blank_advance` (`439–450`) turns a fixed or em blank into a fixed advance and `ToEndOfLine` into `Advance::Fill`, which consumes the rest of that line's budget — the cell width, or the list body width when the paragraph sits in an item. `place_line` (`453–500`) emits a `LinePiece::Stroke` for a positive-width blank and a `LinePiece::Link` for a linked glyph advance. `fragment_from_lines` (`320–363`) turns strokes into `FragmentCommand::Stroke` and links into `FragmentCommand::Link` rects one line tall. `Fragment::with_offset` translates both (`flow.rs` 70–75`), and `measure` offsets the fragment to `(x + inset, y_in_group + inset)` (`table.rs` 256–259`). Blank and link paint ride along if the paragraph `Fit`s. They are not a second content check.

`ParagraphNode::split` (`218–261`) against a cell frame (`height == full_height == inner_full`):

- `line_height > inner_full` → `UnsplittableTallerThanFrame` directly (`228–229`).
- Every line fits → `Fit`. Height is `line_count * line_height`. `spacing_after` is `style.paragraph_spacing` (`212`) and is stored on the fragment, not added to that height (`240–243`, `315–317`).
- Some lines fit and some do not → `Partial` with a `Paragraph` tail (`256–260`). `natural_height` throws the head away and returns `UnsplittableTallerThanFrame`.
- Not even one line fits the remaining height → `MoveToNextPage` (`231–237`), same error from `natural_height`.

Public `default_style` sets `paragraph_spacing` and `first_line_indent` to zero (`document.rs` 591–600`). `Flow::paragraph` overwrites only `first_line_indent`. There is no public setter for `paragraph_spacing`. A cell that reuses this lower path therefore wraps and links like a body paragraph, and its trailing paragraph gap does not increase the row.

**Several nodes in one cell.** `into_node` makes one `Stack`, so `split_stack` (`flow.rs` 252–339`) runs. On `Fit`, `y += height` and `avail.height` shrinks (`272–281`). If another child follows and `spacing_after > 0` and the remainder can hold that gap, the gap is added to `y` (`283–288`). If the gap does not fit, it is dropped and the next child is tried at the same `y`. The last child's `spacing_after` becomes `fragment.spacing_after` and is **not** part of the `Fit` height (`331–337`). `measure` uses that height for `cell_h` (`223`), so only gaps **between** children, and only when they fit in `inner_full`, make the row taller. A later child that then returns `Partial` or `MoveToNextPage` still fails the whole cell (`290–326` then `natural_height`). The dropped gap is not retried on the next page, because the cell never keeps a tail.

**List inside that frame.** The cell frame width is `inner_w`. `ListNode::split` (`list.rs` 172–176`) sets `body_w = inner_w - indent`. With the public spec, `indent == marker_width == 18` (`document.rs` 1171–1172`, `list.rs` 142`). `body_w <= 0` or `marker_width >= inner_w` is `InvalidGeometry` at measure time, not `InvalidCellContent`. Each item body is split at `body_w` (`183–188`), so a paragraph in an item wraps at `inner_w - indent`, then its own first-line indent (`paragraph.rs` 167–184`). That is §2.1's cell width composed with §2.8's hanging indent. There is no cell-specific list branch.

A list `Fit` draws every item's marker (`206–213`), because a successful cell never stores a continuation item. The body-flow behavior — marker only on the item's first fragment, continuation keeps `pending_marker: None` (`220–225`) — is what `nested` pagination uses in the body (`list.rs` test at `342–356` shows the indent; the split-across-pages test around `320–338` shows a continuation line without repeating `1.`). Inside a cell, `Partial` / `MoveToNextPage` from `ListNode::split` (`190–204`, `246–255`) hit `natural_height` and become `UnsplittableTallerThanFrame`. A marker wider than `marker_width` fails inside `single_line_run` during that same split (`208–209`, `paragraph.rs` 265–278`) as `UnbreakableWiderThanFrame`, again at table measure, not at `CellBody::new`.

**Nested list.** `List` is allowed in a list item (`lower.rs` 56`). `ListItem::new(vec![text, FlowNode::List(inner)])` succeeds; the nested-list test (`list.rs` 342–356`) builds that and lays it out in the body. Put the outer `FlowNode::List` in a cell and `check_content(TableCell)` allows the list node without opening it. Inner items were already checked as `ListItem`. Each extra level subtracts another `indent` from `body_w` when that inner list splits.

**What fails, and the error a list shares.** Denied nodes never reach `natural_height`:

| Node in a cell | `check_content` | `node` string | Test |
| --- | --- | --- | --- |
| `Columns` | false (`53`) | `"columns"` | `table.rs` 638–639 |
| `PageBreak` | false (`58`) | `"page break"` | 640 |
| `KeepTogether` (`Open` or `Prepared`) | false (`41–45`) | `"keep together"` | 641–644, empty `Open` only |
| `Table` | false (`55`) | `"table"` | 645 |
| `Stack([PageBreak])` | recurse, then the inner deny | `"page break"` (not `"stack"`) | 646 |

`PageBreak.split` would be `InvalidGeometry` if it ever ran (`flow.rs` 164`). The paginator special-cases page breaks only in the body (`paginate.rs` 144). The matrix is what enforces §2.9 inside a cell.

List items use the same error variant. `item_rejects_table_page_break_and_image` (`list.rs` 498–525`) expects `InvalidCellContent { container: "list item", … }` for `table`, `page break`, and `image`. The arms that deny `columns` and `keep together` are the same booleans as the cell (`53`, `41–45`); the list test does not mention them. `item_allows_hline` (`529–538`) is the positive sibling of `cell_allows_hline` (`table.rs` 660–670`). Nothing in `table.rs` asserts that a `List`, a `Paragraph`, a `Text` with blanks, or a multi-node vec is accepted. `cell_allows_spacer` (`673–678`) and the hline test are the positive cell arms.

Image is the one allow-bit that differs: a cell may hold `FlowNode::Image` (`57`); a list item may not (`498–525`, and the public test `tests/document.rs` 718–731`). A list placed in a cell cannot smuggle an image, a table, columns, a page break, or keep-together, because `ItemBody::new` already rejected those. The cell check then sees only the `List` node and allows it. The diagnostic stays `container: "list item"`, not `"table cell"`.

`KeepTogether` is not opened in a cell or a list item. The recurse guard requires `Container::Column` and `Open` (`41–43`). `KeepTogether(Open([PageBreak]))` in a cell is reported as `"keep together"`. The inner page break is not visited. `Stack([PageBreak])` is reported as `"page break"`. A `Columns` node is checked from the inside first: `ColumnsNode::new` calls `check_content(Column)` on each column (`columns.rs` 114–115`) before the parent container sees the `Columns` node. A page break in a column therefore fails as `"column"` / `"page break"` (public test `tests/document.rs` 380–395`) and never becomes a cell or list-item error.

### Files Read

- `docs/提案书.md` — §2.1 cell wrap width (193–202), §2.6 cell children and row split (338–384), §2.8 item children and 格内列表 (421–454), §2.9 pageBreak (456–458).
- `src/layout/lower.rs` — `Container`, `check_content`, `node_name`, `CellBody`, `ItemBody`.
- `src/layout/flow.rs` — `FlowNode::split`, `split_stack`, `natural_height`, `Fragment::with_offset`, `FragmentCommand::Link`.
- `src/layout/table.rs` — `Cell::new`, `measure`, `split_measured`, `cell_rejects_…`, `cell_allows_hline`, `cell_allows_spacer`.
- `src/layout/paragraph.rs` — `TextNode::split`, `layout_runs`, `ParagraphNode::split`, `fragment_from_lines`, `prepare_inline`, `blank_advance`, `place_line`, `single_line_run`.
- `src/layout/list.rs` — `ListItem`, `ListNode::new`, `ListNode::split`, nested-list test, `item_rejects_table_page_break_and_image`, `item_allows_hline`.
- `src/layout/columns.rs` — `ColumnsNode::new` content check (103–124).
- `src/layout/decorate.rs` — `measure_band` reuses `check_content(Band)` (248–269); band tests name the same error (479, 507).
- `src/layout/style.rs` — `paragraph_spacing`, `first_line_indent`.
- `src/document.rs` — `Flow::text`, `Flow::paragraph`, `Table::lower`, `Cell` / `CellContent`, `List::item` / `lower`, `default_style`.
- `src/error.rs` — `InvalidCellContent` (89–95, 168–169), `UnsplittableTallerThanFrame` (82–83, 164–166).
- `tests/document.rs` — column page-break reports `"column"` (380–395); list item image reports `"list item"` (718–731).

### Boundaries

- Layout can already hold a cell body of `Text` (rich items), `Paragraph`, `List`, `Spacer`, `HLine`, `Image`, and stacks of those. The public `Cell` still stores one `CellContent` and lowers exactly one node. Gaining paragraphs and lists is a lower-arm change onto that vec, not a new `FlowNode` and not an edit to the allow arms.
- Columns, pageBreak, keep_together, and nested table already fail `CellBody::new` with `InvalidCellContent` and container `"table cell"`. A list item fails the same variant with container `"list item"`. Lists do not need a second matrix to stay forbidden from holding table/columns/pageBreak.
- A cell list or a multi-line paragraph must `Fit` in `inner_full`. Line-at-a-time paragraph split and item-at-a-time list split stay available in the body. They are discarded for cell content by `natural_height`. That matches §2.6's whole-row split: the row may move to the next page; the cell may not break in half.
- `check_content` does not walk into `List` or into `KeepTogether` unless the container is a column and the group is `Open`. Forbidden content inside a list is diagnosed when the item is built. Forbidden content inside keep-together is diagnosed as the keep-together node itself.
- Empty `CellBody` passes the matrix (`check_content` on `[]` is `Ok`) and measures as height 0 (`split_stack` 257–261). Public `lower` never builds that vec.
- This pass did not run `cargo test` and did not execute a cell whose body is a multi-node stack, a blank, a link, or a list. The height rules above are from reading `measure`, `natural_height`, `split_stack`, `ParagraphNode::split`, and `ListNode::split`.

### Non-Obvious Things

- **`InvalidCellContent` is the shared nesting error, not a cell-only error.** Bands use it too (`decorate.rs` 479, 507). The container string is the only distinction. A list-in-a-cell that contains an image fails earlier, as `"image" is not allowed in a list item`, even though the cell matrix allows images. The cell matrix has no `"list"` rejection.
- **The list rejection test is a subset of the cell rejection test.** Both deny table and page break. The cell test also denies columns, keep-together, and a page break wrapped in `Stack`. The list test adds image and omits columns and keep-together. The code path for those two is the same `false` result; only the cell test locks the strings `"columns"` and `"keep together"`.
- **Stack names the inner node; keep-together names itself.** `Stack([PageBreak])` in a cell is `"page break"` (`table.rs` 646–655`). `KeepTogether` in a cell does not recurse, so a page break wrapped only in keep-together would be reported as `"keep together"`. Columns fail one level earlier, as a column error, if their own kids are illegal.
- **`Text` and `Paragraph` are one label and two stages.** The public rich paragraph is a `Text` node until `TextNode::split` wraps it. Both are allowed. A cell does not need a `FlowNode::Paragraph` at assembly time for blanks and links to work; those are inline pieces of the `Text` node, painted only if the whole wrap `Fit`s.
- **Trailing paragraph spacing does not grow the row. A gap that does not fit is deleted.** `natural_height` returns the stack's `y`, and the last child's `spacing_after` stays on the fragment. Between children, `split_stack` inserts the gap only when `avail.height >= spacing` (`283–288`). Public spacing is 0, so this is dormant for today's `Flow::paragraph`. A style with a positive `paragraph_spacing` would change row height only for gaps that fit, and would silently close a gap that does not, then still error if the next child cannot start.
- **A list that fits the full body still shows every marker.** The "no marker on the continuation" rule runs only when `ListNode::split` returns a tail. Cell measure never keeps that tail.
- **Narrow columns fail a list geometrically.** Public marker column is 18 pt. `inner_w <= 18` makes `ListNode::split` return `InvalidGeometry` during `measure`, after `CellBody::new` has already succeeded.

### Open Questions

- §2.6 leaves the cell DSL signature unpinned (384). The layout vec can already mix a rich `Text`, a `List`, a spacer, and a rule. Whether the public cell grows one closure, several constructors, or stays one payload is not answered by the matrix. A closure that reuses `Flow` will also accept calls the matrix then rejects; the error string depends on the wrapper (`Stack` vs `KeepTogether` vs `Columns`), as above.
- No test passes `FlowNode::List` or `TextNode::from_items` (blanks, links, indent) through `table::Cell::new` and `measure`. The allow lines and the body-flow paragraph/list tests are the evidence that a fitting cell would paint them. A cell narrower than 18 pt, a list taller than `inner_full`, and a paragraph whose first-line indent consumes `inner_w` are unread by tests.
- `paragraph_spacing` is layout state with no public setter, default 0. If a cell paragraph starts setting it, the row-height rule (trailing gap ignored, an unfitting gap dropped) is what `split_stack` will do. §2.6 does not say whether the gap is part of the row.
