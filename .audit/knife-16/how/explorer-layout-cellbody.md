### Components Found

Two `Cell` types. Callers see `document::Cell` (`src/document.rs` 1030–1111, re-exported `src/lib.rs` 21–24). Layout sees `table::Cell` (`src/layout/table.rs` 50–73), whose body is `CellBody`.

**Public payload is one of three arms (`src/document.rs` 1039–1076).**

| `CellContent` | Constructor | Lowered node (`Table::lower` 1007–1013) |
| --- | --- | --- |
| `Text(String)` | `Cell::new` (1048–1054); also `From<&str\|String\|&String>` (1095–1111) | `vec![flow.text(text)]` → `FlowNode::Text` via private `Flow::text` (665–667) |
| `Image(Image, ImageBox)` | `Cell::image` (1057–1065) | `vec![image_box.lower(...)]` → `FlowNode::Image` |
| `HLine(HLine)` | `Cell::hline` (1068–1076) | `vec![line.lower()]` → `FlowNode::HLine` (`HLine::lower` 559–568) |

`col_span` / `row_span` (1079–1091) do not add children. There is no `Cell::spacer`, no `CellContent::Spacer`, and no `FnOnce(&mut Flow)` on a cell. `Table::header` / `Table::row` (971–987) only collect `Into<Cell>`. Each match arm builds a **one-element** `Vec`. `Flow::spacer` (726–728) pushes `FlowNode::Spacer(SpacerNode { height })` onto a `Flow`, which a cell closure never receives.

**Allow matrix already says yes (`src/layout/lower.rs` 29–97).** `Cell::new` (table.rs 58–63) calls `CellBody::new` → `check_content(Container::TableCell, &nodes)` (lower.rs 90–92). The true arm (33–36) is `Text | Paragraph | Spacer | HLine`. `Stack` recurses with the same container (37–39), so a spacer nested in stacks is also legal. Rejected in a cell: `Columns`, `PageBreak`, `KeepTogether` (allowed only when `container == Column`, 41–45), `Table` (55). `List` is allowed (`container != Band`, 56). `Image` is allowed (57). A rejected spacer would be named `"spacer"` (73) inside `Error::InvalidCellContent` (`"{node} is not allowed in a {container}"`, `src/error.rs` 168–169). No such rejection exists for `TableCell`. `CellBody::into_node` (95–97) is always `FlowNode::Stack(self.0)`, including when the vec has one spacer.

**`SpacerNode` (`src/layout/flow.rs` 127–209).** Field is `height: Length<NonNegative>`. `FlowNode::split` dispatches with no `LayoutCtx` (167). `split` (179–209):

- `h == 0.0` → `Split::Fit { height: 0.0, fragment: Fragment::empty() }` (182–186). Empty fragment is height 0, `spacing_after` 0, `commands` empty (`Fragment::empty` 55–60).
- Otherwise `take = h.min(avail.height)`, `rest = h - take` (188–189). Head fragment: `height: take`, `spacing_after: 0.0`, `commands: Vec::new()` (190–194). No glyph, stroke, or fill.
- `rest <= 0.0` → `Fit { height: take, fragment: head }` (195–199).
- else → `Partial { head_height: take, head, tail: FlowNode::Spacer(SpacerNode { height: NonNegative::pt(rest)? }) }` (201–207).
- Never `MoveToNextPage`. Never reads `avail.width` or `avail.full_height`. `Frame::validate` allows `height == 0` (flow.rs 36–37), so a positive spacer against a used-up frame is `Partial` with `take == 0`.

**Cell measure forces `Fit` (`src/layout/table.rs` 191–232, `src/layout/flow.rs` 341–350).** `measure` builds one frame per cell: `width = inner_w`, `height = full_height = inner_full`, where `inset = border/2 + padding` and `inner_full = avail.full_height - 2*inset` (201–221). `natural_height(body.into_node(), …)` keeps only `Split::Fit`. `Partial` and `MoveToNextPage` become `Error::UnsplittableTallerThanFrame` (347–349). That error aborts `TableNode::split` (182–187) before `split_measured`. Row boxes are placed later as atomic fragments (340–390): a block that does not fit **remaining** `avail.height` stays whole on the tail (`MoveToNextPage` if nothing of this table was placed, else `Partial`).

**Stack walker the cell actually runs (`src/layout/flow.rs` 252–339).** Because `into_node` wraps every cell, both the only-child and the middle-child cases go through `split_stack`, then the child's `split`. On `Fit`, `y += height`, commands are offset by `(0, y)`, and `avail.height` shrinks (272–288). Spacer `spacing_after` is 0, so it never inserts paragraph gap. On `Partial`, already-placed siblings plus the spacer head are returned as a `Stack` tail `[tail, …remaining kids]` (290–307). `natural_height` drops that `Partial`.

**Proposal already decided.** §2.6 (`docs/提案书.md` 368–381): a physical row is not split through the middle; a span group that does not fit moves whole; a group taller than the page body errors. Cell children may include 段落、分隔线、spacer; columns, pageBreak, keep_together, and nested table may not. §2.13 (545–553): spacer is vertical blank, non-negative height (0 allowed), distinct from paragraph spacing, **splits** in the flow (remainder is a shorter spacer, and may be the only content at the top of the next page), and is **allowed in a cell**.

### Flow

Public path today, one node per cell:

1. `doc.table` (`document.rs` 706–714) builds `Table` and `lower`s it with the enclosing `Flow` (994–1027).
2. The match emits one `FlowNode`. `table::Cell::new` runs `check_content`. `TableNode::new` (124–179) stores that `CellBody` on a `PlacedCell`.
3. First `TableNode::split` (182–187) calls `measure` with the paginator's frame. Public tables spread `TableSpec::new` (1020–1025), whose padding and border width are `NonNegative::ZERO` (`table.rs` 35–46). Public `inset` is 0, so `inner_full` is the page body height (`avail.full_height`). `measure` uses `full_height` for the cell frame and the too-tall checks (203, 217–221, 278–287). It does not use remaining `avail.height`.
4. `into_node` → `Stack` → `split_stack` → `SpacerNode::split` if a spacer is present. Only a full `Fit` survives `natural_height`. `cell_h = content_height + 2*inset` (223). A one-row cell updates `row_h[row]` with `max` (224–226). A rowspan cell does not; `row_offsets` (297–305) adds any shortfall to the **last** row of the span.
5. The content fragment is offset to `(x + inset, y_in_group + inset)` (256–259). A spacer fragment has no commands, so the offset adds nothing to paint. The row fragment's **height** is the row box (`row_y[end] - row_y[start]`, 241). That is what grows.
6. `split_measured` (340–390) walks those row fragments against remaining page height. It does not call `SpacerNode::split` again.

**Only child.** `CellBody(vec![Spacer(h)])` becomes `Stack([Spacer(h)])`. `split_stack` runs one iteration. `avail.height` is still `inner_full`.

| `h` vs `inner_full` | `SpacerNode::split` | What the cell does |
| --- | --- | --- |
| `0` | `Fit` height 0, empty fragment (182–186) | Stack height 0. Public inset 0 → row height 0. No commands. |
| `0 < h <= inner_full` | `Fit` height `h`, empty commands (195–199) | `natural_height` returns `(h, empty fragment)`. Single-row `row_h = h + 2*inset`. With public inset 0, the row is `h` tall and paints nothing. Later rows start after that box. |
| `h == inner_full` | `rest == 0` → `Fit` | `cell_h == avail.full_height`. The check is `need > avail.full_height` (281), so equality passes. The row fills one body exactly. |
| `h > inner_full` | `Partial`: head takes all of `inner_full` (empty commands), tail is `Spacer(rest)` (201–207). `split_stack` wraps that as `Stack([Spacer(rest)])` (297–306) | `natural_height` returns `UnsplittableTallerThanFrame` (349). `split_measured` never runs. The remainder is not a next-page spacer. |

Body flow is the opposite for the last row. `paginate_root` gives the spacer the live remaining height (`paginate.rs` 149–153, 170–185) and keeps `Partial`. `spacer_remainder_leads_next_page` (525–554): 80 pt spacer on a 50 pt page, page 0 has no ops, the following paragraph's baseline on page 1 is `30 + metrics.baseline`. That test builds a root `Stack`, not a cell. The same 80-on-50 numbers inside a cell hit the `h > inner_full` row and error.

If the only-child spacer **fits the full body** but the row does not fit **space left on this page**, measure has already succeeded. `split_measured` then moves the whole row (`y + block.height > avail.height`, 359–361). Nothing placed yet → `MoveToNextPage` of the measured table (380–383). Earlier rows placed → `Partial` (386–389). Paginator: `MoveToNextPage` at cursor `y == 0` is `UnsplittableTallerThanFrame` (`paginate.rs` 187–189); at `y != 0` the table retries on a fresh page. A row that passed measure is `<= full_height`, so this top-of-page error is the header-plus-row case already tested (`header_plus_group_taller_than_page_errors_with_no_output`, table.rs 534–549), not a fitted spacer row sitting at `y == 0`.

**Middle child.** Layout can hold it: `table::Cell::new(vec![node_a, FlowNode::Spacer(...), node_b])` passes `check_content` (spacer is on the true arm; the vec is not itself a `Stack` node). `into_node` makes one `Stack`. No public constructor builds that vec.

Assume public inset 0 so `inner_full` is the body height, and the frame starts with `height == full_height == inner_full`.

- Spacer height 0 in the middle: `Fit` of 0. `y` does not move. Following siblings are not pushed down. Row height is unchanged by that child.
- Preceding siblings `Fit`, and `h <=` remaining `avail.height`: spacer `Fit`, `y += h`, commands stay empty. The next sibling is split against the shrunk frame and its fragment is offset by that `y` (`with_offset(0, y)`, 277). Glyphs or strokes of the next sibling start `h` points lower inside the cell. Row height includes the gap. The row is still one fragment in `split_measured`.
- Preceding siblings fit, but `h >` remaining `avail.height` (even when `h <= inner_full`): spacer returns `Partial` with `take = remaining`, `rest = h - remaining`. `split_stack` stops. Later siblings are not measured; they sit on the tail `Stack([Spacer(rest), …])` (297–306). `natural_height` errors. The head (earlier siblings plus an empty spacer slice) is discarded. The cell does not continue on the next page.
- Spacer fits, a **later** sibling does not fit the leftover frame: that sibling's `Partial` or `MoveToNextPage` also becomes `UnsplittableTallerThanFrame`. A legal middle spacer can still fail the table because the stack must `Fit` as a whole.
- `split_stack` does not special-case a zero-height `Partial`. List items do (`list.rs` 190–194: `Partial` is kept only when `head_height > 0`). A cell stack does not.

**Same-row neighbors.** Content is top-aligned at `y = row_y[cell.row] + inset` (257). A spacer cell of 40 pt beside a text cell of one line makes `row_h` the max (225). The text stays at the top of the row. The extra vertical space is empty row box under the text, not a second copy of the spacer and not vertical centering. The border rect, when width > 0, uses the full row box (260–266). Public border width is 0, so `Positive::pt(border)` fails and no stroke is pushed (236).

### Files Read

- `src/layout/lower.rs` — `check_content`, `Container::TableCell`, `CellBody`, `node_name`.
- `src/layout/flow.rs` — `SpacerNode`, `Split`, `split_stack`, `natural_height`, `Fragment::empty`.
- `src/layout/table.rs` — `Cell`, `TableSpec::new`, `measure`, `row_offsets`, `split_measured`, `cell_rejects_…`, `cell_allows_hline`.
- `src/layout/paginate.rs` — body `split` loop (147–196) and `spacer_remainder_leads_next_page` (525–554).
- `src/layout/list.rs` — item body uses live `split`, not `natural_height` (172–225). Contrast only.
- `src/layout/hline.rs` — `HLineNode::split` (43–76): unbreakable, `MoveToNextPage` or `UnsplittableTallerThanFrame`. Contrast only.
- `src/document.rs` — `Flow::spacer`, `Flow::text`, `Table::lower`, `Cell` / `CellContent`.
- `src/error.rs` — `InvalidCellContent` display (168–169).
- `src/geom.rs` — `Length` `Eq` (26–38), `NonNegative::pt` / `ZERO` (102–112).
- `src/lib.rs` — `Cell` re-export.
- `docs/提案书.md` §2.6 (368–381), §2.13 (545–553).

### Boundaries

**`FlowNode::Spacer` inside a cell is legal at layout and unreachable from the DSL.** `check_content` allows it at the cell's top level and inside nested `Stack`s. `table::Cell::new(vec![FlowNode::Spacer(...)])` would succeed the allowlist. Nothing in `src/layout/table.rs` tests constructs one. `cell_allows_hline` (659–670) is the positive test for the sibling arm. `cell_rejects_columns_page_break_keep_together_and_tables` (625–657) lists columns, page break, keep-together, nested table, and `Stack([PageBreak])`. Spacer is absent from that list; absence is not an assertion.

**What is missing for a caller.** Three public holes, all in `src/document.rs`:

1. `CellContent` has no spacer arm (1040–1044).
2. `Cell` has no `spacer` method (1046–1092). `Flow::spacer` (726–728) is the only builder, and it needs a `Flow`.
3. `Table::lower`'s match (1007–1013) has no arm that emits `FlowNode::Spacer`.

A fourth hole is separate: each arm is `vec![one node]`. `Cell::spacer(height)` in the shape of `Cell::hline` would make the spacer the **only** child. A spacer between other cell children needs a multi-node body. `table::Cell::new` already accepts `Vec<FlowNode>`. The public `Cell` stores one `CellContent`. List items are the opposite: `List::item` (1136–1138) passes a nested `Flow`, and `ItemBody` uses the same allow arm, then `ListNode::split` actually paginates that stack (list.rs 188–194).

**No layout type is missing for a sole spacer.** `SpacerNode`, the allow arm, `CellBody`, `natural_height`, and row placement already implement "blank of height `h` inside the row, or error if the cell stack does not fit the inner page." `Length<NonNegative>` implements `Eq` (geom.rs 26–38), and `Cell` / `CellContent` derive `Eq` (1031, 1039), so a height arm matches the existing derives. Negative height fails at `NonNegative::pt` with `Error::NegativeLength` (geom.rs 107–112) before a node exists.

**Cross-page, as the code stands.**

- Flow spacer: splits. Remainder is a shorter `SpacerNode` (`flow.rs` 201–207, `paginate.rs` 525–554).
- Cell spacer: does not split. `natural_height` turns that `Partial` into `UnsplittableTallerThanFrame`.
- A cell spacer that fits `inner_full` is baked into the row height. The row then follows ordinary table pagination: whole row to the next page if the remainder of this page is shorter than the row (`table.rs` 359–389).
- A cell spacer taller than `inner_full` fails the table at measure. It does not become the first content of the next page.
- A middle spacer that fits, inside a stack that also fits, is a command-less gap. A middle spacer that does not fit the **leftover** inner height fails the table even when `h` itself is less than one page.

### Non-Obvious Things

- The cell never calls `SpacerNode::split` except under `natural_height`, and the frame passed in has `height == full_height == inner_full` (table.rs 217–222). The spacer's `Partial` path is live code and dead for any cell whose stack fits. It becomes the error path as soon as `h` exceeds the space `split_stack` has left, which for an only child is the whole inner page.
- `HLineNode::split` returns `Err(UnsplittableTallerThanFrame)` itself when `height > avail.full_height` (hline.rs 52–54), and `MoveToNextPage` when it fits a fresh page but not the remaining height (55–56). `SpacerNode::split` returns neither. Inside a cell both still die in `natural_height`, because the cell frame sets `height == full_height`, and a later sibling that does not fit also yields `Partial` or `MoveToNextPage`. The observable table error matches. The mechanism does not: the spacer is willing to slice itself; the cell refuses the slice.
- `split_stack` on a spilling middle spacer builds a real tail (`Stack([Spacer(rest), later…])`, flow.rs 297–306) and a real head that already contains the earlier siblings. `natural_height` throws both away (349). There is no second pass that keeps the head as a partial cell.
- Public inset is 0, so a sole spacer's row height equals `h`. Layout still adds `2 * inset` (table.rs 201–223). A non-zero padding or border, only available on `TableSpec` inside the layout crate, shrinks `inner_full` and adds that inset on top of `h`. A spacer of `inner_full + epsilon` errors; the same `h` with inset 0 would fit.
- Spacer ignores column width, but `measure` still requires `inner_w > 0` (212–216) before `natural_height`. A zero content width fails as `Error::ColumnWidths { reason: "content-width" }` even for a spacer-only cell.
- `KeepTogether` around a spacer is rejected as `"keep together"` without inspecting the spacer (lower.rs 41–45). The column arm is the one that recurses into `KeepTogetherNode::Open`.
- Paragraph `blank` / `blank_em` / `blank_to_end` (`document.rs` 809–823) are inline horizontal gaps on `Paragraph`. `HLine::space_before` / `space_after` (539–547) are padding inside an unbreakable rule. Neither is `FlowNode::Spacer`.
- Zero-height spacer and an empty `CellBody` both measure as height 0 (`split_stack` 257–261; spacer 182–186). Public `lower` never passes an empty vec. A zero spacer between two nodes does not separate them.
- Rowspan: a spacer cell with `rows > 1` does not write `row_h` in the first loop (224–226). `row_offsets` (297–305) dumps `cell.height - sum(existing)` onto the last row of the span, and only when the spacer cell is taller than the rows it covers.

### Open Questions

- §2.13 says a spacer splits. §2.6 says a row does not split through the middle and a cell may contain a spacer. The code follows the row rule: cell content must `Fit` in `inner_full`, then the row moves whole. Whether a cell spacer that is taller than the body should instead slice like `Flow::spacer` is a product choice the implementation has already answered as "no", and no test locks it.
- A sole `Cell::spacer` does not give callers a spacer **between** other cell children. The layout vec can. The public enum cannot. Which of those the knife is supposed to open is not in the layout code.
- No test passes `FlowNode::Spacer` to `table::Cell::new` or `CellBody::new`, and no test measures a stack whose middle child is a spacer. The height tables above are from reading `split_stack` + `natural_height` + `measure`, not from an executed fixture.
