### Components Found

Two `Cell` types. Public `quire_pdf::Cell` (`src/document.rs` 1033–1128) stores one private `CellContent`. Layout `table::Cell` (`src/layout/table.rs` 50–73) stores a `CellBody` of `FlowNode`s. Callers only see the public one. Integration tests talk to the public one through `document(…).pages()`.

**Public cell payload (`src/document.rs` 1042–1093).** Four arms, each a whole cell:

| Arm | Constructor | Lowered node (`Table::lower` 1007–1016) |
| --- | --- | --- |
| `Text(String)` | `Cell::new` / `From<&str>` | `flow.text` → `FlowNode::Text(TextNode::new)` — one `Face::Body` run (`665–667`, `paragraph.rs` 109–116) |
| `Image` | `Cell::image` | `FlowNode::Image` |
| `HLine` | `Cell::hline` | `FlowNode::HLine` |
| `Spacer` | `Cell::spacer` | `FlowNode::Spacer` |

There is no `Cell` arm for a `Paragraph` closure, `ul` / `ol`, bold, blank, or link. `col_span` / `row_span` (1096–1109) do not add content. Every `lower` arm is `vec![one node]`.

**Rich paragraph already exists on `Flow`, not on `Cell`.** `Flow::paragraph` (`679–690`) runs a `Paragraph` closure and pushes `FlowNode::Text(TextNode::from_items(...))`. `Paragraph` (`797–845`) is where `text`, `bold`, `blank` / `blank_em` / `blank_to_end`, and `link` live. That node is the same `TextNode` a string cell uses, with extra inline items. `FlowNode::Paragraph` (`flow.rs` 165) is a different, already-broken-line node used by layout pagination tests. The public DSL never builds it. Cell bold, blank, and link are properties of `TextNode` items, not of `FlowNode::Paragraph`.

**List already exists on `Flow`.** `Flow::ul` / `Flow::ol` (`745–775`) build `FlowNode::List`. `List::item` (`1153–1156`) gives the closure a nested `Flow`, so an item can call `paragraph` (and therefore bold, blank, and link). `List::lower` (`1168–1176`) hardcodes `marker_width` at 18 pt and copies `parent.style` onto the marker. Layout places the marker glyph at x = 0 of the list fragment and shifts the item body by `indent` (`src/layout/list.rs` 207–215).

**Allowlist already permits both (`src/layout/lower.rs` 30–67).** `check_content(Container::TableCell, …)` returns true for `Text`, `Paragraph`, `Spacer`, `HLine`, `Image`, `Stack` (children rechecked), and `List` (`List` is rejected only in a band, line 56). It returns false for `Columns`, `Table`, `PageBreak`, and `KeepTogether`. It does not walk into a `List` node's items. Item bodies are checked earlier, as `ListItem` (`ItemBody::new`, 104–107).

**Cell paint offset (`src/layout/table.rs` 191–259).** `measure` gives each cell `Frame { width: inner_w, height: inner_full, full_height: inner_full }` and calls `natural_height` (`flow.rs` 341–350). `inner_w` is the spanned columns minus `2 * inset`; `inset = border/2 + padding`. Public `Table::lower` leaves `TableSpec` padding and border at the `new` defaults, both zero (`document.rs` 1023–1028, `table.rs` 35–46), so inset is 0 and a one-column table's content width is the page body width. Equal `Ratio` columns split that width. Commands are then shifted by `(x + inset, y + inset)`. `draw_fragment` (`flow.rs` 353–384`) applies that shift to glyph origins, strokes, and `AnnotateLink` rects.

**Proposal, already decided.** §2.6 cell children (`docs/提案书.md` 377–382): paragraph including 填空, rule, and spacer; image and list as those land. Forbid `columns`, pageBreak, keep_together, and nested table. DSL signatures are not pinned (384). §2.1 (201): a paragraph in a cell wraps at that cell's content width. §2.2 (241–266): blank is an inline run beside `text` / `bold`. §2.8 (453–454): a list in a cell is allowed; a list item holds paragraph (including 填空), rule, spacer, nested list, and not table or columns. §8 (616–619): a URI on text becomes a link rect per visual line; the sentence names 正文 and header/footer.

### Flow

What a test can actually call today:

1. `document(page, |doc| …)` paints into a recording. `Rendered::pages` is that recording. `tests/document.rs` never names `RecordingCanvas`.
2. A table cell is a string, `Cell::new`, `Cell::image`, `Cell::hline`, or `Cell::spacer`. None of those accept `|p: &mut Paragraph|` or `|l: &mut List|`.
3. Bold, blank, and link are reached only by `doc.paragraph`, `col.paragraph`, or `item.paragraph`. A string cell lowers through `TextNode::new`, which stores one body run and no blank and no link (`paragraph.rs` 109–116).
4. A list is reached only by `doc.ul` / `doc.ol`. `check_content` would accept the resulting `FlowNode::List` inside a cell. `Table::lower` has no arm that builds one.
5. At paint, a cell that did hold a `TextNode` or `ListNode` would still go through `natural_height`. `Fit` contributes height and commands, then the table offsets them. `Partial` or `MoveToNextPage` becomes `Error::UnsplittableTallerThanFrame` before any page is drawn. A body paragraph or list may split; the same nodes inside a cell may not.

Helpers that read the recording in `tests/document.rs`:

- `runs` (63–78) keeps glyph text, font id, origin, and advance width. `texts` (80–82) keeps text only. `glyph` (1354–1359) finds one run by text.
- `stroke_samples` (1318–1352) keeps a two-verb stroke's endpoints, width, and color. This is the blank underline, and also an hline.
- `links` (1361–1383) keeps `CanvasCommand::AnnotateLink` origin, size, and URI string.

Layout unit tests use a different recording. `test_util::render` (54–64) paginates into a `RecordingCanvas`. `runs` / `texts` / `bits` / `strokes` (`test_util.rs` 67–99) see glyph text and path points. They do not see font id, stroke width, stroke color, or link rects. Paragraph unit tests in `src/layout/paragraph.rs` assert `Fragment` commands from `TextNode::split`, before any cell offset and before the canvas.

### Files Read

- `tests/document.rs` — public canvas tests for tables, cells, paragraphs, blanks, links, and lists.
- `src/document.rs` — `Flow::paragraph` / `ul` / `ol` / `text`, `Paragraph`, `Table::lower`, public `Cell`, `List::lower`.
- `src/layout/table.rs` — `measure`, cell allow/reject tests, border recording test.
- `src/layout/lower.rs` — `check_content` / `CellBody`.
- `src/layout/flow.rs` — `natural_height`, `draw_fragment` (glyphs, strokes, links).
- `src/layout/paragraph.rs` — `TextNode::new` vs `from_items`; fragment-level blank and link tests.
- `src/layout/list.rs` — marker placement; `render` tests for marker x; item reject tests.
- `src/layout/test_util.rs` — `render`, `runs`, `bits`, `strokes`.
- `docs/提案书.md` §2.1, §2.2, §2.6, §2.8, §8.
- `.audit/handoff/knives-done.md` — knife 17 sentence (cell paragraph, blank/link if same pipeline, cell list; still forbid columns / pageBreak / keep_together / nested table).

### Boundaries

**Cell canvas tests that exist (`tests/document.rs`).** Each cell is still one payload.

| Test | Lines | What the recording shows |
| --- | --- | --- |
| `kind_a_flow_is_two_paragraphs_then_a_table` | 105–126 | String cells. Header glyphs `"Item"` / `"Amount"` after two paragraphs: `y = 2*LINE + BASELINE`, second column `x = A4.width/2`. No wrap, no second font, no stroke, no link. |
| `table_spans_and_repeat_header_switch` | 300–325 | `"Total"` at `x = 0` on the last page when `col_span` is 2. Repeat-header on/off changes page-2 texts. |
| `cell_image_and_last_page_seal_share_coordinates` | 624–657 | `Cell::image` on a 200 pt page, two equal columns. Image rect is `(100, 0, 10, 10)`. krilla feature only. |
| `hline_in_list_item_and_cell` | 1193–1232 | List hline verbs are pinned at x = 18 and x = 58, y = 0.5. The cell hline (`color(rgb(9,9,9))`) is only counted: `strokes.len() >= 2`. Its path, y, and color are not read. |
| `cell_spacer_shifts_the_following_row` | 1248–1271 | Spacer cell emits no non-glyph ops. `"B"`'s y grows by the spacer height. |
| `cell_spacer_alone_records_no_ops` | 1275–1286 | `pages[0].ops().is_empty()`. |
| `cell_spacer_taller_than_body_fails_before_any_page` | 1289–1300 | `UnsplittableTallerThanFrame`, no pages. |
| `short_cell_spacer_does_not_grow_a_text_row` | 1303–1316 | An 8 pt spacer beside `"A"` does not move `"B"`; `"B".y = LINE + BASELINE`. |
| `cell_and_layer_text_do_not_annotate` | 2056–2072 | `row(["cell"])` plus a body link. `links()` has length 1 and the target is the body URI. Texts are `"mark"`, `"cell"`, `"Go"`. This is the negative proof: a string cell adds a glyph and no `AnnotateLink`. |

**Body paragraph tests to mirror for a cell.** All of these call `doc.paragraph`, never `table`.

| Test | Lines | Recording assertion |
| --- | --- | --- |
| `mixed_body_and_bold_runs_keep_fonts_and_origins` | 436–460 | `runs`: `("A", font 0)`, `("B", font 1)`, `("C", font 0)`. Each next `x` is previous `x + width`. All three share `y`. Needs `bold_font`. |
| `bold_and_indent_configuration_errors_are_specific` | 499–506 | `p.bold` with no bold face is `UnknownFont { id: 1 }`. |
| `middle_blank_shifts_the_next_glyph_and_strokes_the_gap` | 1394–1422 | `"C".x = "AB".x + "AB".width + 30`. `stroke_samples[0]`: x from end of `"AB"` to start of `"C"`, `y = baseline + 4`, width 2, color black. |
| `blank_does_not_change_line_height` | 1425–1446 | `"B".y` with a blank equals `"B".y` without one. |
| `fill_blank_reaches_the_frame_edge_with_the_default_stroke` | 1449–1470 | Stroke from end of `"A"` to x = page width (120), `y = baseline + 1`, width 1. |
| `blank_em_is_font_size_times_count_and_needs_no_bold_face` | 1473–1493 | Stroke length 24 (2 × 12 pt) with only a body font. One empty glyph run. |
| `zero_width_blank_draws_no_stroke_and_does_not_shift` | 1496–1516 | No stroke. `"B".x = "A".x + "A".width`. |
| `blank_stays_on_its_line_when_the_paragraph_splits` | 1573–1594 | Page height `LINE`. Stroke stays on page 1; `"B"` on page 2 at `y = BASELINE`. |
| `partial_link_rect_matches_the_linked_glyphs` | 1614–1636 | One `links()` hit. Target is the URI. `x` and `width` match the `"BC"` run. `y = 0`, `height = LINE`. |
| `wrap_emits_one_link_rect_per_visual_line` | 1639–1673 | Two hits, same URI, each matching one glyph run. Second hit `y = LINE`. |
| `body_and_bold_share_one_link_rect` | 1676–1710 | Three glyph runs, fonts implied by text split `"A"` `"B"` `"C"`, one link rect whose width is the sum of the three advances. |
| `blank_inside_a_link_is_not_part_of_either_rect` | 1783–1806 | Two hits, one stroke, 30 pt gap between the rects. |
| `link_does_not_change_glyphs_or_add_a_stroke` | 2007–2030 | Stripping `AnnotateLink` leaves the same commands as the unlinked paragraph. |
| `link_across_pages_restarts_y_at_the_body_top` | 1863–1892 | Each page has one hit at `y = top margin`. |

`first_line_indent_wraps_inside_the_inset_and_does_not_repeat_after_pagination` (463–481) is the same family for indent, which §2.6 does not name separately from 段落.

**List tests to mirror for a cell marker.**

| Test | Lines | What it proves |
| --- | --- | --- |
| `lists_number_bullet_and_custom_markers` | 277–297 | Public recording. Texts: `"\u{2022}"`, `"dot"`, `"I."`, `"one"`, `"II."`, `"two"`, `"(c)"`, `"custom"`. Marker `x = 0`, item body `x = 18`. |
| `list_fill_blank_starts_after_the_marker_column` | 1519–1542 | Item blank starts at `x = 18` and the stroke ends at the page width (200). |
| `link_rect_includes_indent_column_and_list_offset` | 1843–1859 | Link rect `x` equals the item glyph `x` (18), not the marker. |
| `hanging_indent_and_marker_on_first_fragment_only` | `src/layout/list.rs` 301–338 | `render` → `RecordingCanvas`. `"1."` at x = 0, body at `marker_width` (here 20, not the public 18). Continuation line `"C"` on page 2 has no second marker. |
| `nested_list_indents_by_its_own_marker_width` | `list.rs` 342–377 | Outer marker x = 0, outer body x = 20, inner marker x = 20, inner body x = 36. |
| `upper_roman_counts_from_start_and_custom_marker_keeps_its_number` | `list.rs` 398–418 | Texts `"IV."`, `"a"`, `"(x)"`, `"b"`, `"VI."`, `"c"`. |

**Layout cell tests do not paint rich content (`src/layout/table.rs` 625–678).**

- `cell_rejects_columns_page_break_keep_together_and_tables` (626–657): `Cell::new(vec![node])` returns `InvalidCellContent { container: "table cell", node }` for `"columns"`, `"page break"`, `"keep together"`, `"table"`, and a `Stack` whose only child is a page break (reported name `"page break"`).
- `cell_allows_hline` (660–670) and `cell_allows_spacer` (673–678) only `unwrap` `Cell::new`. They do not call `render`.
- No test passes `FlowNode::List`, `FlowNode::Paragraph`, or a `TextNode::from_items` (bold, blank, or link) to `table::Cell::new`.
- `borders_stroke_each_cell_centered_on_grid_lines` (553–606) does use `render`. Cells are `text("A")` style body strings. With padding 4 and border 2, glyph `"A"` is at x = 15 (margin 10 + inset 5) and `"B"` at x = 65. That is the offset a cell paragraph's glyphs, strokes, and link rects would receive once padding exists. Public tables still have inset 0, so a public test would not see 15.
- `header_plus_group_taller_than_page_errors_with_no_output` (534–550) is the layout twin of the spacer-taller test: `render` returns `UnsplittableTallerThanFrame` and `rec.commands()` is empty. The cell text is `"A\nB"`, still one body `TextNode`.

**What no RecordingCanvas assertion proves today.**

- Cell bold. Nothing in a table reads `run.font`. The only font-id assertion is the body test at 450–455. Layout `runs` cannot see font id.
- Cell blank. No table test calls `stroke_samples` on a cell. The cell hline test does not even read the cell stroke. Spacer tests assert the absence of strokes.
- Cell link. `cell_and_layer_text_do_not_annotate` asserts the string cell contributes zero hits. No test asserts an `AnnotateLink` whose rect sits on a cell glyph.
- Cell list marker. No test puts `ul` / `ol` in a cell. Marker strings and the 18 pt body column are proved only for a body list.

**What should stay out.**

- Columns in a cell, a nested table, page break, and keep-together. Locked by `cell_rejects_…` (`table.rs` 626–657) and by §2.6 (381–382). The public `nesting_rules_come_from_layout` (`tests/document.rs` 380–396) only rejects a page break in a column (`container: "column"`). It never builds a cell with those nodes, because `Cell` has no flow closure.
- A table or columns inside a list item, including an item of a list that itself sits in a cell. `item_rejects_table_page_break_and_image` (`list.rs` 498–526) rejects `"table"`, `"page break"`, and `"image"` with `container: "list item"`. It does not mention columns; `check_content` still rejects `Columns` in a list item (`lower.rs` 47–54). Public `list_item_and_overwide_image_fail` (`tests/document.rs` 718–732) is the image case. §2.8 (453) says items do not hold table or columns.
- `check_content` does not open a `List` (`lower.rs` 56). A cell that is handed an already-built `FlowNode::List` will not re-reject a table hidden in an item. The reject happens at `ListItem::new`, which public `List::item` already calls. A cell-list test has to build the list through that path (or call `ListItem::new`) to keep the forbid.
- Do not copy the body split successes into a cell as passing canvas tests. `blank_stays_on_its_line_when_the_paragraph_splits`, `link_across_pages_restarts_y_at_the_body_top`, and the page-2 half of `hanging_indent_and_marker_on_first_fragment_only` depend on `Split::Partial`. Cell content uses `natural_height`, which turns that into `UnsplittableTallerThanFrame`. The error shape to copy is `cell_spacer_taller_than_body_fails_before_any_page` (1289–1300) and `header_plus_group_taller_than_page_errors_with_no_output` (534–550): no pages, or `rec.commands()` empty.
- `to_pdf` tests (`inline_blank_writes_a_pdf`, `text_link_writes_a_pdf`, `mixed_bold_and_indent_write_a_pdf`) only assert `%PDF`. They do not prove coordinates. AGENTS.md says layout assertions stay on the recording.

**Assertions that would prove the four cell cases, using helpers that already exist.** Public tables have inset 0. `BASELINE` is 13.812 and `LINE` is 17.244 (`tests/document.rs` 17–20). A table that is the first body block therefore has first-row glyphs at `y = BASELINE`, same as a body paragraph. Second equal column starts at half the page width (`kind_a` 123–125, cell image at x = 100 on a 200 pt page).

- Cell bold. Same shape as 436–460, inside `t.row`. `runs` font ids `(0, 1, 0)`, abutting `x`, shared `y`. On the second column, add half the page width to every `x`. Missing `bold_font` stays `UnknownFont { id: 1 }` (499–506). `test_util::runs` cannot make this assertion.
- Cell blank. Same shape as 1394–1422: next glyph shifted by the gap, `stroke_samples` from the end of the left run to the start of the right run, `y = glyph.y + baseline_offset`. A fill-blank's `x1` must be the cell content's right edge, not "the page" in general. On a one-column public table that edge equals the page width, so `fill_blank_reaches_the_frame_edge` (1449–1470) would pass even if the blank ignored the cell. The distinguishing mirror is `column_fill_blank_is_shifted_with_its_column` (1545–1570): stroke ends at the column's right edge. For the left cell of two equal columns, `x1` is half the page width. `blank_em` (1473–1493) and the zero-width case (1496–1516) carry over unchanged, because they do not depend on frame width. A blank taller than the page, or a paragraph whose wrap does not fit `inner_full`, follows the spacer error test, not 1573–1594.
- Cell link. Same shape as 1614–1636: `links()` length 1, target equals the URI, `x` / `width` match the linked `glyph`, `height = LINE`. With inset 0 and a first row, `y` is 0, same as the body. A link in the second column uses that column's `x` (`link_rect_includes_indent_column_and_list_offset` 1824–1841 is the column version of this). `cell_and_layer_text_do_not_annotate` must keep passing for `Cell::new("cell")` / `row(["cell"])`: those runs have no link item. `commands_without_links` (1385–1391) is how 2007–2030 shows the link adds no stroke and does not move glyphs.
- Cell list marker. Same shape as 277–297, with the list's fragment origin at the cell origin. First column, inset 0: marker text `"\u{2022}"` or `"I."` at `x = 0`, item body at `x = 18`, same `y` on the first line. Second column: add the column x to both. A fill-blank in that item starts at `cell_x + 18` and ends at `cell_x + inner_w` (1519–1542, shifted). A link on the item text matches the body glyph, not the marker (1843–1859). A wrapped item that still fits the page shows the marker once and the continuation line at `x = cell_x + 18`; the cross-page half of `hanging_indent_…` (331–337) is the picture, but inside a cell that second page must not appear. Layout `bits()` (`test_util.rs` 86–88) is the exact-coordinate form if the node is built as `FlowNode::List` and passed through `render` inside a `TableNode`. Public `marker_width` is 18 (`document.rs` 1171); layout specs in those tests use 20, 16, 14, 30, or 10, so a public cell test should assert 18.

### Non-Obvious Things

- A string cell and a rich paragraph are the same `FlowNode::Text` after lower. The difference is `TextNode::new` (one body run) versus `TextNode::from_items` (bold, blank, link). `Flow::paragraph` already does the second (`document.rs` 688–689). `Table::lower`'s text arm calls the first (`1008`). Allowing `FlowNode::Paragraph` in `check_content` (`lower.rs` 33–34) does not implement cell bold or blank; that variant has no inline items.
- `cell_allows_hline` and `cell_allows_spacer` do not record. Copying them for "cell allows list" would only prove `Cell::new` returns `Ok`. Marker glyphs are proved only by `render` / `document(…).pages()`.
- `hline_in_list_item_and_cell` looks like a cell stroke test and is not one. Index 0 is the list line at x = 18. The cell line is whatever stroke comes later, and the test never reads it. A cell blank test that takes `stroke_samples()[0]` will see the blank only when the cell has no border and no hline. Public tables draw no border (`zero_border_draws_no_strokes`, `table.rs` 609–623, is the layout statement of that; public specs leave border at 0).
- Fill-blank to the page edge does not prove the cell width. One public column is the page body. Two equal columns, or the layout border test's inset, are what move the stroke off the page edge. `borders_stroke_…` (598–605) is also the reminder that a non-zero inset shifts glyphs by `margin + border/2 + padding`, and link rects go through the same `with_offset` (`table.rs` 259, `flow.rs` 379–381).
- A list inside a cell is one `FlowNode::List` on the allowlist. Columns and a nested table are rejected when they are the cell's own child, and again when they are a list item's child, under a different `container` string (`"table cell"` vs `"list item"`). A test that only snapshots `InvalidCellContent` must not treat those two strings as the same failure.
- `natural_height` measures the cell against the full page body, then the row moves as a whole. A short cell paragraph that wraps (`a_paragraph_wraps_at_the_width_of_its_column`, `tests/document.rs` 351–377, is the column version) can `Fit` and record. A paragraph or list that would split vertically cannot record a second page inside the cell. `kind_a` never wraps a cell: `"Item"` and `"Amount"` are one run each.
- Document unit tests in `src/document.rs` (`paragraph_coalesces_adjacent_faces_and_ignores_empty_appends` 1578–1602, `blank_breaks_text_coalescing` 1616–1638, `link_coalesces_on_face_and_uri_and_restores_the_outer_target` 1748–1773) compare `TextNode` values on the flow. They do not paint. They are the assembly mirror, not the canvas mirror.
- Layout link tests (`paragraph.rs` 820–850 and the tests below it) read `FragmentCommand::Link` in fragment-local coordinates. A cell proof has to read `AnnotateLink` after `measure`'s offset, which is what `tests/document.rs` `links()` does. `test_util` has no link helper.
- `std::cell::Cell` in `non_finite_rotation_does_not_draw` (`document.rs` 1901–1910) is unrelated to table cells.

### Open Questions

- §2.6 line 384 still does not pin the cell DSL. The canvas assertions above do not choose between `Cell::paragraph(|p| …)` / `Cell::ul(|l| …)` and a cell `Flow` closure like `List::item`. The closure is the only public pattern that already type-checks `paragraph`, `bold`, `blank`, `link`, and `ul` together (`List::item` 1153–1156). A one-node factory matches every current `Cell` arm and every current cell test.
- §2.6 lists 段落 and 列表 as allowed children. It does not say one cell holds both, or a paragraph and a spacer. Layout `Cell::new` takes a `Vec`. Public `lower` always builds a one-element vec. No test passes two allowed nodes. Knife 16 left that combination out on purpose.
- §8 (616–619) names 正文 and header/footer, not cells. The paint path does not strip links: `draw_fragment` annotates wherever the fragment sits. A cell link recording falls out of `TextNode::from_items` plus the cell offset. `cell_and_layer_text_do_not_annotate` only shows that a string cell has no link run. Whether knife 17's passing test must include a cell URI, or only bold and blank, is the "if same pipeline" clause in the knife 17 handoff, not a second layout rule.
- A wrapped list item that fits in the cell can show one marker and a continuation line in the same recording. No existing test is that case inside a table. The page-split list test is the wrong success condition for a cell.
