# Knife 17 rationale

Chosen shape: one public payload per `Cell`. `Cell::paragraph` runs a `Paragraph` immediately and stores it. `Table::lower` feeds that value through the same helper as `Flow::paragraph`, which emits `FlowNode::Text(TextNode::from_items(...))` using the enclosing flow style plus `first_line_indent`. `Cell::ul` / `Cell::ol` store a `'static` `FnOnce(&mut List)` and `Table::lower` runs it on `flow.child()`, then `List::lower`, same as `Flow::list`. `Cell::new` stays `TextNode::new`. `natural_height` stays Fit-only.

`Cell` drops `Clone + PartialEq + Eq` because the list arm is a `FnOnce` box. `Debug` is manual. No new public type.

Frame reverse words that were available and not taken: `items-only`, `rich-new`, `text-only`, `list-closure-on-row`, `table-child-flow`, `force-both`, `stack-cell`, `split-in-cell`.

## Paragraph closure (`items-only`)

`Flow::paragraph` and `Cell::hline` already take `FnOnce` and run it before returning. `Paragraph` is the public rich-text builder (`text`, `bold`, `blank`, `blank_em`, `blank_to_end`, `link`, `first_line_indent`) and it does not need a `Flow`. An items-only constructor (`Cell::from_items`, or a public `Vec` of runs) would skip `append`'s coalesce, empty-string drop, and `RunAfterFillBlank` / `EmptyLink` recording. Those errors live on `Paragraph.error`. Storing only `items` would drop them. The closure result is the whole `Paragraph`.

## Plain `Cell::new` (`rich-new`)

`CellContent::Text` lowers through `Flow::text` → `TextNode::new`: one body-face run, parent style, no blank, no link. `t.row(["A", "B"])` and `From<&str/String/&String>` depend on that. Making `Cell::new` rich would change every existing text cell and would still not cover blanks and links without a builder. Rich text is a new arm.

## Bold, blank, link (`text-only`)

Those three are already `InlineItem` / `TextRun.link` / `Face::Bold`. `Document::bold_font` resolves `Face::Bold` at layout. A cell-local bold method, a cell-local blank, or a cell-local link would duplicate `Paragraph` and could drift from body coalesce and fill-blank rules. `Cell::paragraph` only has to keep the `Paragraph` intact until `lower_paragraph`.

`FlowNode::Paragraph` is the wrapped line list. `measure` learns `inner_w` later (`spanned width - 2 * inset`; public inset is 0). Emitting the wrapped node from the DSL would freeze the wrong width, so fill blanks and link rects would not land on the cell edge. `lower_paragraph` therefore matches `Flow::paragraph` line for line: error first, then `from_items`.

## List spelling (`list-closure-on-row`)

§2.8's item body is a `Flow`: `item`, `item_marked`, nested `ul` / `ol`, and `paragraph` inside the item. `Flow::ul(|list| ...)` and `Flow::ol(numbering, |list| ...)` are the public names. `Cell::ul` / `Cell::ol` use those names and that `List`. A `Table::ul_cell(&mut self, ...)` factory would be a second list entry point, and it cannot be written next to `Cell::hline` before the cell is attached to a row. `Numbering` maps through one private `numbering_kind`; `start` stays `NonZeroU32::MIN`, matching `Flow::ol`.

## Defer until `Table::lower` (`table-child-flow`)

`List` requires a parent `Flow` when `item` runs. `Cell::ul` is a free function, and `Table` does not hold a `Flow` (`Columns` and `List` do). The enclosing flow exists as `&Flow` on `Table::lower`. Storing the closure and invoking `assemble_list(flow.child(), kind, f)` there reuses `Flow::nested`, style, `DocToken`, and `ListItem::new` without a second item API.

Giving `Table` a child `Flow` (`table-child-flow`) does not make a context-free `Cell::ul` work. The closure would still have to run on a method that can see that field, which is the row-builder API above. Building the list from `Flow::new(default_style(), doc)` inside `Cell::ul` matches today's fonts only because nothing assigns `Flow.style` after `Flow::new`. `Cell::new` already takes style at lower time. A list that bypasses `flow.child()` would diverge on the first real style write.

A declarative list value (owned items, no closure, no `Flow`) avoids `FnOnce` and could keep `Clone`. It is the large third option: a second assembly path beside `List::item`, easy to get a different `InvalidCellContent` container, and it reopens multi-kid cells. Out of an evening, and not one of the named reverses.

`'static` is the bound inside the defer default, not a reverse. `lower` runs after the `table` closure returns, so a local born in that closure cannot be borrowed no matter what. A lifetime on `Cell` would allow borrowing an outer local, and it would also make `let c = Cell::new("A")` fail to infer: `'a` is not determined by `new`'s inputs, and `Into<Cell<'a>>` would not unify today's mixed rows. `move` of an owned `String` covers the outer-local case and leaves `Cell::new`, `From<&str>`, and `col_span` lifetime-free. String literals are already `'static`.

The derive drop is the other measured cost of this seam. `InlineItem` is `PartialEq` but not `Eq`, and `Error` is not `Eq`, so a `Paragraph` arm already removes `Eq`. The `FnOnce` box also removes `Clone` and `PartialEq`. In-tree callers never clone or compare `document::Cell`. Hand `Debug` still prints spans, the paragraph, and the list kind. Implementing `Clone` by panicking on the list arm would be a trap. `Cell<F>` with a generic closure cannot sit in one array beside `Cell::new`.

## One knife (`force-both`)

`force-both` would mean shipping a design that does not fit an evening. This one does: helpers are moves of `Flow::paragraph` and `Flow::list`, the lower match gains two arms, layout files stay closed, tests are recording assertions with fixtures that already exist (`FONT`, `LINE`, `BASELINE`, `runs` / `links` / `stroke_samples`). Two commits on one branch (paragraph, then list) isolate the derive drop. They are not a 17a/17b split. The knife includes the list arm; paragraph-only would leave §2.8's 格内列表 unexposed while the seam is already this small.

## One payload (`stack-cell`)

`CellBody` is a `Vec<FlowNode>` and `check_content` allows several legal children. Public `Table::lower` always passes `vec![one node]`, and knife 16 kept spacer as one arm. §2.6 lists paragraph and list as cell children, not as a sequence API. A `Cell` stack or a cell `Flow` closure would be a second cell model and would need a rule for mixed paragraph + list + spacer in one grid slot. Callers who need both a list and a paragraph use two cells, or a list whose items contain paragraphs. Adding the arms does not expose the internal vector.

## Too tall (`split-in-cell`)

`measure` calls `natural_height` with `height == full_height == inner full frame`. `natural_height` maps `Partial` and `MoveToNextPage` to `UnsplittableTallerThanFrame`. §2.6 splits tables on physical rows. §2.8 lets a body list item split; that path is `ListNode::split` when the frame is the leftover body, which a cell never passes in.

Keeping `Partial` inside `natural_height` would change every unsplittable cell child (image, hline, spacer, single-line text), not just a tall paragraph. Special-casing the paragraph or list arm to ignore `natural_height` would cut a row across a page boundary. A row that fits the full page but not the current remainder still moves in `split_measured`. The recording plan uses a page of height `LINE` and `"A\nB"`, which body flow already paginates, and expects the error and no pages. A two-item list on that page is the list `Partial` case.

## Forbidden kids and the list-item name

`TableCell` already rejects columns, page break, keep-together, and nested table. This knife adds no public method that builds those nodes as the cell payload, so it does not add a public `"table cell"` rejection test. The existing layout test remains that proof.

The item `Flow` inside `Cell::ul` can call `image`, `table`, `columns`, `page_break`, and `keep_together`. Those fail in `ListItem::new` → `check_content(Container::ListItem)`, whose container string is `"list item"`. Image is allowed on `Cell::image` and rejected on an item (`lower.rs` image arm is column or table cell only). A list path that validated item kids with the table-cell matrix would accept the image and would be wrong. The public test pins `container: "list item"` for that reason.

A cell list whose `inner_w` is not greater than 18 pt fails `InvalidGeometry` in `ListNode::split`. That is geometry, not the allowlist. Public inset is 0, so the page width is the inner width for a one-column table.
