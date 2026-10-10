### Components Found

Two different `Cell` types. Public `quire_pdf::Cell` (`src/document.rs` 1030–1111, re-exported at `src/lib.rs` 21–24) stores one private `CellContent`. Layout `table::Cell` (`src/layout/table.rs` 50–73) stores a `CellBody` of `FlowNode`s. Callers only see the public one.

**Public cell payload (`src/document.rs` 1039–1076).** `CellContent` is a private enum with three arms:

| Arm | Constructor | What the caller passes |
| --- | --- | --- |
| `Text(String)` | `Cell::new(impl Into<String>)` (1048–1054) | a string |
| `Image(Image, ImageBox)` | `Cell::image` (1057–1065) | image handle + closure on `ImageBox` |
| `HLine(HLine)` | `Cell::hline` (1068–1076) | closure on `HLine` |

`From<&str>`, `From<String>`, and `From<&String>` (1095–1111) all call `Cell::new`. `col_span` / `row_span` (1079–1091) only change span; they do not add content. There is no `Cell::spacer`, no `CellContent::Spacer`, and no cell method that takes `FnOnce(&mut Flow)`.

**How a row becomes layout cells (`src/document.rs` 969–1027).** `Table::header` and `Table::row` collect `Into<Cell>`. `Table::lower` (994–1027) matches each `CellContent` into a one-element `Vec<FlowNode>`:

- `Text` → `vec![flow.text(text)]`. Private `Flow::text` (665–667) is `TextNode::new(text, self.style.clone())`: one body run, the enclosing flow's style.
- `Image` → `vec![image_box.lower(&image, flow.doc)?]` (`ImageBox::lower` 484–491 → `FlowNode::Image`).
- `HLine` → `vec![line.lower()]` (`HLine::lower` 559–568 → `FlowNode::HLine`). The table flow's style is not applied.

That vec goes to `table::Cell::new(content)?.with_span(...)`. `table::Cell::new` (58–63) calls `CellBody::new`.

**Allowlist already permits a spacer (`src/layout/lower.rs` 29–97).** `check_content(Container::TableCell, …)` returns true for `FlowNode::Text`, `Paragraph`, `Spacer`, and `HLine` (33–36). It recurses into `Stack`. It rejects columns, page break, keep-together, nested table, and (for a cell) image is allowed. `node_name` of a spacer is `"spacer"` (73). `CellBody::into_node` (95–97) wraps the vec as `FlowNode::Stack`. So a layout cell can already hold `FlowNode::Spacer`; the public match never builds one.

**Spacer node (`src/layout/flow.rs` 128–209).** `SpacerNode { height: Length<NonNegative> }`. `split` emits a `Fragment` whose `commands` vec is empty. Height 0 is `Fit` of height 0. A positive height takes `min(h, avail.height)` and, if remainder remains, returns `Partial` whose tail is a shorter `SpacerNode`. `Flow::spacer` (725–728) is the only public builder: `self.push(Ok(FlowNode::Spacer(SpacerNode { height })))`.

**Cell measurement does not use that split (`src/layout/table.rs` 191–222, `src/layout/flow.rs` 341–350).** Each cell is measured with `natural_height` on a frame whose height is the page body minus `2 * inset` (`inset = border/2 + padding`). `natural_height` turns `Partial` and `MoveToNextPage` into `Error::UnsplittableTallerThanFrame`. A spacer inside a cell therefore contributes empty commands and a height, or fails the table. It does not continue as a shorter spacer on the next page the way a body `Flow::spacer` does.

**Proposal, already decided.** `docs/提案书.md` §2.6 cell children (377–381): paragraph, rule, and spacer are allowed in a cell; columns, pageBreak, keep_together, and nested table are not. §2.13 (545–553): spacer is vertical blank, non-negative height, may be 0, may split across pages in the flow, and is allowed in a cell.

### Flow

Public assemble, as tests actually call it:

1. `document(page, |doc| …)` (`src/document.rs` 42–56) runs the closure, then `Document::paint` into a `RecordingCanvas`. `Rendered::pages` (1517–1521) returns `recording.pages()`. Integration tests never name `RecordingCanvas`.
2. `doc.table(|t| …)` (`706–715`) builds a public `Table` and `lower`s it with the enclosing `Flow`.
3. A cell enters as a string (`Into<Cell>`), `Cell::new`, `Cell::image`, or `Cell::hline`. Tests do all four. None pass a spacer.
4. `lower` makes exactly one `FlowNode` per cell, then `table::Cell::new` runs the allowlist.
5. At paint, `TableNode::split` → `measure` places that node with `natural_height`, offsets commands by `(x + inset, y + inset)` (`table.rs` 246–259), and optionally strokes a border rect. A spacer adds height and no command, so the canvas shows only neighbors (glyphs, images, rules, borders).

List items are the contrasting call site. `List::item` (`src/document.rs` 1136–1138) gives the closure a nested `Flow`, so `item.spacer(height)` type-checks today. A table cell has no such closure. Layout `ListItem::new` uses the same matrix (`ItemBody::new` → `check_content(ListItem)`).

### Files Read

- `tests/document.rs` — public DSL tests and their canvas assertions.
- `src/document.rs` — `document`, `Flow::spacer` / `Flow::text` / `Flow::table`, `Table::lower`, public `Cell`, `ImageBox::lower`, `HLine::lower`, `Rendered::pages`.
- `src/layout/table.rs` — layout `Cell`, `measure`, unit tests `cell_rejects_…` and `cell_allows_hline`.
- `src/layout/lower.rs` — `check_content` / `CellBody`.
- `src/layout/flow.rs` — `SpacerNode::split`, `natural_height`.
- `src/layout/paginate.rs` — `spacer_remainder_leads_next_page` and the local `RecordingCanvas` helpers.
- `src/layout/test_util.rs` — `render`, `runs`, `texts`, `bits`, `strokes`.
- `src/layout/list.rs` — `item_allows_hline` / `item_rejects_…` (same allowlist style, list container).
- `src/lib.rs` — public re-export of `Cell`.
- `docs/提案书.md` §2.6 cell children, §2.13 spacer.
- `src/layout/decorate.rs` band-reject tests, only to see that they use `render` + `rec.commands().is_empty()` and are not cell/spacer tests.

### Boundaries

**What callers can put in a cell today (every public call site in `tests/document.rs`).**

| Test | Lines | Cell content |
| --- | --- | --- |
| `kind_a_flow_is_two_paragraphs_then_a_table` | 90–95, 105–126 | `t.header(["Item", "Amount"])`, `t.row([&name, &amount])` — `From<&str>` |
| `table_spans_and_repeat_header_switch` | 300–325 | strings, plus `Cell::new("Total").col_span(2)` |
| `cell_image_and_last_page_seal_share_coordinates` | 624–657 | `Cell::new("A")` and `Cell::image` (krilla feature) |
| `hline_in_list_item_and_cell` | 1193–1232 | list `item.hline`; table `Cell::hline(|h| h.color(Color::rgb(9, 9, 9)))` |
| `cell_and_layer_text_do_not_annotate` | 1985–2000 | `table.row(["cell"])` |

`Flow::spacer` appears only on the body: `keep_together_spacer_and_page_break` (328–347), `closure_errors_propagate_unchanged` (418–423, negative length), `hline_moves_whole_and_pins_stroke` (1135–1174). No test calls `spacer` inside `table`, `Cell`, or a list item.

**Layout allowlist tests (`src/layout/table.rs` 625–670).** `cell_rejects_columns_page_break_keep_together_and_tables` expects `Error::InvalidCellContent { container: "table cell", node }` for columns, page break, keep-together, nested table, and a `Stack` whose child is a page break. `cell_allows_hline` only does `Cell::new(vec![FlowNode::HLine(line)]).unwrap()`. The helper `cell()` (414–416) builds text cells. Nothing constructs `FlowNode::Spacer`.

**Is there a layout-level test that a cell can hold Spacer?** No. `rg` over `*.rs` finds `FlowNode::Spacer` only in `src/layout/flow.rs` (the node) and `src/layout/paginate.rs` `spacer_remainder_leads_next_page` (526–554), which paginates a body `Stack` of spacer then paragraph. `check_content` allows the node; no test passes it to `table::Cell::new` or `CellBody::new`. The list twin `item_allows_hline` (`src/layout/list.rs` 528–539) also has no spacer case. `item_rejects_table_page_break_and_image` (497–526) does not mention spacer.

**Assertion style knife 16 should copy.**

Public tests read recorded commands through `document(…).unwrap().pages()`, not by constructing `RecordingCanvas`. That is the same canvas: `document()` paints into one (`src/document.rs` 48–55) and `pages()` returns it (1520–1521). Helpers in `tests/document.rs`:

- `runs` / `texts` (63–82) keep `DrawGlyphRun` text and origin.
- Coordinates use `pt_near` (`kind_a` 123–125, `keep_together_spacer` 347) or exact `PathVerb` / `Point::try_pt` (`hline_in_list_item_and_cell` 1225–1231, `hline_moves_whole` 1167–1172).
- Errors use `assert_eq` on `Error` (`nesting_rules_come_from_layout` 380–395, `list_item_and_overwide_image_fail` 718–731).
- `to_pdf` smoke is `starts_with(b"%PDF")` and is feature-gated (`hline_writes_a_pdf` 1234–1244). A spacer does not need that.

A spacer paints nothing (`SpacerNode::split` 190–193, `commands: Vec::new()`), so the hline stroke filter is the wrong primary assertion. `hline_in_list_item_and_cell` is the right *call-site* shape (`doc.table` / `t.row` / `Cell::…` on a custom page) and the wrong *oracle*: it keeps `StrokePath` ops, asserts `strokes.len() >= 2`, and checks only the list line (`x = 18`, `y = 0.5`, length 40). The cell rule's color `rgb(9, 9, 9)` and its y are never read.

Copy the spacer *effect* oracles instead:

- Body, public: `keep_together_spacer_and_page_break` (343–347) — which page owns the following glyphs, then `pt_near` on that glyph's y. `hline_moves_whole_and_pins_stroke` (1153–1172) — the pushed page has the stroke; the previous page has none.
- Body, layout `RecordingCanvas`: `spacer_remainder_leads_next_page` (`src/layout/paginate.rs` 526–554). `RecordingCanvas::new()`, `paginate_root(…, &mut rec)`, `pages[0].ops().is_empty()`, next-page glyph y compared with `f32::to_bits()` against `30.0 + baseline` (80 pt spacer on a 50 pt page leaves 30 pt). This is the only existing spacer canvas test. It is not inside a cell, and it depends on `Partial` split, which `natural_height` rejects.
- Table geometry, layout: `src/layout/test_util.rs` `render` (54–64) returns `(Result, RecordingCanvas)`. Table tests then use `texts`, `bits` (`to_bits` on x and y), and `strokes` (`src/layout/table.rs` 451–470, 588–605). Error paths assert `rec.commands().is_empty()` (`header_plus_group_taller_than_page_errors_with_no_output` 548–549).
- Allowlist, no canvas: `cell_allows_hline` (659–670). A layout “cell holds Spacer” test would be this shape with `FlowNode::Spacer(SpacerNode { height })` and `.unwrap()`. That test is absent, so knife 16 cannot copy one; it would be new.

For a public `Cell::spacer`, the integration test should follow `tests/document.rs`: `document` → `pages()` → neighbor glyph y via `pt_near` or `runs`, or empty ops where the spacer contributes no ink. Layout `to_bits` belongs in `src/layout/table.rs` if the test builds a `TableNode` directly. Do not assert a `StrokePath` for the spacer itself.

`src/document.rs` 1885 `use std::cell::Cell` is the std once-cell used by `non_finite_rotation_does_not_draw`. It is not a table cell.

### Non-Obvious Things

- The allowlist and the DSL disagree. `check_content` already treats `Spacer` like `HLine` for `TableCell` (`lower.rs` 33–36). `cell_allows_hline` locks the hline arm; the spacer arm is untested and unreachable from `Table::lower`'s match (1007–1013), which is exhaustive over three variants.
- Every public cell is one node. Each match arm is `vec![single]`. A caller cannot put a spacer and a string in the same cell. §2.6 lists spacer next to paragraph and rule as cell children; the current `Cell` cannot express that list. Layout `Cell::new` accepts a `Vec`, including a `Stack`, but no test passes more than one allowed node except the reject case `Stack([PageBreak])`.
- `cell_rejects_…` does not prove spacer is allowed. Spacer is simply not in the rejected list. Absence from that array is not an assertion.
- Cell height and flow-spacer split are different mechanisms. `spacer_remainder_leads_next_page` shows a body spacer splitting 80 pt across a 50 pt page. The same node inside a cell is measured by `natural_height` (`table.rs` 217–222). A spacer taller than `full_height - 2*inset` becomes `UnsplittableTallerThanFrame` for the whole table, not a tail on the next page. Copying the paginate test's numbers into a cell would not describe current cell layout.
- Zero border plus a spacer-only cell draws nothing. `zero_border_draws_no_strokes` (609–622) already shows a text row with no strokes. A spacer row would also have no glyph runs. The observable is the y of whatever follows, or a later row's baseline, not an op of the spacer's own.
- `hline_in_list_item_and_cell` needs a font (`doc.font`) even though neither the list rule nor the cell rule draws glyphs. The cell is a one-column table (`t.row([one cell])`), so column count is 1 and the default rule width is the cell content width. The test never checks that.
- Image cells are feature-gated (`backend-krilla`) because `load_image` decodes pixels. `Cell::hline` and a future `Cell::spacer` do not need that feature; `hline_in_list_item_and_cell` runs without it.
- Negative height never reaches `SpacerNode`. `closure_errors_propagate_unchanged` (418–423) expects `Error::NegativeLength` from `NonNegative::pt(-1.0)` inside the document closure, before `Flow::spacer` is called. A cell spacer that takes `Length<NonNegative>` gets the same constructor rejection, with no cell-specific error.
- `nesting_rules_come_from_layout` (380–395) shows the public surface of the allowlist: a column `page_break` fails as `InvalidCellContent { container: "column", node: "page break" }`. There is no matching public test that a cell rejects columns or that a cell accepts a spacer. Cell rejection today cannot be triggered from the DSL, because the three constructors never emit a forbidden node.

### Open Questions

- A public test can show a spacer-only cell only by the y of a following row or a following paragraph. No existing table test does that for blank height. `borders_stroke_each_cell_centered_on_grid_lines` (`table.rs` 552–606) is the closest: row height is `line_height + 2*padding + border`, and glyph y is `margin + inset + baseline`. A spacer test would add `height` into that sum. Whether knife 16 should do that on `RecordingCanvas` in `table.rs`, or only via `document().pages()` and `pt_near`, is not settled by current tests.
- §2.13 says a flow spacer may split. §2.6 says a cell may contain a spacer, and a row does not split through the middle. No test combines those. It is unproven whether a cell spacer that fits in the row but makes the row taller than the page is `UnsplittableTallerThanFrame` (the `natural_height` path) or moves the whole row (`split_measured`, `table.rs` 340–390).
- One `CellContent` arm cannot hold “paragraph and spacer” together. Tests never build a multi-node public cell. If callers need a spacer *between* other cell children, `Cell::spacer` as a whole-cell constructor (the same shape as `Cell::hline`) is a different API from a nested `Flow`.
- `cell_allows_hline` does not paginate. An allowlist `.unwrap()` would not show that the spacer survived `measure`. A layout test that a cell *holds* a spacer and a layout test that the spacer *changes row y* are both missing; only the first is a one-line copy of `cell_allows_hline`.
