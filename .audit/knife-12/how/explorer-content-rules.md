# Explorer: nested container content rules (separator host matrix)

Workspace: `/mnt/data/demo/quire` @ `7cb1654` (`feat(document): draw line, rect, and ellipse on behind/front layers`).

Scope: who may hold which `FlowNode`, and which match arms must change so a §2.3 separator is legal in table cells, list items, and columns, and still rejected in header/footer bands (including when wrapped in structures those bands already allow).

There is no `FlowNode` separator variant today. `rg` over `src/` finds no `hline` / `rule` / `Separator` flow node.

## Components Found

### One matrix: `Container` + `check_content`

`src/layout/lower.rs` is the only content-rule matrix (module comment, lines 1–2).

`Container` (`lower.rs:11–16`): `Column`, `TableCell`, `ListItem`, `Band`.

Display names (`lower.rs:18–26`):

| Variant | `container` string |
| --- | --- |
| `Column` | `"column"` |
| `TableCell` | `"table cell"` |
| `ListItem` | `"list item"` |
| `Band` | `"header/footer band"` |

`check_content` (`lower.rs:30–65`) walks `&[FlowNode]` in order. First rejection returns `Error::InvalidCellContent { container, node }` (`error.rs:88–93`). Display (`error.rs:165–166`): `"{node} is not allowed in a {container}"`. The same error type is used for all four containers.

Current allow/deny (`lower.rs:32–56`). `true` means the node itself is legal in that container; recursion notes are separate.

| `FlowNode` arm | Column | TableCell | ListItem | Band | Recurse? |
| --- | --- | --- | --- | --- | --- |
| `Text` / `Paragraph` / `Spacer` (`:33`) | allow | allow | allow | allow | no (leaves) |
| `Stack(kids)` (`:34–37`) | allow | allow | allow | allow | **yes, same container**, before allow |
| `KeepTogether(Open(kids))` (`:38–42`) | allow | **deny** | **deny** | **deny** | **only if container is `Column`** |
| `KeepTogether(Prepared(_))` (`:38–42`) | allow | deny | deny | deny | **never** (if-let requires `Open`) |
| `Columns` (`:44–51`) | allow | **deny** | **deny** | allow | **only if container is `Band`**: `check_content(Band, columns.column_stacks())` |
| `Table` (`:52`) | allow | deny | deny | deny | never |
| `List` (`:53`) | allow | allow | allow | **deny** | never |
| `Image` (`:54`) | allow | allow | **deny** | **deny** | never |
| `PageBreak` (`:55`) | deny | deny | deny | deny | n/a |

`node_name` (`lower.rs:67–78`): `Paragraph` and `Text` both report `"paragraph"`. Others: `"spacer"`, `"page break"`, `"keep together"`, `"stack"`, `"columns"`, `"table"`, `"list"`, `"image"`. Stack rarely appears in errors because an illegal descendant fails first (see Flow).

### Checked wrappers

- `CellBody` (`lower.rs:81–94`): `new` calls `check_content(Container::TableCell, &nodes)`. `into_node` wraps the vec as `FlowNode::Stack` (`:91–93`) with **no second check**.
- `ItemBody` (`lower.rs:96–109`): same with `Container::ListItem`, then `FlowNode::Stack`.
- `ColumnsNode::new` (`columns.rs:103–124`): each column's kids are `check_content(Container::Column, &kids)` **then** stored as `FlowNode::Stack(kids)` (`:117`). Fewer than 2 columns is `Error::ColumnWidths { reason: "column-count" }` (`:107–110`), before the content walk of later columns only if `columns.len() < 2` returns first — a one-column row never reaches `check_content`. `column_stacks` (`columns.rs:126–128`) returns those already-wrapped stacks.
- Split tails rebuild `ColumnsNode { .. }` directly (`columns.rs:169–173`) and do **not** call `check_content` again.
- `measure_band` (`decorate.rs:248–257`): non-empty band nodes call `check_content(Container::Band, &nodes)` **then** `FlowNode::Stack(nodes).split`. Empty vec skips the check and returns `Ok(None)` (`:254–256`).

`FlowNode` (`flow.rs:138–150`) variants today: `Paragraph`, `Text`, `Spacer`, `PageBreak`, `KeepTogether`, `Stack`, `Columns`, `Table`, `List`, `Image`. `split` (`flow.rs:152–167`) is an exhaustive match. `PageBreak` returns `Error::InvalidGeometry` (`:156`) and is not a normal split. `natural_height` (`flow.rs:333–341`) only accepts `Split::Fit`; `Partial` or `MoveToNextPage` becomes `UnsplittableTallerThanFrame`.

### Who calls the check (construction vs decorate time)

| Host | Call site | When |
| --- | --- | --- |
| Table cell | `table::Cell::new` → `CellBody::new` (`table.rs:57–63`) | table lower / `TableNode` build |
| List item | `ListItem::new` / `custom` → `ItemBody::new` (`list.rs:105–121`) | list lower |
| Column | `ColumnsNode::new` (`columns.rs:115`) | columns lower |
| Header/footer | `measure_band` (`decorate.rs:257`) | **after** body pagination, inside `measure_bands` (`decorate.rs:101–165`), called from `paginate_sections` **before** `paint_pages` (`paginate.rs:116–117`) |

Document root is **not** passed through `check_content`. `Section::lower` (`document.rs:141–160`) does `FlowNode::Stack(flow.into_nodes()?)` only. Body `page_break`, tables, images, lists, columns, keep-together are legal because nothing rejects them. A new separator pushed on the root `Flow` is legal the same way.

`plan_pages` (`paginate.rs:126–147`) special-cases only `FlowNode::PageBreak`. Every other node, including a future separator, falls through to `node.split`. No new paginate arm is required unless a separator were a cursor mark (proposal says it is an unsplittable block, not a page break: `docs/提案书.md:282`).

### Public DSL vs the matrix

`Flow` (`document.rs:506–513`) doc: "the body, a column, a keep-together group, or a list item." Not cells. Not bands.

`Flow::child` (`document.rs:537–538`) clones style and doc token into a fresh node list. `nested` (`:555–558`) runs a closure on that child and `into_nodes`.

- Columns: `Columns::col` (`document.rs:687–689`) → `parent.nested` → `Columns::lower` (`:692–702`) → `ColumnsNode::new`.
- List items: `List::item` (`:867–869`) → `nested` → `ListItem::new`. `item_marked` (`:873–879`) → `ListItem::custom`.
- `keep_together` (`:615–619`) wraps `nested` kids in `KeepTogetherNode::Open`.
- `page_break` (`:628–629`) pushes `FlowNode::PageBreak` with no local whitelist. Rejection is only when a container later runs `check_content`.

Public table cells do **not** take a `Flow`. `document::Cell` (`document.rs:773–824`) holds `CellContent` (`:782–786`):

- `Text(String)` via `Cell::new` / `From<&str|String|&String>` (`:788–841`)
- `Image(Image, ImageBox)` via `Cell::image` (`:799–806`)

`Table::lower` (`document.rs:738–769`) maps those two arms only (`:751–755`) into `table::Cell::new(vec![...])`. Layout therefore allows spacer, paragraph, stack, and list inside a cell, but the public builder cannot emit them. A separator allowed by `check_content(TableCell)` is still unreachable from `t.row` / `t.header` until `CellContent` and this match grow an arm.

Public bands do **not** take a `Flow`. `Band` (`document.rs:941–1000`) has `left` / `right` strings only (`:955–961`). `Band::lower` (`:964–999`):

- each set slot is one `FlowNode::Text` (`:965–971`)
- zero slots → empty vec (`:977–978`)
- one track and zero horizontal margin insets → that track's nodes unwrapped (`:993–994`)
- otherwise `FlowNode::Columns` with gap `NonNegative::ZERO`, equal-ratio slots, plus fixed-width **empty** columns for non-zero left/right margins (`:980–998`)

`Band::lower` does not call `check_content`. `band_content` (`document.rs:1003–1012`) runs the hook per page and returns `band.lower`. `measure_band` is the backstop. Header and footer share `Container::Band` (same check, same error string).

`Layer` (`document.rs:1016+`) is page-coordinate escape, not a `FlowNode` host: `text_at` (`:1038`), `image_at` (`:1049`), `line` (`:1066`), `rect` (`:1071`). `Layer::line` is a free segment in page space. It is not the §2.3 flow rule. §2.3 (`docs/提案书.md:272`) says the separator is not the `behind` escape path.

### Proposal hosts (re-read)

`docs/提案书.md`:

- §2.3 (`:270–285`): block flow node, full current frame width `W` by default, unsplittable, horizontal stroke. Not an inline blank, not `behind`.
- §2.4 (`:296`): a column's inner stack may hold "段落、表、线…".
- §2.5 (`:324`): column may hold 段落、线、嵌套 stack/columns、keep_together; tables follow each element's own rule.
- §2.6 cell children (`:377–382`): **allow** 段落（含填空）、分隔线、spacer; image/list as those PRs land. **Forbid** `columns`, pageBreak, keep_together, nested table.
- §2.7 (`:415`): image allowed in cells.
- §2.8 (`:453–454`): item may hold 段落（含填空）、线、spacer、嵌套列表; **not** table/columns in v1. Lists inside cells allowed.
- §2.9 (`:458`): pageBreak forbidden in cells and list items via `InvalidCellContent`.
- §2.10 (`:486–499`): band height = top/bottom margin; content that does not fit errors. **No** separator (or any flow-node list) as band content.
- `:661`: DSL name `hline` / `rule` still unpinned.

§2.6's "随对应 PR" for image and list is already implemented: `Image` is `Column | TableCell` (`lower.rs:54`); `List` is every container except `Band` (`:53`).

### Tests that pin rejects

Layout:

- `table::tests::cell_rejects_columns_page_break_keep_together_and_tables` (`table.rs:626–657`). `table::Cell::new` (not `document::Cell`). Rejected, container `"table cell"`:
  - `Columns` → `"columns"`
  - `PageBreak` → `"page break"`
  - `KeepTogether(Open(empty))` → `"keep together"`
  - nested `Table` → `"table"`
  - `Stack([PageBreak])` → `"page break"` (inner name, not `"stack"`)
- `list::tests::item_rejects_table_page_break_and_image` (`list.rs:496–524`). `ListItem::new`. Container `"list item"`: table, page break, image. Does **not** assert columns or keep-together, though the matrix denies both.
- `decorate::tests::band_rejects_images` (`decorate.rs:452–485`): header hook returns `FlowNode::Image`. Container `"header/footer band"`, node `"image"`. Canvas commands empty.
- `decorate::tests::band_rejects_tables` (`:488–512`): bare `FlowNode::Table`. node `"table"`.
- `decorate::tests::band_columns_still_reject_a_nested_table` (`:553–580`): header is `Columns` whose left stack is text and right stack is a `Table`. Error is still container `"header/footer band"`, node `"table"`, not `"column"` / `"columns"`. This is the anti-smuggle pin: column construction used `Container::Column` (table legal there), band re-walks `column_stacks` as `Band`.

Document (`tests/document.rs`):

- `nesting_rules_come_from_layout` (`:378–394`): `doc.columns` → `col(Flow::page_break)` → `InvalidCellContent { container: "column", node: "page break" }`. Proves column rules run at DSL lower, and `Flow::page_break` itself does not reject.
- `list_item_and_overwide_image_fail` (`:593–607`): `ul` item `image` → container `"list item"`, node `"image"`.

Positive pins (these hosts already accept the node; a separator must not be folded into a stricter arm):

- Images in columns: `image::tests::column_offsets_the_image` (`image.rs:235–250`) `ColumnsNode::new` with `FlowNode::Image` succeeds.
- Keep-together in a column: `columns::tests::stuck_column_on_full_page_errors_with_no_output` (`columns.rs:426–435`) builds `KeepTogether(Open([text]))` inside a column successfully (failure later is height, not `InvalidCellContent`).
- Nested list in an item: `list.rs:353` `ListItem::new([text, FlowNode::List(...)])` succeeds.
- Band columns of text succeed: `band_columns_share_one_line` (`decorate.rs:522–550`).
- Cell images are a public path: `Cell::image` (`document.rs:799`) → `Table::lower` image arm (`:753–755`).

No test asserts "list inside a cell succeeds" or "spacer inside a cell succeeds". Both are allow-arms only.

## Flow

### Check entry, then layout

1. **Cell.** `document::Table::lower` builds a `Vec<FlowNode>` per cell (`document.rs:751–757`) → `table::Cell::new` → `CellBody::new` → `check_content(TableCell)`. Later `measure` (`table.rs:210–222`) calls `natural_height(body.into_node(), …)` on the stack. No re-check.
2. **List item.** `List::item` / `item_marked` → `ListItem::new` / `custom` → `ItemBody::new` → `check_content(ListItem)`. `ListNode::new` (`list.rs:159–161`) stores `item.body.into_node()` and `split`s that stack (`list.rs:188`). No re-check.
3. **Column.** `Columns::col` collects kids with no content check. `Columns::lower` → `ColumnsNode::new` checks each column as `Column`, wraps `Stack`. Nested `Columns` are checked again as `Column` when the inner `ColumnsNode::new` runs (the outer `Columns` arm does not re-walk). `split` (`columns.rs:130`) does not re-check.
4. **Band.** Hook runs per page (`decorate.rs:145` → `band_content` / test hooks). `measure_band` checks `Band`, then splits against a frame whose height is the **page** height (`decorate.rs:258–263`), then rejects `Fit` taller than the margin (`:266–267`). Failure happens before `paint_pages`, so reject tests see an empty canvas.
5. **Root / keep-together at root.** No `check_content`. `keep_together` on the body can contain anything the closure pushes, including `page_break`.

### Recursion that matters for a leaf separator

A separator is a leaf (no children in §2.3). It is seen only when some walk visits that node with a `Container`.

Walks that **will** see it:

- Direct kid of a cell / item / column vec (the `new` checks).
- Any `Stack` ancestor: `Stack` always re-enters `check_content` with the **same** container (`lower.rs:34–36`). `Stack([PageBreak])` in a cell reports `"page break"` (`table.rs:646–655`). A `Stack([Separator])` in a band would hit the separator arm with `Band`.
- `KeepTogether(Open)` **only when the container is already `Column`** (`lower.rs:39–40`). Kids are checked as `Column`.
- `Columns` **only when the container is `Band`** (`lower.rs:46–48`). `column_stacks()` yields `Stack`s, so the next step is the `Stack` arm, then the real kids, still with `Band`. Nested columns repeat this (`Columns`+`Band` again). `band_columns_still_reject_a_nested_table` is this path for tables.

Walks that **will not** open a separator sitting inside them:

- `Columns` when container is `Column` (`lower.rs:45` is `true` with no call). Safety depends on `ColumnsNode::new` having already checked those kids as `Column`. A later **band** placement does **not** trust that: the `Band` arm re-walks. So a column-legal separator becomes illegal if that `Columns` node is returned from a band hook.
- `List` anywhere (`lower.rs:53`). Item bodies were checked as `ListItem` at `ItemBody::new`. Placing the list in a cell or column does not re-apply `TableCell` / `Column`. Placing the list in a band rejects the **list** (`"list"`), not the inner separator.
- `Table` anywhere (`lower.rs:52`). Cell bodies were checked as `TableCell`. A table in a band is rejected as `"table"` without opening cells.
- `KeepTogether` when container is not `Column`. The node is denied (`container == Column` is false) and kids are not read. Error node is `"keep together"` even if a kid would also be illegal.
- `KeepTogether(Prepared)` even in a column. Prepared groups are fragments (`flow.rs:126–136`), produced during split from an `Open` group (`flow.rs:208–216`). Content was checked only if that `Open` group passed through a `Column` check.

### What "legal where the proposal says" requires

Predicate that matches §2.6 / §2.8 / §2.5 / §2.10 given the four containers:

```text
separator allowed  <=>  container != Band
                    <=>  Column | TableCell | ListItem
```

That is the **same boolean as `FlowNode::List`** (`lower.rs:53`), and **not** the same as:

- `Text|Paragraph|Spacer` (`true`) — would allow bands
- `Image` (`Column | TableCell`) — would **reject list items**, contradicting §2.8 "线"
- `PageBreak` (`false`) — would reject cells, items, and columns
- `Table` (`== Column`) — would reject cells and list items

Exhaustive `FlowNode` matches that **must** gain an arm (crate will not compile without them):

1. `check_content` (`lower.rs:32–56`). New arm: allow iff `container != Container::Band`. Do not attach it to the `Spacer` or-pattern. Recursion is unnecessary on the leaf itself. Do not add a recurse into `List` / `Table` just for this; those parents are already denied in `Band`, and their children were checked at the inner host.
2. `node_name` (`lower.rs:67–78`). New string, distinct from `"spacer"` / `"paragraph"`, so band/item/cell errors can name the node. No name is specified in the proposal.
3. `FlowNode::split` (`flow.rs:155–166`). Otherwise the node cannot be placed in the root, in a cell (`natural_height` → `split`), in an item, or in a column. Pagination does not need a new arm (`paginate.rs:142–147` wildcard).

DSL so the three hosts can actually emit it:

4. A method on `Flow` (`document.rs:575–663` area). That single method covers **root** (implicit stack, §2.4 `:289`), **columns** (`Columns::col`), **list items** (`List::item` / `item_marked`), and **keep-together groups** (legal at root always; legal inside columns because `Column` re-checks `Open` kids). `Flow`'s doc (`:506`) already lists those callers.
5. Public cells: new `CellContent` variant plus an arm in `Table::lower` (`document.rs:751–755`), and a constructor beside `Cell::new` / `Cell::image`. `check_content` alone does not make `t.row([...])` able to hold a line. Layout `table::Cell::new(vec![FlowNode::…])` will accept it once arm (1) allows `TableCell`.

Must **not** change:

- `Band::left` / `Band::right` / `Band::lower` must not emit the node. `measure_band` still has to reject it if a hook or test injects `FlowNode::…` directly (`band_rejects_*` style).
- `Layer::line` (`document.rs:1066`) stays the page-coordinate stroke. §2.3 forbids using the behind/front path as the flow rule.
- Existing deny arms for columns/pageBreak/keep-together/table in cells, and table/pageBreak/image in items, and image/table in bands. Adding a separator arm must not widen those.

### Smuggle map (separator must not become legal here)

| Injection | What runs | Result if separator arm is `!= Band` |
| --- | --- | --- |
| Direct child of band hook `Vec` | `measure_band` → separator arm with `Band` | reject, container `"header/footer band"` |
| `Stack` inside a band (bands allow stacks) | `Stack` re-enters with `Band` | reject, same container |
| `Columns` inside a band, separator in a column stack | `Columns`+`Band` → `column_stacks` → `Stack` → separator with `Band` | reject as separator, **not** as `"columns"`. Same container string as `band_columns_still_reject_a_nested_table` |
| Nested `Columns` inside those column stacks | `Columns`+`Band` repeats | still `Band` when the leaf is visited |
| Separator inside `KeepTogether` inside band columns | `KeepTogether` arm, container is `Band`, kids **not** opened | reject `"keep together"` (separator not named; group never accepted) |
| Separator inside `List` / `Table` used as band content | `List` / `Table` denied in `Band`, children not opened | reject `"list"` or `"table"` |
| `col(|c| c.keep_together(|g| g.<separator>()))` | `ColumnsNode::new` checks `KeepTogether` as `Column`, opens `Open` kids as `Column` | **allow** (§2.5 lists both 线 and keep_together) |
| `item(|i| i.keep_together(|g| g.<separator>()))` | `ItemBody` sees `KeepTogether`, not `Column` | reject `"keep together"`; separator not stored |
| `item` that contains a nested list whose item has a separator | inner `ItemBody` checks `ListItem` (allow); outer item sees `List` (allow, no re-walk) | **allow** (§2.8 nested lists and 线) |
| cell contains a list item that contains a separator | cell sees `List` (allow, no re-walk); item already checked as `ListItem` | **allow** (§2.6 分隔线 and 格内列表; §2.8 线) |
| column contains a table cell that contains a separator | column sees `Table` (allow); cell checked as `TableCell` at `Cell::new` | **allow** (§2.5 表; §2.6 分隔线) |
| cell contains `Columns` that contains a separator | cell denies `Columns` without opening kids | reject `"columns"`; separator never legal in the cell |
| root `Flow` separator | no `check_content` | **allow** (§2.3 body node, §2.4 implicit stack) |

The hole that makes bands accept a separator is specifically: leaf arm returns `true` for `Band`, **or** someone later allows `List`/`Table`/`KeepTogether` in `Band` without re-walking children that were checked under a looser container. Today those three are denied in `Band`, and `Columns`/`Stack` **do** re-walk. A correct separator arm plus the current recurse rules is enough. Do not "fix" smuggling by skipping `check_content` on `column_stacks` in the `Band` arm (`lower.rs:46–48`); that recurse is what restamps column-legal children as band content.

`Columns`+`Column` not recursing is safe only because every `ColumnsNode` is born through `new`, which checks kids as `Column`. The struct fields are private (`columns.rs:96–100`). The split-tail rebuild (`columns.rs:169–173`) copies already-checked stacks; it does not insert new node kinds.

## Files Read

- `src/layout/lower.rs` (entire, 110 lines)
- `src/layout/flow.rs` `KeepTogetherNode`, `FlowNode`, `split`, `natural_height` (`:121–167`, `:333–341`)
- `src/layout/columns.rs` `ColumnsNode::new`, `column_stacks`, split tail (`:96–173`); keep-together column test (`:422–435`)
- `src/layout/table.rs` `Cell` / `Cell::new` (`:50–73`); cell measure (`:210–222`); `cell_rejects_*` (`:626–657`)
- `src/layout/list.rs` `ListItem` (`:99–123`); `into_node` at list build (`:159–161`); nested list (`:353`); `item_rejects_*` (`:496–524`)
- `src/layout/decorate.rs` `measure_bands` (`:101–166`); `measure_band` (`:248–275`); `band_rejects_images` (`:452–485`); `band_rejects_tables` (`:488–512`); `band_columns_still_reject_a_nested_table` (`:553–580`); `band_columns_share_one_line` (`:522–550`)
- `src/layout/paginate.rs` `paginate_sections` (`:93–117`); `plan_pages` page-break match (`:126–147`)
- `src/layout/image.rs` `column_offsets_the_image` (`:235–250`)
- `src/document.rs` `Section::lower` (`:141–167`); `Flow` (`:506–663`); `Columns` (`:679–704`); `Table::lower` (`:738–770`); `Cell` / `CellContent` (`:773–841`); `List::item` / `item_marked` (`:865–880`); `Band` / `Band::lower` / `band_content` (`:936–1013`); `Layer::line` (`:1064–1068`)
- `src/error.rs` `InvalidCellContent` (`:88–93`, `:165–166`)
- `tests/document.rs` `nesting_rules_come_from_layout` (`:378–394`); `list_item_and_overwide_image_fail` (`:593–607`)
- `docs/提案书.md` §2.3 (`:270–285`), §2.4 (`:287–300`), §2.5 (`:314–325`), §2.6 cell table (`:377–382`), §2.8 item row (`:447–454`), §2.9 (`:456–458`), §2.10 (`:486–499`), DSL name (`:661`)

## Boundaries

- Did not design separator geometry, stroke, `space_before` / `space_after`, or `Fragment` drawing. Those are outside this slice. `split` must exist or the new variant does not compile; behavior of that `split` was not specified here beyond "unsplittable block" from §2.3.
- Did not audit font/measure, line breaking, or krilla emit.
- `src/layout/dump.rs` is a stub comment only; no `FlowNode` match.
- Backend and canvas have no `FlowNode` match. A separator becomes drawing only after `split` emits fragment commands.
- Public `document::Cell` allow-list is narrower than `Container::TableCell` (text and image only). Layout allows list/spacer/stack/paragraph in cells with no success test found for list or spacer.
- `item_rejects_*` does not pin columns or keep-together; the matrix still denies them (`lower.rs:44–51` and `:38–42`).
- Band rejection is decorate-time (`measure_bands` before paint). Cell, item, and column rejection is builder-time (`CellBody::new` / `ItemBody::new` / `ColumnsNode::new`).
- Root flow is unchecked. Forbidding a separator on the body would be a new policy, not the current matrix.
- `Layer::line` is a different feature (this commit's behind/front paths). Not a host in `check_content`.

## Non-Obvious Things

- `InvalidCellContent` is the band error too. Tests and `Display` say `"header/footer band"`, not a distinct error variant.
- Illegal nodes inside `Stack` report the **descendant** name. Illegal `Columns` / `List` / `Table` / `KeepTogether` report **themselves** and hide descendants. A separator wrapped only in `Stack` is visible to the outer container; a separator wrapped in `KeepTogether` is visible only when the outer container is `Column`.
- Band + columns rewrites the container from `Column` (used at `ColumnsNode::new`) to `Band` (used at `measure_band`). Column-legal content is not grandfathered. Table-in-band-column is the existing proof (`decorate.rs:553–577`).
- `column_stacks` returns `FlowNode::Stack`, not the raw kid vec (`columns.rs:117`, `:126–128`). The band walk hits the `Stack` arm one level in, then the leaf.
- `PageBreak` is denied in all four containers but legal on the document `Flow`, because the root never calls `check_content`. `tests/document.rs:378–394` only fails when the break is inside a column.
- Grouping the new variant with `Spacer` is the bug that legalizes bands. Grouping it with `Image` is the bug that bans it in list items. Grouping it with `PageBreak` bans it in all nested hosts. The `List` boolean matches the proposal; sharing the condition is behaviorally identical **for these four containers**, but `node_name` still needs its own arm, and a future fifth `Container` would couple list and separator if they share one pattern.
- `KeepTogether(Prepared)` skips the child walk even in columns. Not a separator smuggle path if `Prepared` is only created from an already-checked `Open` group during split (`flow.rs:208–216`).
- Public `Band::lower` can only emit `Text` and `Columns` of text/empty inset tracks (`document.rs:964–998`). It cannot smuggle a separator unless someone adds an emit. The `Band` arm is what stops test hooks and any future emit. Header and footer are the same `Container::Band`.
- First error in a `Flow` sticks (`document.rs:541–544`); `check_content` likewise returns on the first illegal node in preorder. A stack of `[Separator, PageBreak]` in a cell would report whichever the separator arm decides first: if separator is allowed, the error is still `"page break"`.

## Open Questions

- Public method name (`hline` vs `rule`) is explicitly unpinned (`docs/提案书.md:661`). `node_name`'s `"…"` string is also unspecified. Tests that snapshot `InvalidCellContent.node` will need that string.
- §2.6 says cell DSL signatures are not pinned (`:384`). Whether a cell gets `Cell::rule(...)` or a mini `Flow` closure is not decided by the matrix. The matrix only needs `TableCell` to allow the `FlowNode`. The public `CellContent` match (`document.rs:751–755`) cannot express it today.
- §2.8 does not mention keep-together or images inside items. Code denies both (`KeepTogether` not `Column`; `Image` not in the image allow-set). `item_rejects_*` pins image only. A separator inside `item.keep_together(...)` stays illegal because the group is illegal, not because the line is illegal.
- No success test that a `List` or `Spacer` inside `table::Cell::new` is accepted. The allow arms say they are. A separator test should follow `Cell::new` / `ListItem::new` / `ColumnsNode::new` for allows, and `measure_band` (direct, inside `Stack`, inside `Columns`) for the band deny, or existing `cell_rejects_*` / `item_rejects_*` / `band_rejects_*` will not cover the new variant.
- Whether `check_content` should grow a fifth container later is out of scope. An explicit `matches!(Column | TableCell | ListItem)` does not silently allow a new container; `!= Band` would.
