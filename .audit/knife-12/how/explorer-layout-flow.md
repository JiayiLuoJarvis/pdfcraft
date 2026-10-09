# Layout flow pipeline (hline seam map)

Baseline: `feat/hline` @ `7cb1654` (`feat(document): draw line, rect, and ellipse on behind/front layers`). Repo `/mnt/data/demo/quire`.

Scope: how `FlowNode` measures, splits, and paints today, and which existing seams a block-level unbreakable horizontal `Stroke` would have to pass through. No new API. Proposal facts used only as constraints already written in `docs/提案书.md` §2.3 / §2.1 / §2.5 / §2.5.1 / §2.6 / §2.8 / §2.13.

There is no `hline` / `rule` / `RuleNode` type in `src/` at this commit. `FragmentCommand::Stroke` already exists and is produced by table borders and by behind/front shapes.

---

## Components Found

### Protocol (`src/layout/flow.rs`)

Not a public trait. File header: consume-self `split` and fragment `draw`; callers never implement the protocol (`flow.rs:1-3`). `layout/mod.rs:1-13` mounts `flow` next to `columns`, `image`, `list`, `lower`, `paginate`, `paragraph`, `table`. `dump.rs` is a one-line stub (`dump.rs:1`).

| Type | Role | Lines |
| --- | --- | --- |
| `LayoutCtx` | Only `&mut FontStore`. Paragraph/text/list markers/keep-together/table/columns/list need it. Image and spacer do not. | `flow.rs:17-20` |
| `Frame` | `width`, `height` (remaining, may be 0), `full_height` (page body, must be > 0). | `flow.rs:22-43` |
| `Frame::validate` | `width` finite and > 0; `height` finite and >= 0; `full_height` finite and > 0. Else `Error::InvalidGeometry`. | `flow.rs:31-42` |
| `Fragment` | `height`, `spacing_after`, `commands`. Origin-relative. | `flow.rs:45-51` |
| `Fragment::empty` | All zeros, no commands. | `flow.rs:54-60` |
| `Fragment::with_offset` | Shifts every command. Stroke/Fill: `path.translated(dx, dy)`. | `flow.rs:62-76`, Stroke arm `69-70` |
| `FragmentCommand` | `Glyphs`, `Fill { path, paint }`, `Stroke { path, paint, stroke }`, `Image { image, rect }`. | `flow.rs:80-95`, Stroke `86-90` |
| `PlacedFragment` | Page-space `origin` + `fragment`. | `flow.rs:98-102` |
| `Split` | `Fit { height, fragment }`, `Partial { head_height, head, tail }`, `MoveToNextPage(FlowNode)`. | `flow.rs:106-118` |
| `FlowNode` | `Paragraph`, `Text`, `Spacer`, `PageBreak`, `KeepTogether`, `Stack`, `Columns`, `Table`, `List`, `Image`. No rule variant. | `flow.rs:139-150` |
| `FlowNode::split` | Validates the frame, then dispatches. `PageBreak` is `Error::InvalidGeometry` if it reaches here. | `flow.rs:152-167` |
| `draw_fragment` | Adds `origin` to each command and calls `Canvas`. Stroke → `canvas.stroke_path`. | `flow.rs:344-373`, Stroke arm `362-365` |

`spacing_after` on `Fragment` is the paragraph-gap channel (see Stack and paginate). It is not a second copy of a node's own vertical padding. Nodes that have no gap (`Image`, table row fragments, spacer heads) store `0.0`.

### `Split` outcomes

- `Fit`: node is fully consumed. `height` is what the cursor advances. `fragment.spacing_after` may still be applied after that, and can be dropped.
- `Partial`: `head` is placed now; `tail` is a new `FlowNode` for later frames. Head fragments built by stack/list/table set `spacing_after: 0.0`.
- `MoveToNextPage`: nothing from this node is placed. The same node (or a prepared form) is retried on the next frame.

### Unbreakable siblings

**`ImageNode`** (`src/layout/image.rs:11-61`). Size is fixed at `ImageNode::new` (`image.rs:19-27`). `split(self, avail)` does not take `LayoutCtx`.

1. `width > avail.width` → `Error::UnbreakableWiderThanFrame { width, avail }` (`image.rs:32-37`). No shrink.
2. `height > avail.full_height` → `Error::UnsplittableTallerThanFrame` (`image.rs:38-40`).
3. `height > avail.height` → `Split::MoveToNextPage(FlowNode::Image(self))` (`image.rs:41-43`).
4. Else `Split::Fit` with `spacing_after: 0.0` and one `FragmentCommand::Image` whose rect x comes from `TextAlign` (`image.rs:44-59`):

```text
Start  → 0
Center → (avail.width - width) / 2
End    → avail.width - width
```

Same formula as paragraph `align_x` (`paragraph.rs:260-266`). Image tests: end placement `image.rs:127-160`; wider-before-taller and short-frame move `image.rs:192-231`; column offset of the rect `image.rs:235-271`.

**`KeepTogetherNode`** (`flow.rs:126-136`, `203-241`). Two phases:

- `Open(Vec<FlowNode>)`: build a full frame `{ width: avail.width, height: full_height, full_height }` and `natural_height(FlowNode::Stack(kids), …)` (`flow.rs:210-223`). Any `Partial` or `MoveToNextPage` from that call becomes `Error::UnsplittableTallerThanFrame` (`flow.rs:332-340`).
- Stores `PreparedGroup { height, spacing_after, fragment }`. During prepare, `fragment.spacing_after` is stripped to `0` and kept on the group (`flow.rs:217-218`).
- `Prepared`: if `height <= avail.height`, `Fit` and put `spacing_after` back on the fragment (`flow.rs:227-233`). Else if `height <= avail.full_height`, `MoveToNextPage(KeepTogether(Prepared))` so the next page does not remeasure (`flow.rs:234-237`). Else `UnsplittableTallerThanFrame` (`flow.rs:238-239`).

Natural height includes gaps between children (stack adds them into `y`) and excludes the group's outer trailing spacing (stripped above). Matches proposal §2.4 as implemented.

**`ParagraphNode`** is splittable by line, but one line is atomic: `line_height > full_height` errors (`paragraph.rs:105-107`); `avail.height < line_height` is `MoveToNextPage` (`paragraph.rs:108-110`); otherwise `Partial` of whole lines (`paragraph.rs:123-137`). Already-wrapped `self.width > avail.width` is `InvalidGeometry`, not `UnbreakableWiderThanFrame` (`paragraph.rs:95-98`). `TextNode::split` wraps once at `avail.width` then delegates (`paragraph.rs:37-39`).

### Splittable contrast: `SpacerNode`

`flow.rs:121-123`, `170-200`. No commands. Height is `Length<NonNegative>` (0 allowed).

- `h == 0` → `Fit` empty fragment (`flow.rs:173-178`).
- Else `take = min(h, avail.height)`, `rest = h - take`. Head is a command-less fragment of height `take`.
- `rest <= 0` → `Fit`. Else `Partial` whose tail is `FlowNode::Spacer` of `NonNegative::pt(rest)` (`flow.rs:186-198`).

Never `MoveToNextPage`. A spacer taller than the page becomes the first content of the next page (test `spacer_remainder_leads_next_page`, `paginate.rs:522-549`: 80 pt spacer on a 50 pt page, page 0 has no ops, paragraph baseline on page 1 is `30 + metrics.baseline`).

`Fragment.spacing_after` is the other vertical gap, and it is droppable:

- Stack (`flow.rs:274-279`): after a `Fit` child, if more siblings remain and `spacing > 0` and `avail.height >= spacing`, add it to `y` and shrink `avail.height`. Otherwise the gap is not carried.
- Paginate (`paginate.rs:159-167`): same drop when the remainder of the body is `< spacing`.
- Partial heads zero the field (`flow.rs:295`, `flow.rs:313`, `list.rs:251`, `table.rs:371`, `columns.rs:191`).

Proposal §2.13 states this difference: paragraph spacing is dropped when it does not fit; spacer is its own node and splits.

### Stroke geometry already in the crate

`geom::Stroke` is only a positive width (`geom.rs:520-536`). `Stroke::new(Length<Positive>)`. No dash, cap, or join field.

`geom::Path::line(from, to)` is one open segment: `begin(from).line_to(to).finish()` (`geom.rs:625-628`). `Path::rect` is a closed rectangle (`geom.rs:630-643`). `Path::translated` maps every point (`geom.rs:696-698`).

`Positive::pt` rejects `<= 0` (`geom.rs:129-136`). `NonNegative::pt` accepts 0 (`geom.rs:102-112`). `NonNegative::ZERO` is `0.0` (`geom.rs:103-104`).

Backend: `to_stroke` copies width and color and `..KStroke::default()` for cap, join, miter, dash (`backend/convert.rs:73-82`). `emit::draw_mark` for `Mark::Stroke` sets fill to `None`, stroke to that value, `draw_path` (`backend/emit.rs:122-129`).

### Table borders emit `FragmentCommand::Stroke`

`table.rs:17-22`: `Border { width: Length<NonNegative>, color }`. Comment: "Grid-line stroke centered on cell edges. Width zero draws nothing."

`TableNode::split` (`table.rs:182-188`): `Open` → `measure` then `split_measured`; `Measured` skips remeasure.

`measure` (`table.rs:191-295`):

- Column widths from `resolve_column_widths` against `avail.width` (`table.rs:192`).
- `border = width.as_pt()` (`table.rs:201`).
- `inset = border / 2 + padding` (`table.rs:202`). Comment and this formula are the only in-tree statement that the stroke is centered on the edge (half the width lies outside the path).
- `inner_full = avail.full_height - 2 * inset`; `<= 0` → `UnsplittableTallerThanFrame` (`table.rs:203-206`).
- Each cell body is `natural_height` against `Frame { width: inner_w, height: inner_full, full_height: inner_full }` (`table.rs:217-222`). Cell content therefore sees a full frame, not the page's remaining height.
- `stroke = Positive::pt(border).ok().map(Stroke::new)` (`table.rs:235`). Zero or non-positive → `None` → no stroke command. Test `zero_border_draws_no_strokes` (`table.rs:608-622`).
- Per cell, content commands are `with_offset(x + inset, y + inset)` (`table.rs:259`).
- If `stroke` is `Some`, push `FragmentCommand::Stroke { path: rect_path(x, y, w, h), paint: Paint::solid(color), stroke }` (`table.rs:260-267`).
- `rect_path` (`table.rs:393-398`): closed rectangle, top-left, clockwise, `Close`. Not `Path::line`.
- `x, y, w, h` are the cell's outer box (column span by row span), not the inset content box (`table.rs:256-262`).
- Row-group fragment `height` is `row_y[end] - row_y[start]` (`table.rs:240-244`). That height includes padding and the border's layout inset (`cell_h = content_h + 2 * inset`, `table.rs:223-226`) but does not add an extra half-stroke outside the box. The centered stroke's outer half therefore extends past the fragment.

`split_measured` (`table.rs:340-391`) places whole row-group fragments. It does not split a stroke. `MoveToNextPage` when header+first row do not fit (`table.rs:346-353`) or when zero groups were placed (`table.rs:380-383`). `Partial` keeps a `TableState::Measured` tail (`table.rs:385-390`). Groups taller than `avail.full_height` already failed in `measure` (`table.rs:278-287`). Test `header_plus_group_taller_than_page_errors_with_no_output` asserts the error and `rec.commands().is_empty()` (`table.rs:534-549`).

Paint check `borders_stroke_each_cell_centered_on_grid_lines` (`table.rs:553-606`): page 220×400, margins top 15 / left 10 / bottom 0 / right 10, border 2, padding 4, columns 50 + auto. Recorded strokes are closed rects at page coordinates (body origin y = 15, x = 10), e.g. `(10, 15) → (60, 15) → (60, row+15) → (10, row+15) → Close`. `test_util::strokes` (`test_util.rs:90-99`) reads `CanvasCommand::StrokePath` points; `Close` is `None`.

### Second Stroke producer (not flow)

`decorate::measure_layer` (`decorate.rs:211-231`) turns behind/front `Ink::Stroke(pen)` / `Ink::FillStroke` into the same `FragmentCommand::Stroke`. Those fragments are placed at `(0, 0)` with `height: 0` (`decorate.rs:238-244`). They are not flow nodes and do not occupy body height. `paint_pages` draws them outside the body list (`paginate.rs:269-275`).

### Where width `W` is chosen before a child splits

The child does not know its container. It reads `Frame.width` at `split`.

| Container | Frame passed to children | Site |
| --- | --- | --- |
| Page body | `width = page_w - left - right`, `height = (body_h - y).max(0)`, `full_height = body_h` | `paginate.rs:21-27`, `148-153` |
| Stack | Same frame, `height` reduced by placed children and consumed spacing | `flow.rs:270-278` |
| Columns | Per column: `width` = resolved column width (gaps already subtracted), `height` / `full_height` copied from the parent frame. Fragment then `with_offset(x, 0)` | `columns.rs:130-166` |
| List item body | `width = avail.width - indent` (marker column removed). Body fragment `with_offset(indent, y)` | `list.rs:173-174`, `183-187`, `215` |
| Table cell | `width = span_width - 2*inset`, `height = full_height = inner_full`. Only via `natural_height` | `table.rs:211-222` |
| Keep-together measure | `width = avail.width`, `height = full_height = avail.full_height` | `flow.rs:211-216` |
| Header/footer band | `width = page width` (not body width), `height = full_height = page height`, then result must be `<= margin` | `decorate.rs:248-266` |

§2.1 (`docs/提案书.md:193-206`) is this table: body, column, cell content width, list content width. §2.3 says default rule width is that `W`, and inside a column the stroke only reaches the column width (`docs/提案书.md:270-283`).

### Container allow-list (`src/layout/lower.rs`)

`check_content` (`lower.rs:30-65`) and `node_name` (`lower.rs:67-79`) match every `FlowNode` variant. A new variant does not compile until both arms exist.

Current allow-list:

| Node | Column | Table cell | List item | Band |
| --- | --- | --- | --- | --- |
| Text, Paragraph, Spacer | yes | yes | yes | yes |
| Stack | recurse | recurse | recurse | recurse |
| KeepTogether | yes, recurse only if `Open` | no | no | no |
| Columns | yes | no | no | yes, and recurse into column stacks |
| Table | yes | no | no | no |
| List | yes | yes | yes | no |
| Image | yes | yes | no | no |
| PageBreak | no | no | no | no |

Proposal text that names 分隔线 as allowed content (not yet encoded in `check_content`): column contents include 线 (`docs/提案书.md:324`); table cells include 分隔线 (`docs/提案书.md:377-381`); list items include 线 (`docs/提案书.md:453`). Bands are not named.

`CellBody::into_node` / `ItemBody::into_node` wrap accepted nodes in `FlowNode::Stack` (`lower.rs:91-93`, `106-108`).

### Errors (`src/error.rs`)

- `UnbreakableWiderThanFrame { width, avail }` (`error.rs:74-79`). Display: `unbreakable content width {width} exceeds available width {avail}` (`error.rs:157-159`).
- `UnsplittableTallerThanFrame` (`error.rs:81`). Display: `unsplittable content is taller than the body frame` (`error.rs:161-163`). No numeric payload.
- `InvalidGeometry` for a `PageBreak` that reaches `FlowNode::split`, a non-positive frame, and several other geometry failures (`flow.rs:156`, `error.rs:139`).
- `DecorationTallerThanMargin` / `DecorationWiderThanFrame` are the band rewrites of flow errors (`decorate.rs:264-272`).

### Paint entry

`Document::paint` (`document.rs:331-377`): stored builder error returns before any canvas call (`341-342`); sections `lower` to `(PaginateConfig, FlowNode, Decorations)`; `paginate_sections`.

`paginate_sections` (`paginate.rs:93-118`):

1. `plan_pages` for every section. `?` returns here.
2. `measure_bands`. `?` returns here. Comment: failure leaves the canvas untouched (`decorate.rs:97-98`).
3. `paint_pages` (`paginate.rs:266-279`). First canvas mutation of a page is `canvas.start_page(page.size)` (`paginate.rs:268`).

`RecordingCanvas::start_page` pushes `CanvasCommand::StartPage` (`canvas.rs:312-314`). Tests that fail layout assert `rec.commands().is_empty()` (no `StartPage`): paginate `675`, table `549`, document `keep_together_taller_than_the_page_fails_before_any_page` (`document.rs:1313-1320`) via `assert_fails_with_empty_canvas` (`document.rs:1228-1232`).

`plan_pages` (`paginate.rs:120-203`):

- Root `Stack` is unwrapped one level. Any other root becomes a one-node list (`paginate.rs:126-129`). Nested stacks are not flattened.
- `PageBreak` is not split. `apply_page_break` (`paginate.rs:214-234`) implements the three cursors `Fresh` / `HasBody` / `PendingBlank` (`paginate.rs:44-48`).
- Other nodes: `ensure_content_page` (opens a `PagePlan` in memory only, `paginate.rs:205-211`), build `Frame`, `node.split`.
- `Fit`: push `PlacedFragment` at `(body_left, body_top + y)`, advance `y` by `height`, then maybe `spacing_after` (`paginate.rs:155-168`).
- `Partial`: push head, `commit_page`, insert `tail` at front, `y = 0`, cursor `Fresh` (`paginate.rs:170-186`). Does not look at `head_height` (`head_height: _`).
- `MoveToNextPage` when `y == 0.0`: `return Err(UnsplittableTallerThanFrame)` (`paginate.rs:187-190`). This is the loop brake: a node that refuses a fresh full frame must not be retried forever.
- `MoveToNextPage` when `y > 0`: commit, insert tail, reset `y` (`paginate.rs:191-194`).

When `y == 0`, `avail.height == full_height == body_h`. Image and keep-together already return `UnsplittableTallerThanFrame` from `split` in that case, so they do not reach the `y == 0` arm. The arm still fires if any node returns `MoveToNextPage` on a full frame.

Draw order inside a started page (`paginate.rs:269-275`): `behind`, `header`, body `placed`, `footer`, `front`. Each goes through `draw_fragment`.

---

## Flow

### 1. Build

Public builders push `FlowNode` variants (`document.rs`). Relevant existing pushes: `KeepTogether` `616-618`, `Spacer` `622-624`, `PageBreak` `627-629`, `Image` `465-471`. No horizontal-rule builder. Root of a section is `FlowNode::Stack(flow.into_nodes())` (`document.rs:160`).

Nested containers call `check_content` at construction (`columns.rs:115`, `CellBody::new` `lower.rs:86-88`, `ItemBody::new` `lower.rs:101-103`, bands at measure time `decorate.rs:257`).

### 2. Plan (no canvas)

`plan_pages` walks the top-level list. Each non-`PageBreak` node is `FlowNode::split` against the remaining body frame.

`FlowNode::split` (`flow.rs:153-166`) always `avail.validate()` first.

Stack (`split_stack`, `flow.rs:243-330`):

- Empty → `Fit` height 0.
- Pull children from the front.
- `Fit`: offset child commands by `(0, y)`, append, `y += height`, shrink remaining height, maybe consume `spacing_after` if another sibling exists.
- `Partial`: offset head, `y += head_height`, return `Partial` whose tail is `Stack([child_tail, …rest])`. `spacing_after` of the head is discarded (the stack head sets `spacing_after: 0`).
- `MoveToNextPage`: if nothing has been placed, bubble `MoveToNextPage(Stack([node, …rest]))`. If something has been placed, return `Partial` of the placed prefix and `Stack([node, …rest])`.

Columns (`columns.rs:130-196`): split every column against the same remaining height and that column's width. `progress` is true only when some column's placed height is `> 0`. All finished → `Fit`, `spacing_after` = max of column spacings. None progressed and some tail → `MoveToNextPage` of a new `ColumnsNode`. Some progress and some tails → `Partial`.

List (`list.rs:172-244`): same stack-like loop on items, but the body frame is narrower by `indent`. A body `MoveToNextPage` or a zero-height `Partial` pushes the item back and either bubbles `MoveToNextPage(List)` or returns `partial` (`list.rs:195-204`). Marker glyphs use `first_baseline` (`list.rs:258-265`), which reads only `Glyphs`. `Fill`, `Stroke`, and `Image` yield `None`, so the marker falls back to its own baseline (`list.rs:210`). A stroke-only item does not move the marker.

Table: measure all cells to fragments (strokes included), then pack row groups. See Components.

Image / spacer / keep-together / paragraph: see Components.

`natural_height` (`flow.rs:333-341`) is the "must fit this full frame" gate used by keep-together and table cells. `Partial` and `MoveToNextPage` both become `UnsplittableTallerThanFrame`. A node that only `Fit`s when `height <= full_height` passes. A node that `Partial`s whenever `height > 0` fails this gate even if the piece would have fit.

### 3. Band measure (still no canvas)

After every section has a `PagePlan`, `measure_bands` runs. Band flow is `FlowNode::Stack(nodes).split` on a frame as tall as the page (`decorate.rs:265`). `Fit` with `height <= margin` is kept. `Fit` taller than the margin, `Partial`, `MoveToNextPage`, or `UnsplittableTallerThanFrame` become `DecorationTallerThanMargin`. `UnbreakableWiderThanFrame` becomes `DecorationWiderThanFrame`.

### 4. Paint

`paint_pages` → `start_page` → `draw_fragment` for each placed fragment → `finish_page`.

`draw_fragment` (`flow.rs:344-373`) translates by the placed origin and:

- `Glyphs` → `draw_glyph_run`
- `Fill` → `fill_path`
- `Stroke` → `stroke_path(&path.translated(ox, oy), paint, stroke)`
- `Image` → `draw_image`

Two translations stack: parents (`with_offset`) bake container x/y into the path; `draw_fragment` adds the page origin. A stroke emitted in fragment-local coordinates therefore lands at `page_origin + stack_y + local`.

`RecordingCanvas` stores `StrokePath` (`canvas.rs:330-336`). `pages()` only emits a page after `FinishPage` (`canvas.rs:256-295`). An error before `start_page` leaves `commands()` empty, so `pages()` is also empty.

---

## Files Read

- `src/layout/flow.rs` (entire, 373 lines)
- `src/layout/image.rs` (entire)
- `src/layout/paginate.rs` `1-280`, tests `522-549`, `653-676`
- `src/layout/table.rs` `1-399`, tests `534-669`
- `src/layout/columns.rs` `90-197`
- `src/layout/list.rs` `172-265`, test `479-493`
- `src/layout/paragraph.rs` `28-266` (`align_x`)
- `src/layout/lower.rs` (entire)
- `src/layout/decorate.rs` `185-275`
- `src/layout/style.rs` `1-14` (`TextAlign`)
- `src/layout/test_util.rs` `47-99` (`render`, `strokes`)
- `src/layout/mod.rs`, `src/layout/dump.rs`
- `src/error.rs` `73-163`
- `src/geom.rs` `70-148` (length brands), `520-565` (`Stroke`, `Pen`), `617-643` (`Path::line`, `Path::rect`), `696-698` (`translated`)
- `src/canvas.rs` `182-197`, `312-336`
- `src/document.rs` `331-377`, `612-629`, `1228-1232`, `1313-1320`
- `src/backend/convert.rs` `73-82`
- `src/backend/emit.rs` `115-129`
- `docs/提案书.md` §2.0 `208-226`, §2.1 `193-206`, §2.3 `270-285`, §2.5 `314-325`, §2.5.1 `327-336`, §2.6 cell table `377-381`, §2.8 item contents `453`, §2.13 `545-554`
- `git rev-parse` → `7cb1654`, branch `feat/hline`

Grep for `FlowNode::` under `src/` (exhaustive match sites listed in Boundaries).

---

## Boundaries

### Seams a new unbreakable horizontal Stroke node has to touch

The node is not designed here. These are the compile and behavior slots that already exist.

1. **`FlowNode` variant + `FlowNode::split` arm** (`flow.rs:139-167`). This is the only dispatch. Paginate does not match node kinds except `PageBreak` vs everything else (`paginate.rs:142-147`). Stack, columns, list, keep-together, and table cells all call `FlowNode::split` / `natural_height`, so they pick up a new arm with no further split-loop edits.

2. **`lower.rs` `check_content` and `node_name`** (`lower.rs:32-79`). Exhaustive. Until those arms exist the crate does not build. Proposal-allowed containers that the matrix does not yet mention as a variant: column, table cell, list item. Band is a separate policy; today only Text/Paragraph/Spacer/Stack/Columns pass, and a too-tall or `MoveToNextPage` child is rewritten to `DecorationTallerThanMargin` (`decorate.rs:268-269`).

3. **Split function shape closest to `ImageNode::split`** (`image.rs:29-60`), not spacer and not keep-together:
   - Pure geometry. No `FontStore`. Image's method does not take `LayoutCtx`; `FlowNode::split` still has `ctx` and can ignore it the way it does for spacer (`flow.rs:159`).
   - No `Prepared` cache. Height does not depend on child layout.
   - Do not return `Partial`. `natural_height` (`flow.rs:338-340`) turns `Partial` into `UnsplittableTallerThanFrame`, which would reject the node inside keep-together and inside table cells even when it fits the full frame.
   - Width: default is `avail.width` (the `W` already narrowed by columns/list/cell). Optional shorter width uses the `TextAlign` x formula shared by `image.rs:44-48` and `paragraph.rs:260-266`. Image's too-wide path is `UnbreakableWiderThanFrame` (`image.rs:32-37`). Paragraph's too-wide path is `InvalidGeometry` because wrapping already happened (`paragraph.rs:95-98`).
   - Height compare: `> full_height` → `UnsplittableTallerThanFrame`; `> avail.height` → `MoveToNextPage`; else `Fit`. That is image `38-43` and keep-together `227-239`. On a fresh page `avail.height == full_height`, so the `MoveToNextPage` arm is not taken; paginate's `y == 0` brake (`paginate.rs:187-190`) is the backstop if it is taken anyway.
   - `Fit` fragment: `spacing_after: 0.0` (image `53`, table fragments `242`). Own `space_before` / `space_after` belong inside `Fragment.height`, not in `spacing_after`. Stack (`flow.rs:275-279`) and paginate (`paginate.rs:162-167`) drop `spacing_after` when the remainder is too small. That would violate §2.3 "计入节点高度" and "整条不可拆".
   - Occupied height is `space_before + thickness + space_after` (§2.3, `docs/提案书.md:280-282`). Spacer proves a command-less vertical gap can exist (`flow.rs:181-185`), but spacer `Partial`s. An unbreakable node keeps the padding in the same `Fit`/`MoveToNextPage` box as the stroke.

4. **Command to emit: existing `FragmentCommand::Stroke`** (`flow.rs:86-90`). No new command variant. `with_offset` already translates stroke paths (`flow.rs:69-70`). `draw_fragment` already calls `stroke_path` (`flow.rs:362-365`). `list::first_baseline` already ignores `Stroke` (`list.rs:261-263`).

   Path helper already in geom: `Path::line` (`geom.rs:625-628`). Table uses a closed `rect_path` instead (`table.rs:263-266`, `393-398`) because a border is a rectangle, not a rule.

   `Paint::solid(color)` is what table uses (`table.rs:236`). `Stroke::new` needs `Length<Positive>` (`geom.rs:527-530`). Table turns a non-negative border into a stroke with `Positive::pt(border).ok()` and skips the command at 0 (`table.rs:235`, `260`). §2.0 says 线粗 is a non-negative length (`docs/提案书.md:226`). `Stroke` cannot represent width 0. A zero thickness cannot be this command; table's choice is "draw nothing".

5. **Parent offset, not a new coordinate system.** Stack `with_offset(0, y)` (`flow.rs:268`). Columns `with_offset(x, 0)` (`columns.rs:164`). List body `with_offset(indent, y)` (`list.rs:215`). Table content `with_offset(x + inset, y + inset)` (`table.rs:259`). A rule in fragment-local space is shifted by those. Column "only as wide as the column" falls out of the column frame's `width` (`columns.rs:146-150`) if the rule's default length is `avail.width`.

6. **Paginator stays generic** if the node is not a page-break. Do not add a `plan_pages` arm. `PageBreak` is the only node intercepted before `split` (`paginate.rs:143-146`). Hitting `split` with `PageBreak` is `InvalidGeometry` (`flow.rs:156`).

7. **Error-before-`start_page` is structural.** `plan_pages` and `measure_bands` both finish before `paint_pages` (`paginate.rs:101-117`, `266-268`). `UnsplittableTallerThanFrame` from `split`, from `natural_height`, or from `MoveToNextPage` at `y == 0` all return out of `paginate_sections` with the `RecordingCanvas` log still empty. Same for `UnbreakableWiderThanFrame` during plan. Band measure can still fail after a successful body plan, and that also happens before `start_page`.

8. **Tests that already observe strokes and empty canvases.** `test_util::render` calls `paginate_root` (`test_util.rs:54-63`). `strokes()` lists `StrokePath` polylines (`test_util.rs:90-99`). Table border test is the pattern for asserting a stroke's page coordinates (`table.rs:553-596`). Empty-canvas assertions: `paginate.rs:675`, `table.rs:549`, `document.rs:1228-1232`.

### What does not need a new paint path

`Canvas::stroke_path` (`canvas.rs:197`), `RecordingCanvas` (`canvas.rs:330-336`), krilla `Mark::Stroke` (`krilla_canvas.rs:261-268`), `to_stroke` + `draw_path` (`convert.rs:73-82`, `emit.rs:122-129`). Cap, join, miter, and dash stay krilla defaults. §2.3 says dashed, double, and vertical rules are out of v1 (`docs/提案书.md:279`), which matches a `Stroke` that has only width.

### Exhaustive `FlowNode` matches in production code

- `flow.rs:155-166` `split`
- `lower.rs:32-56` `check_content`
- `lower.rs:67-78` `node_name`

`paginate.rs:126-129` and `142-147` are not exhaustive on variants. `decorate.rs:265` splits a `Stack`. Adding a variant does not require a paginate match.

---

## Non-Obvious Things

1. **`Fragment.spacing_after` is not node padding.** It is a gap after the node that stack and paginate discard when the remainder is smaller than the gap (`flow.rs:275-279`, `paginate.rs:162-167`). Keep-together even strips it during measure and restores it only on `Fit` (`flow.rs:217-218`, `227-229`). §2.3 `space_before` / `space_after` are inside the unbreakable height. Putting them in `spacing_after` makes the tail gap disappear at a page boundary while the stroke stays on the previous page. Spacer is the opposite bug if copied: it splits the gap across pages (`flow.rs:179-198`) and would tear `space_before` from the stroke.

2. **Stroke is centered on the path; table fragment height does not contain the outer half.** `inset = border/2 + padding` (`table.rs:202`) and the struct comment (`table.rs:17`) say the stroke is centered on the cell edge. The row fragment height is the row box (`table.rs:240-241`). Half the border paints outside that box. A rule whose layout height includes the full thickness cannot put the path at local `y = 0` (or at `y = space_before`) if the ink is supposed to stay inside `[space_before, space_before + thickness]`. The centerline has to sit half a thickness below the top of the ink. Table's closed rect is the wrong shape to copy; the inset formula is the evidence that width extends to both sides of the path. `to_stroke` does not set cap (`convert.rs:77-81`), so endpoint extension past `from`/`to` is whatever `KStroke::default()` does; this repo does not document that default.

3. **Zero width cannot be a `Stroke`.** `Positive::pt(0)` is `NonPositiveLength` (`geom.rs:132-134`). Table skips the command (`table.rs:235`). A rule of thickness 0 can still occupy `space_before + space_after` only as a command-less fragment (spacer's fragment shape, `flow.rs:181-185`) or it cannot use `FragmentCommand::Stroke`. §2.0's "非负" 线粗 and `Stroke`'s strict positive width are different types.

4. **`natural_height` collapses both non-fit splits into one error** (`flow.rs:338-340`). Table cells and keep-together never show a child the remaining page height. They show `height == full_height`. A rule that `Fit`s on that frame is baked into the cell or the group. The page break is then the row (`table.rs:359-383`) or the whole group (`flow.rs:234-237`), not the rule. A rule that `Partial`s on a full frame cannot live in a cell or a keep-together.

5. **Fresh-page `MoveToNextPage` is a hard error, not a blank page.** `paginate.rs:187-190`. Image avoids it by comparing `full_height` before returning `MoveToNextPage` (`image.rs:38-43`). When `y == 0`, remaining height equals `full_height`, so those two compares are the same number. A rule taller than the body errors inside `split` if it follows image. If it returns `MoveToNextPage` anyway, paginate errors with the same `UnsplittableTallerThanFrame` and still has not called `start_page`.

6. **List markers ignore strokes.** `first_baseline` skips `Stroke` (`list.rs:261-263`). An item whose first content is only a rule does not supply a baseline; the marker uses the marker run's own y (`list.rs:210`). Marker test for images: `image_command_has_no_baseline` (`list.rs:479-493`).

7. **List and table narrow `W` before the child runs.** A default "full frame width" rule inside a list item is `avail.width - indent`, then shifted right by `indent` (`list.rs:173-174`, `215`). It does not cross the marker column. Inside a cell it is `span_width - 2*inset`, and the cell fragment is later shifted by `inset`, while the border stroke is a separate command on the outer rect (`table.rs:211-222`, `256-266`). The rule does not replace the border.

8. **Column progress ignores zero-height fits.** `progress |= placed > 0` (`columns.rs:160`). A zero-height rule that `Fit`s does not count as progress. If every column places 0 and none has a tail, the columns node still `Fit`s with height 0 (`columns.rs:174-182`). If a sibling column `MoveToNextPage`s and this rule places 0, the columns node `MoveToNextPage`s as a whole (`columns.rs:184-186`).

9. **Root stack is unwrapped only once** (`paginate.rs:126-129`). A rule nested in an inner `Stack` is split by `split_stack`, not by `plan_pages`. Its `MoveToNextPage` becomes either a bubbled `MoveToNextPage(Stack(…))` or a `Partial` of the siblings already placed (`flow.rs:301-317`).

10. **Behind/front strokes are a different lifetime.** They are measured after pagination, origin `(0,0)`, fragment height 0 (`decorate.rs:238-244`), and drawn in the page stack outside the body (`paginate.rs:269-275`). A flow rule that went through that path would not consume body `y` and would not move to the next page with the flow. §2.3 says the rule is not a `behind` escape (`docs/提案书.md:272`).

11. **`PageBreak` inside `split` is `InvalidGeometry`**, including a `PageBreak` nested in a child stack that paginate did not peel (`flow.rs:156`). Top-level breaks never become fragments. A rule must go through `split` or it will not paint.

12. **Alignment enum already exists.** `TextAlign::{Start, Center, End}` (`style.rs:10-14`). Default for images is chosen by the caller (`image.rs:23`). §2.3 says a short rule defaults to Center (`docs/提案书.md:277`). Nothing in layout defaults a missing align to Center; image tests pass `TextAlign::End` explicitly (`image.rs:149`). Paragraph uses the style's align (`paragraph.rs:248`).

13. **No flow-node tests live in `flow.rs`.** Spacer, keep-together, and the `y == 0` error are tested through `paginate_root` (`paginate.rs:522-549`, `653-676`). Image and table test their own `split` and `render`.

---

## Open Questions

These are facts the layout code does not decide. They are not an API design.

1. **Centerline y vs box.** §2.3 defines occupied height as `space_before + thickness + space_after` and says the fragment contains one horizontal `Stroke`. It does not say whether the path sits on the visual center of the thickness. Table code is the only implementation that places a stroke relative to a layout box, and it centers on the edge so half the ink is outside the path (`table.rs:17`, `202`). Which of those two the rule should follow is not written in `src/`.

2. **Thickness 0.** §2.0 types 线粗 as non-negative, so 0 is constructible as `NonNegative`. `FragmentCommand::Stroke` cannot carry it. Table omits the command. Whether a 0-thickness rule is an empty gap of `space_before + space_after`, a build error, or impossible at the type layer is not settled in layout code.

3. **Shorter-than-`W` but longer than `avail.width`.** Image reports `UnbreakableWiderThanFrame` (`image.rs:32-37`). §2.3 says a fixed length may be shorter than `W` and does not say what a length greater than `W` returns. No rule node exists to copy.

4. **`check_content` arms for a new variant.** Proposal allows the rule in columns, table cells, and list items. It does not mention header/footer bands. `check_content` will not compile without an explicit bool for each container (`lower.rs:32-56`). Band failure mode, if the node is allowed there, is already `DecorationTallerThanMargin` / `DecorationWiderThanFrame` (`decorate.rs:264-272`), and the band frame width is the full page width (`decorate.rs:259-262`), not the body width.

5. **List-item marker alignment** when the item's first fragment has only a `Stroke`. Today the marker keeps its own baseline (`list.rs:210`, `258-264`). §2.8 does not say where the marker sits beside a rule.

6. **Krilla default line cap** is not recorded in this repo. `to_stroke` keeps `KStroke::default()` (`convert.rs:73-81`). Butt vs projecting caps changes whether the ink is longer than the path. Layout tests that use `strokes()` compare path points, not the painted envelope (`test_util.rs:90-99`, `table.rs:589-596`).

7. **`space_before` at the top of a page.** Spacer can be the first thing on a page and still consumes height (`paginate.rs:522-549`). A rule that `Fit`s does the same if its height includes `space_before`. Nothing in `plan_pages` strips leading space. Whether leading `space_before` should collapse is not implemented anywhere.
