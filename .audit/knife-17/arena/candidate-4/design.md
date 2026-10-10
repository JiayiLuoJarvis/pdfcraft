# Knife 17 — Cell Rich Paragraph + List (Candidate 4 Design)

**Authority.** `docs/提案书.md` §2.6 (单元格孩子：段落、列表、填空、线、spacer) & §2.8 (格内列表).  
**Baseline.** Stack on `feat/cell-spacer` tip (includes Knife 16 `Cell::spacer`).  
**Scope.** Public DSL expansion in `src/document.rs`: `Cell::paragraph`, `Cell::ul`, and `Cell::ol`. Reuses existing `Paragraph` and `List` infrastructure. No modifications to `src/layout/` allowlist matrix or pagination algorithms. No second PDF backend; zero krilla leakage outside `backend/`.

---

## 1. Caller Usage Sketch

Callers construct rich paragraph cells and list cells directly using familiar closure builders, sitting cleanly alongside plain text `Cell::new`, `Cell::hline`, `Cell::spacer`, and `Cell::image`.

### 1.1 Rich paragraph in table cell

Reuses `Paragraph`'s public methods (`text`, `bold`, `blank*`, `link`):

```rust
use quire::{document, Cell, Color, Length, PageSize, NonNegative};

document(PageSize::a4(), |doc| {
    doc.font(FONT.to_vec())?;
    doc.bold_font(BOLD.to_vec())?;

    doc.table(|t| {
        t.header([
            Cell::new("Item"),
            Cell::new("Description / Details"),
        ]);
        t.row([
            Cell::new("Contract A-1"),
            Cell::paragraph(|p| {
                p.text("Base fee: ");
                p.bold("￥1,200.00");
                p.blank_em(1);
                p.link("https://example.com/terms", |p| {
                    p.text("[Terms]");
                });
            }),
        ]);
        Ok(())
    })?;
    Ok(())
})?;
```

Plain text convenience remains completely intact:
```rust
Cell::new("Plain text shorthand"); // Lowers via Flow::text -> TextNode::new
```

### 1.2 Unordered and ordered lists in table cell

Reuses `List` and item `Flow` (`item`, `item_marked`, nested `ul`/`ol`):

```rust
use quire::{document, Cell, Numbering, PageSize};

document(PageSize::a4(), |doc| {
    doc.table(|t| {
        t.row([
            Cell::new("Deliverables"),
            Cell::ul(|list| {
                list.item(|item| {
                    item.paragraph(|p| p.text("Draft specifications"));
                });
                list.item_marked("–", |item| {
                    item.paragraph(|p| {
                        p.text("Design review ");
                        p.bold("(Urgent)");
                    });
                    item.ul(|nested| {
                        nested.item(|i| i.paragraph(|p| p.text("Architecture freeze")));
                    });
                });
            }),
        ]);
        t.row([
            Cell::new("Milestones"),
            Cell::ol(Numbering::UpperRoman, |list| {
                list.item(|item| item.paragraph(|p| p.text("Kickoff")));
                list.item(|item| item.paragraph(|p| p.text("Acceptance")));
            }),
        ]);
        Ok(())
    })?;
    Ok(())
})?;
```

### 1.3 Nested validation & error handling

- **Whole cell rejections:** Attempting to put `columns`, `page_break`, `keep_together`, or a nested `table` in a cell remains forbidden by `check_content(Container::TableCell)` -> returns `Error::InvalidCellContent { container: "table cell", node: ... }`.
- **List item rejections:** Inside `list.item(|item| ...)`, calling `columns`, `table`, `page_break`, `keep_together`, or `image` is rejected by `check_content(Container::ListItem)` -> returns `Error::InvalidCellContent { container: "list item", node: ... }`.
- **Multi-kid cells:** Out of scope for this knife. Exactly one primary payload per public `Cell` (no `Cell::stack` or mixed sequential siblings in a single cell).

---

## 2. Types, Signatures & Lowering Pipeline

### 2.1 Shared paragraph lowering helper

To prevent semantic divergence between body paragraphs and table cell paragraphs, extract the lowering sequence into a private helper function in `src/document.rs`:

```rust
// In src/document.rs
fn lower_paragraph(paragraph: Paragraph, parent_style: Style) -> Result<FlowNode, Error> {
    if let Some(err) = paragraph.error {
        return Err(err);
    }
    let mut style = parent_style;
    style.first_line_indent = paragraph.first_line_indent;
    Ok(FlowNode::Text(TextNode::from_items(paragraph.items, style)))
}
```

Refactor `Flow::paragraph` to delegate to this helper:
```rust
pub fn paragraph(&mut self, f: impl FnOnce(&mut Paragraph)) {
    let mut paragraph = Paragraph::default();
    f(&mut paragraph);
    let node = lower_paragraph(paragraph, self.style.clone());
    self.push(node);
}
```

**Crucial node invariant:** This emits `FlowNode::Text`, **not** `FlowNode::Paragraph`. Line breaking and wrap width depend on the cell's `inner_w`, which is resolved during layout table measurement (`TextNode::split`). Emitting `FlowNode::Paragraph` would wrap prematurely before the column width is established.

### 2.2 `CellContent` and `ListSeed`

Extend `CellContent` in `src/document.rs`:

```rust
enum CellContent {
    Text(String),
    Image(Image, ImageBox),
    HLine(HLine),
    Spacer(Length<NonNegative>),
    Paragraph(Paragraph),
    List(ListSeed),
}

struct ListSeed {
    kind: ListKind,
    build: Box<dyn FnOnce(&mut List) + 'static>,
}

impl std::fmt::Debug for ListSeed {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ListSeed")
            .field("kind", &self.kind)
            .finish_non_exhaustive()
    }
}
```

### 2.3 `Cell` constructors

Add public constructors to `Cell` in `src/document.rs`:

```rust
impl Cell {
    /// A one-by-one cell holding a rich paragraph (supporting bold, blanks, links, and indent).
    pub fn paragraph(f: impl FnOnce(&mut Paragraph)) -> Self {
        let mut paragraph = Paragraph::default();
        f(&mut paragraph);
        Self {
            content: CellContent::Paragraph(paragraph),
            col_span: NonZeroU16::MIN,
            row_span: NonZeroU16::MIN,
        }
    }

    /// A one-by-one cell holding an unordered (bulleted) list.
    pub fn ul(f: impl FnOnce(&mut List) + 'static) -> Self {
        Self {
            content: CellContent::List(ListSeed {
                kind: ListKind::Bullet,
                build: Box::new(f),
            }),
            col_span: NonZeroU16::MIN,
            row_span: NonZeroU16::MIN,
        }
    }

    /// A one-by-one cell holding an ordered list starting at 1.
    pub fn ol(numbering: Numbering, f: impl FnOnce(&mut List) + 'static) -> Self {
        let style = match numbering {
            Numbering::Decimal => MarkerKind::Decimal,
            Numbering::UpperAlpha => MarkerKind::UpperAlpha,
            Numbering::LowerAlpha => MarkerKind::LowerAlpha,
            Numbering::UpperRoman => MarkerKind::UpperRoman,
            Numbering::LowerRoman => MarkerKind::LowerRoman,
        };
        Self {
            content: CellContent::List(ListSeed {
                kind: ListKind::Ordered {
                    style,
                    start: NonZeroU32::MIN,
                },
                build: Box::new(f),
            }),
            col_span: NonZeroU16::MIN,
            row_span: NonZeroU16::MIN,
        }
    }
}
```

### 2.4 `Cell` derives adjustment

Currently, `Cell` derives `#[derive(Debug, Clone, PartialEq, Eq)]`.
- `Paragraph` holds `Vec<InlineItem>` (not `Eq`), `Option<Error>` (not `Eq`), and `Paragraph` is not `Clone`.
- `ListSeed` holds `Box<dyn FnOnce(&mut List) + 'static>`, which cannot implement `Clone`, `PartialEq`, or `Eq`.
- Therefore, `Cell` must adjust derives to:
  ```rust
  #[derive(Debug)]
  #[must_use]
  pub struct Cell { ... }
  ```
- Derive `Debug` on `Paragraph` (all its fields implement `Debug`).
- Manual `Debug` on `ListSeed`.
- Auditing the entire workspace verifies that `Cell` is never cloned or checked for equality (`==`) anywhere in `src/` or `tests/`.

### 2.5 Lowering inside `Table::lower`

In `Table::lower(self, flow: &Flow)` (`src/document.rs`):

```rust
let content = match cell.content {
    CellContent::Text(text) => vec![flow.text(text)],
    CellContent::Image(image, image_box) => {
        vec![image_box.lower(&image, flow.doc)?]
    }
    CellContent::HLine(line) => vec![line.lower()],
    CellContent::Spacer(height) => {
        vec![FlowNode::Spacer(SpacerNode { height })]
    }
    CellContent::Paragraph(paragraph) => {
        vec![lower_paragraph(paragraph, flow.style.clone())?]
    }
    CellContent::List(seed) => {
        let mut list = List {
            parent: flow.child(),
            items: Vec::new(),
        };
        (seed.build)(&mut list);
        let node = list.lower(seed.kind)?;
        vec![node]
    }
};
Ok(table::Cell::new(content)?.with_span(cell.col_span, cell.row_span))
```

---

## 3. Module Map

| Module | Responsibility & Changes |
| --- | --- |
| `src/document.rs` | **Primary change site.**<br/>- Add `lower_paragraph` helper.<br/>- Add `CellContent::Paragraph` and `CellContent::List`.<br/>- Add `ListSeed` with manual `Debug`.<br/>- Add `Cell::paragraph`, `Cell::ul`, `Cell::ol`.<br/>- Update `Table::lower` match arms.<br/>- Derive `Debug` on `Paragraph`.<br/>- Change `Cell` derives to `#[derive(Debug)]`. |
| `src/layout/lower.rs` | **Zero changes.** `check_content(Container::TableCell)` already allows `Text` and `List`. `Container::ListItem` already validates item children. |
| `src/layout/table.rs` | **Zero changes.** `Cell::new` receives `Vec<FlowNode>`; `measure` applies `natural_height` on `body.into_node()`. |
| `src/layout/paragraph.rs` | **Zero changes.** `TextNode::from_items` wraps at `avail.width` (`inner_w`); blanks, bold, and links are laid out identically to body flow. |
| `src/layout/list.rs` | **Zero changes.** `ListNode::split` uses `inner_w`; hanging indent and marker generation apply without table-specific branches. |
| `src/layout/flow.rs` | **Zero changes.** `natural_height` Fit-only enforcement remains intact. |
| `src/lib.rs` | **Zero changes.** `Cell`, `Paragraph`, `List`, and `Numbering` are already publicly exported. |
| `tests/document.rs` | **Verification suite.** Comprehensive public recording tests asserting fonts, blanks, link rects, markers, error bailouts, and pagination. |

---

## 4. List ↔ Flow Seam Choice

### 4.1 Evaluation of candidates

1. **Defer list builder closure until `Table::lower(&Flow)` (Chosen default):**
   - `Cell::ul` and `Cell::ol` store a `ListSeed` carrying `Box<dyn FnOnce(&mut List) + 'static>`.
   - When `Table::lower` executes, it holds `&Flow`. It calls `flow.child()`, initializes `List`, runs the stored closure, and calls `list.lower()`.
   - **Advantages:** Clean, elegant public syntax `t.row([Cell::ul(...), Cell::new(...)])`; completely preserves `List::item` and nested `Flow` ergonomics; style and `DocToken` are inherited faithfully; zero changes to `Table` builder signatures.
   - **Trade-off:** Requires `'static` on the boxed closure. In PDF generation DSLs, text runs and cell values are either literals or owned values, easily satisfying `'static`.

2. **Give `Table` a child `Flow` (`table-child-flow`):**
   - `Flow::table` stores `parent: self.child()`. Cells containing lists must be built via `Table` methods, e.g., `t.ul_cell(...)` or by passing `&mut Table` into cell constructors.
   - **Drawbacks:** Breaks the orthogonal design where `Cell` is an independent value passed via `Into<Cell>`. Clumsy borrow mechanics when iterating rows.

3. **Synthesize fake `Flow::new(default_style(), DocToken::next())` inside `Cell::ul`:**
   - **Drawbacks:** Creates an orphaned `Flow` outside the document hierarchy. Breaks `DocToken` monotonicity and diverges from document styling. Explorer explicitly condemns this shortcut.

4. **Declarative AST without closures:**
   - **Drawbacks:** Massive refactor. Introduces a second list-item representation duplicating item validation.

### 4.2 Conclusion

The deferred closure pattern (`ListSeed`) in `Table::lower` is the minimal, most idiomatic seam.

---

## 5. Geometry, Layout & `natural_height` Notes

### 5.1 Content width and wrapping

In `layout/table.rs`:
- For each placed cell, `inner_w = spanned_col_width - 2 * inset`. In current public tables, `border = 0` and `padding = 0`, so `inset = 0` and `inner_w = spanned_col_width`.
- When a rich paragraph cell splits (`TextNode::split`), it lays out runs at `inner_w`.
- First-line indent reduces line 1 budget: `inner_w - first_line_indent`. If non-positive, layout returns `Error::InvalidGeometry`.
- `blank_to_end` fills remaining width up to `inner_w`.
- Links receive translation offsets matching the cell's final placement on the page: `(col_x + inset, row_y + inset)`.

### 5.2 Fit-only contract of `natural_height`

- Table cell measurement evaluates:
  ```rust
  natural_height(body.into_node(), ctx, cell_frame)
  ```
- `cell_frame.height` is `inner_full` (the full page body height minus insets).
- `natural_height` accepts **only** `Split::Fit`. If cell content produces `Split::Partial` or `Split::MoveToNextPage`, `natural_height` immediately maps it to `Error::UnsplittableTallerThanFrame`.
- **Row-level pagination invariant (§2.6):** A table cell **never** splits across pages. If the cell fits within a full page frame, the whole row can paginate to the next page if needed. If the cell content exceeds the full page body height, the entire document layout aborts with `UnsplittableTallerThanFrame`.
- Although §2.8 allows body list items to split across pages, inside a table cell the §2.6 physical-row boundary strictly takes precedence.

### 5.3 Geometric limits on list cells

- `ListSpec` hardcodes `marker_width = 18.0 pt`.
- `ListNode::split` checks `marker_width >= avail.width` (i.e. `18.0 >= inner_w`). If a cell column is 18.0 pt or narrower, list layout yields `Error::InvalidGeometry`. This is expected and identical to body lists.

---

## 6. Comprehensive Test Plan

Testing relies on `document().pages()` and `RecordingCanvas` inspection in `tests/document.rs`, checking font IDs, coordinates, and error bailouts without brittle PDF byte assertions.

### 6.1 Paragraph cell tests (17a)

| Test Name | Verification Target | Assertion Strategy |
| --- | --- | --- |
| `cell_paragraph_bold_font_id` | `p.bold` uses bold font | Assert glyph run in cell uses registered `doc.bold_font` id. |
| `cell_paragraph_blank_to_end_cell_width` | `blank_to_end` spans cell width | Inspect `CanvasCommand::Stroke` x-endpoints against cell column width. |
| `cell_paragraph_link_rect_offset` | `p.link` rect matches cell position | Verify link annotation bounds match cell `(x + dx, y + dy)`. |
| `cell_paragraph_empty_link_bailout` | `p.link("", ...)` records `EmptyLink` | Assert doc returns `Err(Error::EmptyLink)`; zero canvas output. |
| `cell_paragraph_run_after_fill_blank` | Error after `blank_to_end` | Assert `Err(Error::RunAfterFillBlank)`; zero canvas output. |
| `cell_plain_text_parity` | `Cell::new` unchanged | Verify single text run, body font, unchanged position. |

### 6.2 List cell tests (17b)

| Test Name | Verification Target | Assertion Strategy |
| --- | --- | --- |
| `cell_ul_bullet_marker_and_text_offset` | Bullet marker at cell x; item text at `cell_x + 18` | Verify marker glyph x position and paragraph text x position. |
| `cell_ul_item_rich_features` | Items support bold, blank, and links | Assert bold font and link annotations work inside list items in a cell. |
| `cell_ul_nested_list_indent` | Nested list indents by 2x marker width | Assert nested bullet placed at `cell_x + 18` and inner text at `cell_x + 36`. |
| `cell_ol_numbering_marker` | `Numbering::UpperRoman` renders "I." | Assert marker glyph string corresponds to Roman numeral marker. |
| `cell_list_item_rejects_table` | `item.table(...)` rejected | Assert `Err(Error::InvalidCellContent { container: "list item", node: "table" })`. |
| `cell_list_item_rejects_image` | `item.image(...)` rejected | Assert `Err(Error::InvalidCellContent { container: "list item", node: "image" })`. |
| `cell_list_taller_than_frame_errors` | Over-tall list does not split mid-cell | Multi-item list exceeding page height yields `UnsplittableTallerThanFrame`. |

---

## 7. Out-of-Knife Boundaries

1. **Forbidden cell containers:** `columns`, `page_break`, `keep_together`, and nested `table` remain forbidden in cells (§2.6).
2. **Multi-kid cells / `Cell::stack`:** Public `Cell` holds exactly one payload. No mixed sequences (`paragraph + hline + list`) in a single cell.
3. **No mid-cell page splitting:** No modifications to `natural_height` to permit `Split::Partial` inside a cell.
4. **List customizations:** No custom marker widths, no start numbering other than 1, no reverse numbering (§2.8).
5. **No second PDF backend:** Backend isolation strictly maintained; no krilla types exposed.

---

## 8. Slicing Strategy: 17a / 17b

- **Decomposition:**
  - **17a (Paragraph):** `lower_paragraph` helper + `Cell::paragraph` constructor + `Table::lower` paragraph arm + paragraph tests + `Cell` derive adjustment.
  - **17b (List):** `ListSeed` + `Cell::ul`/`Cell::ol` constructors + `Table::lower` list arm + list tests.
- **Delivery timeline:** Both 17a and 17b are compact and can easily land together within ~one evening as **two sequential commits on one branch**:
  1. `feat(document): Cell::paragraph rich text support`
  2. `feat(document): Cell::ul and Cell::ol table cell list support`
- **Fallback:** If list closure capture ergonomics require unexpected refinement, 17a is independently shippable without holding up rich paragraph cells.
