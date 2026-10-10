# Knife 17 — cell rich paragraph + list (candidate 1)

**Scope.** Public DSL only: §2.6 cell children **段落（含填空/link/bold）** and §2.8 **格内列表**, reusing existing `Paragraph` and `List`. Layout allowlist, `TextNode::from_items`, list marker/hanging indent, and table row pagination stay unchanged.

**Baseline.** Stack on `feat/cell-spacer` tip (includes knife 16).

**Autonomy.** All frame defaults; no reverse words (`items-only`, `rich-new`, `text-only`, `list-closure-on-row`, `table-child-flow`, `force-both`, `stack-cell`, `split-in-cell`).

---

## 1. Caller usage sketch

Rich paragraph beside existing `Cell::new`, `Cell::hline`, `Cell::spacer`, `Cell::image`:

```rust
use quire::{document, Cell, PageSize, /* FONT, helpers as in tests/document.rs */};

document(PageSize::a4(), |doc| {
    doc.font(FONT.to_vec())?;
    doc.bold_font(BOLD.to_vec())?;
    doc.table(|t| {
        t.row([
            Cell::paragraph(|p| {
                p.text("Amount: ");
                p.bold("100");
                p.blank_em(2);
                p.link("https://example.test", |p| p.text("details"));
            }),
            Cell::new("plain still works"),
        ]);
        Ok(())
    })?;
    Ok(())
})?;
```

Plain string shorthand unchanged:

```rust
Cell::new("Total")  // still Flow::text → TextNode::new at lower
```

Unordered / ordered list mirrors body `Flow::ul` / `Flow::ol` spelling:

```rust
doc.table(|t| {
    t.row([Cell::ul(|list| {
        list.item(|item| {
            item.paragraph(|p| p.text("first line"));
        });
        list.item_marked("–", |item| {
            item.paragraph(|p| p.bold("marked"));
            item.ul(|nested| {
                nested.item(|i| i.paragraph(|p| p.text("nested")));
            });
        });
    })]);
    t.row([Cell::ol(Numbering::UpperRoman, |list| {
        list.item(|item| item.paragraph(|p| p.text("I")));
    })]);
    Ok(())
})?;
```

Forbidden content is still rejected by layout, not by extra cell methods:

- Whole cell: `columns`, `pageBreak`, `keep_together`, nested `table` → `InvalidCellContent { container: "table cell", … }`.
- Inside list items: same structural escapes → `container: "list item"` (existing tests pattern).

No mixed sequence in one public cell (no `paragraph + spacer + list` in a single `Cell`).

---

## 2. Types, signatures, lowering

### Shared paragraph lowering (new private helper)

Extract the three steps shared by `Flow::paragraph` and `Table::lower`:

```rust
// src/document.rs (same module as Flow / Paragraph)
fn lower_paragraph(
    paragraph: Paragraph,
    parent_style: Style,
) -> Result<FlowNode, Error> {
    if let Some(error) = paragraph.error {
        return Err(error);
    }
    let mut style = parent_style;
    style.first_line_indent = paragraph.first_line_indent;
    Ok(FlowNode::Text(TextNode::from_items(paragraph.items, style)))
}
```

`Flow::paragraph` becomes a thin wrapper: build `Paragraph`, run closure, `self.push(lower_paragraph(...))`.

**Important:** emit `FlowNode::Text`, never `FlowNode::Paragraph`. Wrapping happens at cell `inner_w` during table measure.

### `CellContent` arms

```rust
// Paragraph: same pattern as Cell::hline — closure runs at construct, value stored.
enum CellContent {
    Text(String),
    Image(Image, ImageBox),
    HLine(HLine),
    Spacer(Length<NonNegative>),
    Paragraph(Paragraph),           // NEW
    List(ListCellSeed),             // NEW — see below
}

impl Cell {
    pub fn paragraph(f: impl FnOnce(&mut Paragraph)) -> Self { /* … */ }
    pub fn ul(f: impl FnMut(&mut List) + 'static) -> Self { /* … */ }
    pub fn ol(numbering: Numbering, f: impl FnMut(&mut List) + 'static) -> Self { /* … */ }
}
```

`Cell::paragraph`: `Paragraph::default()`, run `f`, store `Paragraph` (items + `first_line_indent` + `error`). Do **not** strip `error`; lower must match body semantics (`EmptyLink`, `RunAfterFillBlank`).

`ListCellSeed` (private):

```rust
struct ListCellSeed {
    kind: ListKindTag, // Ul | Ol(Numbering) — same mapping as Flow::ol match
    build: Box<dyn FnMut(&mut List) + 'static>,
}
```

Rust cannot store `FnOnce(&mut List)` in `Cell` without generics on `Table`/`Cell`. **`FnMut + 'static` in a box** is the storable equivalent of “run once at lower”: `Table::lower` calls `build` exactly once, then drops the seed. Callers capturing `String`/`Vec` from the table closure satisfy `'static` when owned.

At `Table::lower` for `CellContent::List(seed)`:

```rust
let mut list = List {
    parent: flow.child(),
    items: Vec::new(),
};
(seed.build)(&mut list);
let node = list.lower(seed.kind.into()); // same List::lower as Flow::list
vec![node]
```

Paragraph arm:

```rust
CellContent::Paragraph(p) => {
    vec![lower_paragraph(p, flow.style.clone())?]
}
```

### `Cell` derives

Current `Cell: Debug + Clone + PartialEq + Eq` conflicts with stored `Paragraph`, `Box<dyn FnMut …>`, and non-`Eq` inline items.

**Plan:** drop `Clone`, `PartialEq`, and `Eq` from `Cell` (keep `Debug` via manual impl or `#[derive(Debug)]` on arms that support it — list arm debug as `"ListCellSeed { .. }"`). No in-tree `Cell == Cell`; document in CHANGELOG as 0.x API tweak.

Spans unchanged: `col_span` / `row_span` remain on `Cell`.

### `Table::lower`

Extend the existing per-cell `match content` only; `table::Cell::new(vec![one node])` path unchanged.

---

## 3. Module map

| Location | Change |
| --- | --- |
| `src/document.rs` | `lower_paragraph`; `CellContent` arms; `Cell::paragraph` / `ul` / `ol`; `ListCellSeed`; `Flow::paragraph` refactor; `Table::lower` arms; optional `Cell` `Debug` |
| `src/layout/lower.rs` | **No allowlist change** (`Text`, `List` already allowed for `TableCell`) |
| `src/layout/paragraph.rs` | **No change** (bold/blanks/links already on `TextNode::from_items`) |
| `src/layout/list.rs` | **No change** (item validation, markers, nest) |
| `src/layout/table.rs` | **No change** (`natural_height` on `body.into_node()`) |
| `src/layout/flow.rs` | **No change** (`natural_height` Fit-only) |
| `src/lib.rs` | Re-exports unchanged (`Paragraph`, `List`, `Numbering` already public) |
| `tests/document.rs` | New recording tests (below); no layout-only substitute for public API |

Dependency direction unchanged: no `krilla` outside `backend/`.

---

## 4. List ↔ Flow seam choice

**Chosen (autonomy default): defer list builder until `Table::lower(&Flow)`.**

| Option | Verdict |
| --- | --- |
| Defer closure at lower | **Yes.** Preserves `Cell::ul` / `Cell::ol` beside `Cell::hline`; reuses `List::item` → `Flow::nested` → `ListItem::new` unchanged; style/`DocToken` from `flow.child()` match body lists today. |
| `Table` holds child `Flow` (`Columns` pattern) | **No.** Forces list cells through `Table` methods; breaks free `Cell::ul` in `t.row([…])`. |
| Declarative multi-kid cell builder | **No.** Largest surface; duplicates item validation; opens multi-payload cell (out of knife). |

Paragraph path does not need a `Flow` at construct time (same as `Cell::hline`).

List path needs `Flow` only when `List::item` runs; lower-time `flow.child()` is the smallest bridge.

**Rejected shortcut:** `Flow::new(default_style(), DocToken::next())` inside `Cell::ul` — wrong token boundary and diverges when `Flow.style` becomes mutable/inherited.

---

## 5. `natural_height` and pagination notes

Table measure (`layout/table.rs`): for each placed cell, `inner_w` = spanned column width minus inset; frame height = full inner page body minus inset; `natural_height(body.into_node(), …)`.

`natural_height` (`layout/flow.rs`) keeps only `Split::Fit`. Any child that would return `Partial` or `MoveToNextPage` in body flow becomes **`Error::UnsplittableTallerThanFrame`** in a cell.

Consequences for this knife:

| Content | Body flow | Cell |
| --- | --- | --- |
| Wrapping paragraph | May split across pages | Must fit full cell inner height or error |
| Multi-item list | Items / fragments may split (§2.8) | Entire list must `Fit` inner full frame or error |
| Spacer / hline / image | Existing rules | Unchanged |

Row pagination unchanged: whole physical rows (and row-span groups) move to the next page; no mid-cell continuation on a later page.

Geometric failures independent of allowlist:

- Narrow cell: public `marker_width` 18 pt; if `inner_w <= marker_width`, list layout → `InvalidGeometry` (same as body).
- Paragraph first-line indent consuming line width → `InvalidGeometry` at wrap time using cell `inner_w`.

Document in `Cell::paragraph` / `Cell::ul` rustdoc: taller-than-frame content errors; do **not** change `natural_height` to admit mid-cell `Partial`.

---

## 6. Test plan

Prefer **`document().pages()` + `RecordingCanvas`** (existing helpers in `tests/document.rs`). Assert no paint on hard errors.

### 17a — paragraph cell (can land first)

| Test | Assert |
| --- | --- |
| `cell_paragraph_bold_uses_bold_font_id` | Glyph ops in cell use bold font id where `p.bold` used |
| `cell_paragraph_blank_to_end_strokes_cell_width` | Fill blank stroke endpoints span cell content width (column-aware) |
| `cell_paragraph_link_rect_offset` | Link annotation rect includes cell origin offset |
| `cell_paragraph_empty_link_no_output` | `EmptyLink`; no glyphs |
| `cell_paragraph_run_after_fill_blank` | Same error as body; no output |
| `cell_plain_new_unchanged` | `Cell::new` still body face only |

Optional layout supplement: `table::Cell::new(vec![text from from_items])` — not a substitute for public API tests.

### 17b — list cell

| Test | Assert |
| --- | --- |
| `cell_ul_marker_and_text_x` | Marker x ≈ `cell_x`; text x ≈ `cell_x + 18` (public marker width) |
| `cell_ul_item_paragraph_bold_and_link` | Reuse body patterns inside `list.item` |
| `cell_nested_ul_in_item` | Nested marker indent |
| `cell_ol_upper_roman_marker` | `Numbering::UpperRoman` in cell |
| `cell_list_item_table_rejected` | `InvalidCellContent { container: "list item", node: "table" }` |
| `cell_list_taller_than_frame` | `UnsplittableTallerThanFrame`; no second-page list continuation in cell |

Regression: existing cell hline/spacer/image tests; body ul/ol unchanged.

---

## 7. Out of knife

Per frame + proposal:

- Cell **columns**, **pageBreak**, **keep_together**, **nested table** (allowlist already rejects).
- **Multi-kid** public cell / `Cell::stack` / mixed paragraph+spacer+list.
- **Named styles**; changing `natural_height` for mid-cell `Partial`.
- **Configurable `ol` start** beyond what body `Flow::ol` already exposes (cell should not leapfrog body API).
- flow canvas, qr/svg/toc, encryption, second PDF backend.
- Layout allowlist edits “for symmetry” without behavior gap.

---

## 8. Split 17a / 17b?

| Piece | Effort | Dependency |
| --- | --- | --- |
| `lower_paragraph` + `Cell::paragraph` + lower arm + paragraph tests | Small (~half evening) | None |
| `ListCellSeed` + `Cell::ul`/`ol` + derive change + list tests | Medium (~half–1 evening) | Prefer landing derive decision in 17a |

**Recommendation:** **One knife, two commits** if implementer can finish in ~one evening: `(1) paragraph + shared helper + derive prep`, `(2) deferred list + list tests`. If list closure/`Cell` derive work slips, **ship 17a first** (paragraph-only public API); 17b adds list arms only, no layout changes.

Do **not** `force-both` in a single commit if list tests are unfinished — paragraph value is independently shippable per rubric laziness rule.

---

## 9. Implementation checklist (for implement PR, not this artifact)

1. Add `lower_paragraph`; refactor `Flow::paragraph`.
2. Add `CellContent::Paragraph`, `Cell::paragraph`, `Table::lower` arm.
3. Paragraph tests green; `cargo fmt`, clippy, test.
4. Add `ListCellSeed`, `Cell::ul`/`ol`, lower arm; adjust `Cell` derives.
5. List + over-height tests; full CI.
