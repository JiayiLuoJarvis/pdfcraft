# Knife 8 candidate 1 — Document builder owns the assemble tree; page-time hooks are Decorations

Baseline: `feat/document-dsl` @ `6dbf40c`. Authority: 提案书 > AGENTS.md > 设计稿技术事实。

## Problem

Knife 7 left a complete internal engine: `FlowNode::{Text, Paragraph, Spacer, PageBreak, KeepTogether, Stack, Columns, Table, List}`, `paginate_root(root, ctx, config, &Decorations, canvas)`, and margin-band `Decorations { header, footer }` measured before anything is painted. `document.rs` is still an empty placeholder. Knife 8 must expose the 提案书 §1 closure DSL on top of it without new split logic and without inventing the builders §2 left unconfirmed.

The constraints from the existing code that make the shape non-obvious:

- `TextNode` has **one** `Style`. Layout has no rich-text runs and no bold face, so §1's `p.bold(..)` has nothing to lower into.
- `BandContent = Box<dyn Fn(&PageInfo) -> Result<Vec<FlowNode>>>` is implicitly `'static`, but §1's header borrows `&company.name`. The closure box needs a lifetime.
- `check_content(Container::Band, ..)` rejects `Columns`. Laying a left slot and a right slot on one line is a two-column row.
- `Decorations` has no behind/front layers, and `paint_pages` paints header, then body, then footer. §2.12 fixes the layer order as behind → header → body → footer → front.
- `KrillaCanvas::finish` is the only path to PDF bytes and is gated behind `backend-krilla`. Tests must run the same layout into a `RecordingCanvas`.
- `Point` has `try_pt` / `try_mm` / `new(Length<Finite>, ..)`; there is no infallible `Point::new(f32, f32)` (per §2.0, constraints are typed first).
- The fixture font `tests/fixtures/quire-measure.ttf` covers ASCII, digits, U+2022, and 第 / 页 / 共. Tests use ASCII content plus a Chinese page-number string.

## Usage (caller's view)

Written first. Everything under `## Shape` is derived from these call sites. They are meant to land as-is as `tests/document.rs` (integration tests, public API only).

### Gaps from the 提案书 §1 sketch (each forced by an existing constraint)

| §1 sketch | This design | Why |
| --- | --- | --- |
| `document(PageSize::A4, \|doc\| { … })?` returns bytes | `document(A4, \|doc\| { …; Ok(()) })?.to_pdf()?` | The forced direction is a closure returning `Result<()>` so the caller can use `?` on fallible `quire` constructors (`NonNegative::pt`, `Point::try_pt`). Returning `Rendered` lets tests read the `RecordingCanvas` without `backend-krilla`. The `PageSize` enum is not pinned (the A-series list is still open), so this reuses `geom::A4`. |
| `p.bold(&contract.id)` | **omitted** | `TextNode` is single-style and no bold face is registered. Adding it means inventing rich-text runs. |
| `h.right(\|page\| format!(…page.number…))` | `h.right(format!(…, h.page().number, …))` | See "Band slots take strings" in Shape. The header closure already runs once per page, so `h.page()` carries the same information. |
| `Point::new(180.0, 400.0)` | `Point::try_pt(180.0, 400.0)?` | Matches existing `geom`. |
| `page.image_at(&seal, Rect…)` | **omitted** | Image layout is not ready (frame says skip). `front` exists and supports `text_at` only. |

### Quickstart (README shape)

```rust
use quire_pdf::{document, Margins, NonNegative, Point, A4};

let pdf: Vec<u8> = document(A4, |doc| {
    doc.font(std::fs::read("SourceHanSerif.otf")?)?;      // first font = default face
    doc.margins(Margins::uniform(NonNegative::mm(20.0)?));

    doc.header(|h| {
        h.left(&company.name);
        h.right(format!("第 {} 页 / 共 {} 页", h.page().number, h.page().count));
    });

    doc.paragraph(|p| p.text(format!("合同 {}", contract.id)));
    if contract.kind == Kind::A {
        doc.paragraph(|p| p.text("按甲方案执行。"));
        doc.table(|t| {
            t.header(["项目", "金额"]);
            for item in &contract.items {
                t.row([&item.name, &item.amount]);
            }
        });
    } else {
        doc.paragraph(|p| p.text("按乙方案执行。"));
    }

    doc.columns(|row| {
        row.col(|c| c.paragraph(|p| p.text(&contract.party_a)));
        row.col(|c| c.paragraph(|p| p.text(&contract.party_b)));
    });

    doc.behind(|page| page.text_at("DRAFT", Point::try_pt(180.0, 400.0).unwrap()));
    doc.front(|page| {
        if page.is_last() {
            page.text_at("SEAL", Point::try_pt(430.0, 680.0).unwrap());
        }
    });
    Ok(())
})?
.to_pdf()?;
```

(`std::io::Error` → `quire_pdf::Error` does not exist; the README loads the font outside the closure. The `?` above only marks the shape.)

### `tests/document.rs`

```rust
use quire_pdf::{
    document, pt_near, CanvasCommand, Cell, Error, Margins, NonNegative, Numbering, Point,
    RecordedPage, Rendered, Result, A4,
};

const FONT: &[u8] = include_bytes!("fixtures/quire-measure.ttf");

#[derive(PartialEq)]
enum Kind { A, B }
struct Item { name: String, amount: String }
struct Contract { id: String, kind: Kind, items: Vec<Item>, party_a: String, party_b: String }

fn contract(kind: Kind) -> Contract {
    Contract {
        id: "C-18".into(),
        kind,
        items: vec![
            Item { name: "Pen".into(), amount: "3".into() },
            Item { name: "Ink".into(), amount: "5".into() },
        ],
        party_a: "Party A".into(),
        party_b: "Party B".into(),
    }
}

fn texts(page: &RecordedPage) -> Vec<String> {
    page.ops().iter().filter_map(|op| match op {
        CanvasCommand::DrawGlyphRun { run } => Some(run.text.clone()),
        _ => None,
    }).collect()
}

fn runs(page: &RecordedPage) -> Vec<(String, f32, f32)> { /* (text, x, y) of each glyph run */ }

/// §1 first example: `if` picks which blocks enter the flow.
fn first_example(c: &Contract) -> Result<Rendered> {
    document(A4, |doc| {
        doc.font(FONT.to_vec())?;
        doc.paragraph(|p| p.text(format!("Contract {}", c.id)));
        if c.kind == Kind::A {
            doc.paragraph(|p| p.text("Plan A."));
            doc.table(|t| {
                t.header(["Item", "Amount"]);
                for item in &c.items {
                    t.row([&item.name, &item.amount]);
                }
            });
        } else {
            doc.paragraph(|p| p.text("Plan B."));
            doc.paragraph(|p| p.text("Plan B has no table."));
        }
        Ok(())
    })
}

#[test]
fn kind_a_flow_is_two_paragraphs_then_a_table() {
    let pages = first_example(&contract(Kind::A)).unwrap().pages();
    assert_eq!(pages.len(), 1);
    assert_eq!(texts(&pages[0]),
        ["Contract C-18", "Plan A.", "Item", "Amount", "Pen", "3", "Ink", "5"]);
}

#[test]
fn kind_b_flow_has_no_table() {
    let pages = first_example(&contract(Kind::B)).unwrap().pages();
    assert_eq!(texts(&pages[0]), ["Contract C-18", "Plan B.", "Plan B has no table."]);
    assert!(pages[0].ops().iter().all(|op| !matches!(op, CanvasCommand::StrokePath { .. })));
}

/// §1 full template, minus `bold` and `image_at`.
fn full_template(c: &Contract, company: &str) -> Result<Rendered> {
    document(A4, |doc| {
        doc.font(FONT.to_vec())?;
        doc.margins(Margins::uniform(NonNegative::pt(36.0)?));
        doc.header(|h| {
            h.left(company);
            h.right(format!("第 {} 页 / 共 {} 页", h.page().number, h.page().count));
        });
        doc.paragraph(|p| {
            p.text("Contract no. ");
            p.text(&c.id);
        });
        doc.page_break();
        doc.columns(|row| {
            row.col(|col| col.paragraph(|p| p.text(&c.party_a)));
            row.col(|col| col.paragraph(|p| p.text(&c.party_b)));
        });
        doc.behind(|page| page.text_at("DRAFT", Point::try_pt(180.0, 400.0).unwrap()));
        doc.front(|page| {
            if page.is_last() {
                page.text_at("SEAL", Point::try_pt(430.0, 680.0).unwrap());
            }
        });
        Ok(())
    })
}

#[test]
fn header_numbers_every_page_and_layers_follow_behind_header_body_footer_front() {
    let pages = full_template(&contract(Kind::A), "ACME").unwrap().pages();
    assert_eq!(pages.len(), 2);
    assert_eq!(texts(&pages[0]), ["DRAFT", "ACME", "第 1 页 / 共 2 页", "Contract no. C-18"]);
    assert_eq!(texts(&pages[1]),
        ["DRAFT", "ACME", "第 2 页 / 共 2 页", "Party A", "Party B", "SEAL"]);
}

#[test]
fn header_spans_full_page_width_and_body_respects_margins() {
    let pages = full_template(&contract(Kind::A), "ACME").unwrap().pages();
    let r = runs(&pages[0]);
    assert!(pt_near(r[1].1, 0.0));                    // header left at x = 0 (§2.10)
    assert!(r[2].1 > A4.width().as_pt() / 2.0);       // right slot end-aligned
    assert!(pt_near(r[3].1, 36.0));                   // body at margin.left
    assert!(r[3].2 >= 36.0);                          // body below the band
}

#[test]
fn columns_split_body_width_with_default_gap() {
    let pages = full_template(&contract(Kind::A), "ACME").unwrap().pages();
    let r = runs(&pages[1]);
    let body_w = A4.width().as_pt() - 72.0;
    let col_w = (body_w - 12.0) / 2.0;
    assert!(pt_near(r[3].1, 36.0));
    assert!(pt_near(r[4].1, 36.0 + col_w + 12.0));
    assert!(pt_near(r[3].2, r[4].2));                 // top-aligned
}

#[test]
fn behind_text_at_is_the_top_left_of_its_line_box() {
    let pages = full_template(&contract(Kind::A), "ACME").unwrap().pages();
    let (_, x, y) = &runs(&pages[0])[0];
    assert!(pt_near(*x, 180.0));
    assert!(*y > 400.0);                              // baseline sits below the given point
}

#[test]
fn header_with_zero_margin_is_rejected_before_any_page() {
    let result = document(A4, |doc| {
        doc.font(FONT.to_vec())?;
        doc.header(|h| h.left("H"));
        doc.paragraph(|p| p.text("A"));
        Ok(())
    });
    assert!(matches!(result, Err(Error::DecorationTallerThanMargin { margin, .. }) if margin == 0.0));
}

#[test]
fn lists_number_bullet_and_custom_markers() {
    let pages = document(A4, |doc| {
        doc.font(FONT.to_vec())?;
        doc.ul(|l| {
            l.item(|i| i.paragraph(|p| p.text("dot")));
        });
        doc.ol(Numbering::Decimal, |l| {
            l.item(|i| i.paragraph(|p| p.text("one")));
            l.item(|i| i.paragraph(|p| p.text("two")));
            l.item_marked("(c)", |i| i.paragraph(|p| p.text("custom")));
        });
        Ok(())
    }).unwrap().pages();
    assert_eq!(texts(&pages[0]),
        ["\u{2022}", "dot", "1.", "one", "2.", "two", "(c)", "custom"]);
}

#[test]
fn table_spans_and_repeat_header_switch() {
    let build = |repeat: bool| document(A4, |doc| {
        doc.font(FONT.to_vec())?;
        doc.table(|t| {
            t.repeat_header(repeat);
            t.header(["Item", "Amount"]);
            for n in 0..80 {
                t.row([format!("row {n}"), n.to_string()]);
            }
            t.row([Cell::new("Total").col_span(2)]);
        });
        Ok(())
    });
    let on = build(true).unwrap().pages();
    let off = build(false).unwrap().pages();
    assert!(on.len() >= 2);
    assert_eq!(texts(&on[1])[..2], ["Item", "Amount"]);
    assert_ne!(texts(&off[1])[0], "Item");
    assert_eq!(texts(on.last().unwrap()).last().unwrap(), "Total");
}

#[test]
fn keep_together_spacer_and_page_break() {
    let pages = document(A4, |doc| {
        doc.font(FONT.to_vec())?;
        doc.paragraph(|p| p.text("A"));
        doc.spacer(NonNegative::pt(A4.height().as_pt() - 20.0)?);
        doc.keep_together(|g| {
            g.paragraph(|p| p.text("B1"));
            g.paragraph(|p| p.text("B2"));
        });
        doc.page_break();
        doc.paragraph(|p| p.text("C"));
        Ok(())
    }).unwrap().pages();
    assert_eq!(pages.iter().map(texts).collect::<Vec<_>>(),
        [vec!["A"], vec!["B1", "B2"], vec!["C"]]);
}

#[test]
fn nesting_rules_come_from_layout() {
    let result = document(A4, |doc| {
        doc.font(FONT.to_vec())?;
        doc.columns(|row| {
            row.col(|c| c.page_break());
            row.col(|c| c.paragraph(|p| p.text("x")));
        });
        Ok(())
    });
    assert_eq!(result.err(),
        Some(Error::InvalidCellContent { container: "column", node: "page break" }));
}

#[test]
fn first_builder_error_wins_and_later_calls_are_ignored() {
    let result = document(A4, |doc| {
        doc.font(FONT.to_vec())?;
        doc.table(|t| t.row([Cell::new("x").col_span(0)]));
        doc.columns(|row| row.col(|c| c.paragraph(|p| p.text("only one"))));
        Ok(())
    });
    assert_eq!(result.err(), Some(Error::InvalidCellSpan));
}

#[test]
fn closure_errors_propagate_unchanged() {
    let result = document(A4, |doc| {
        doc.spacer(NonNegative::pt(-1.0)?);
        Ok(())
    });
    assert!(matches!(result, Err(Error::NegativeLength { .. })));
}

#[test]
fn text_without_a_font_fails_before_any_page() {
    let result = document(A4, |doc| {
        doc.paragraph(|p| p.text("A"));
        Ok(())
    });
    assert_eq!(result.err(), Some(Error::UnknownFont { id: 0 }));
}

#[cfg(feature = "backend-krilla")]
#[test]
fn to_pdf_smoke() {
    let pdf = full_template(&contract(Kind::A), "ACME").unwrap().to_pdf().unwrap();
    assert!(pdf.starts_with(b"%PDF"));
}
```

## Shape

### Public types (all in `src/document.rs`, re-exported from `lib.rs`)

```rust
pub fn document<'a>(
    page_size: Size,
    build: impl FnOnce(&mut Document<'a>) -> Result<()>,
) -> Result<Rendered>;

/// Root flow plus document-level settings and page-time hooks.
pub struct Document<'a> {
    fonts: FontStore,
    margins: Margins,                         // default Margins::default() = 0 (§7)
    body: Flow,
    header: Option<Box<dyn Fn(&mut Band) + 'a>>,
    footer: Option<Box<dyn Fn(&mut Band) + 'a>>,
    behind: Option<Box<dyn Fn(&mut Layer) + 'a>>,
    front:  Option<Box<dyn Fn(&mut Layer) + 'a>>,
}
impl<'a> Document<'a> {
    pub fn font(&mut self, bytes: Vec<u8>) -> Result<FontId>;   // first = default face (id 0)
    pub fn margins(&mut self, margins: Margins);
    pub fn header(&mut self, f: impl Fn(&mut Band) + 'a);
    pub fn footer(&mut self, f: impl Fn(&mut Band) + 'a);
    pub fn behind(&mut self, f: impl Fn(&mut Layer) + 'a);
    pub fn front(&mut self, f: impl Fn(&mut Layer) + 'a);
}
impl Deref for Document<'_> { type Target = Flow; }
impl DerefMut for Document<'_> {}

/// Vertical flow content: the document body, a column, a keep-together group, a list item.
pub struct Flow {
    style: Style,                 // default: FontId(0), 12 pt, 1.2, black, spacing 0, Start
    nodes: Vec<FlowNode>,
    error: Option<Error>,         // first builder error; later calls become no-ops
}
impl Flow {
    pub fn paragraph(&mut self, f: impl FnOnce(&mut Paragraph));
    pub fn table(&mut self, f: impl FnOnce(&mut Table));
    pub fn columns(&mut self, f: impl FnOnce(&mut Columns));
    pub fn keep_together(&mut self, f: impl FnOnce(&mut Flow));
    pub fn spacer(&mut self, height: Length<NonNegative>);
    pub fn page_break(&mut self);
    pub fn ul(&mut self, f: impl FnOnce(&mut List));
    pub fn ol(&mut self, numbering: Numbering, f: impl FnOnce(&mut List));
}

pub struct Paragraph { text: String }
impl Paragraph { pub fn text(&mut self, s: impl AsRef<str>); }       // appends

pub struct Columns { cols: Vec<Flow> /* child flows share the parent style */ }
impl Columns { pub fn col(&mut self, f: impl FnOnce(&mut Flow)); }   // equal Ratio(1) tracks, gap 12 pt

pub struct Table { header: Vec<Vec<Cell>>, body: Vec<Vec<Cell>>, repeat_header: bool }
impl Table {
    pub fn header<I>(&mut self, cells: I) where I: IntoIterator, I::Item: Into<Cell>;  // repeatable = multi-row header
    pub fn row<I>(&mut self, cells: I) where I: IntoIterator, I::Item: Into<Cell>;
    pub fn repeat_header(&mut self, on: bool);                                    // default true (§2.6)
}
pub struct Cell { text: String, col_span: u16, row_span: u16 }
impl Cell {
    pub fn new(text: impl Into<String>) -> Self;
    pub fn col_span(self, n: u16) -> Self;   // 0 → deferred InvalidCellSpan
    pub fn row_span(self, n: u16) -> Self;
}
impl From<&str> for Cell; impl From<String> for Cell; impl From<&String> for Cell;

pub enum Numbering { Decimal, UpperAlpha, LowerAlpha, UpperRoman, LowerRoman }  // ↔ layout::list::MarkerKind
pub struct List { items: Vec<(Option<String>, Flow)> }
impl List {
    pub fn item(&mut self, f: impl FnOnce(&mut Flow));
    pub fn item_marked(&mut self, marker: impl Into<String>, f: impl FnOnce(&mut Flow));
}

/// Page facts available at draw time. Single implicit section: totals only.
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Page { pub number: u32, pub count: u32, pub size: Size, pub margins: Margins }
impl Page { pub fn is_last(&self) -> bool; }

pub struct Band { page: Page, left: Option<String>, right: Option<String> }
impl Band {
    pub fn page(&self) -> Page;
    pub fn left(&mut self, s: impl AsRef<str>);
    pub fn right(&mut self, s: impl AsRef<str>);
}

pub struct Layer { page: Page, texts: Vec<(String, Point)> }
impl Layer {
    pub fn page(&self) -> Page;
    pub fn is_last(&self) -> bool;
    pub fn text_at(&mut self, s: impl AsRef<str>, at: Point);   // `at` = top-left of the line box
}

pub struct Rendered { recording: RecordingCanvas, fonts: FontStore }
impl Rendered {
    pub fn pages(&self) -> Vec<RecordedPage>;
    #[cfg(feature = "backend-krilla")]
    pub fn to_pdf(self) -> Result<Vec<u8>>;
}
```

13 public names. `Flow`, `Paragraph`, `Columns`, `Table`, `List`, `Band`, `Layer` have no public constructors; callers only ever get `&mut` to them inside closures.

### Data flow

```text
document(size, build)
  ├─ Document::new: FontStore::new(), Flow::root(default_style())
  ├─ build(&mut doc)?                      ← caller's `?` errors return unchanged
  ├─ if let Some(e) = doc.body.error { return Err(e) }
  ├─ root = FlowNode::Stack(doc.body.nodes)
  ├─ decorations = Decorations { header/footer: wrap_band(f, style), behind/front: wrap_layer(f, style) }
  ├─ paginate_root(root, &mut LayoutCtx{fonts}, PaginateConfig{size, margins}, &decorations, &mut RecordingCanvas)
  └─ Ok(Rendered { recording, fonts })

Rendered::to_pdf   (backend-krilla)
  └─ replay recording.commands() into KrillaCanvas::new_with_fonts(Arc::new(fonts)) → finish()
```

**One lower path, at closure exit.** Each builder method lowers to a finished `FlowNode` when its closure returns and pushes it via `Flow::push(Result<FlowNode>)`:

| Builder | Lowers to | Validation it inherits (unchanged knife-7 code) |
| --- | --- | --- |
| `paragraph` | `FlowNode::Text(TextNode::new(text, style))` | lazy wrap at the current `avail.width` (§2.1) |
| `columns` | `ColumnsNode::new(default_column_gap(), cols.map(\|f\| (Ratio(1), f.nodes)))` | `check_content(Column)`, ≥2 columns |
| `table` | `TableNode::new(TableSpec { widths: n × Ratio(1), header, body, repeat_header, ..TableSpec::new })` where n = Σ col_span of the first row; each cell is `Cell::new(vec![Text]).with_span(..)` | `CellBody` rules, `InvalidCellSpan`, column-count |
| `keep_together` | `FlowNode::KeepTogether(Open(child.nodes))` | `UnsplittableTallerThanFrame` at split |
| `spacer` | `FlowNode::Spacer(SpacerNode { height })` | none needed: the type is already non-negative |
| `page_break` | `FlowNode::PageBreak` | three-state cursor in paginate; rejected in containers by `check_content` |
| `ul` / `ol` | `ListNode::new(ListSpec { kind, marker_width: 18 pt, nest_indent: None, marker_style: style, items })` | `ItemBody` rules, marker overflow |

A child `Flow`'s `error` merges into the parent when the child closure returns (first error wins). The nesting rule matrix stays in `layout::lower::check_content`. The DSL adds no rules of its own, which is why `row.col(|c| c.page_break())` compiles and then fails with the layout error (per single-source-of-truth).

**Page-time hooks are wrapped closures stored in `Decorations`.** `wrap_band(f, style)` builds a `BandContent<'a>`: it creates a `Band` from `PageInfo` per page, runs `f`, and lowers the slots:

- no slot set → `vec![]` (measure_band already returns `None`)
- left only → `[Text(left, Start)]`; right only → `[Text(right, End)]`
- both → `[Columns(gap 0, [(Ratio1, [Text Start]), (Ratio1, [Text End])])]`

`wrap_layer(f, style)` builds a `LayerContent<'a>` that returns the collected `(String, Point)` texts. Fonts are only touched later inside `measure_bands`, so a `MissingGlyph` in a header or watermark still fails before the first `start_page` (this keeps the existing "bands measured before paint" invariant).

### Band slots take strings, not `string | closure`

To make §1's `h.right(|page| …)` and `h.left(&name)` work on the same method, the method would have to be generic over a `SlotText` trait implemented for `&str`, `String`, and `F: FnOnce(&Page) -> String`. Coherence allows this (`FnOnce` is `#[fundamental]`), but Rust does not infer closure parameter types through a blanket impl. `|page| page.number` would then fail with E0282 or "not general enough" unless the caller annotates `|page: &Page|`. Because the band closure already runs once per page, `h.page()` exposes the same facts and needs no trait (per lean-surface). Two-phase borrows make `h.right(format!(.., h.page().number))` compile.

### Knife-7 internals touched (minimal diff, no split logic)

| File | Change |
| --- | --- |
| `layout/decorate.rs` | `BandContent<'a> = Box<dyn Fn(&PageInfo) -> Result<Vec<FlowNode>> + 'a>`; `Decorations<'a>` gains `behind`, `front: Option<LayerContent<'a>>` where `LayerContent<'a> = Box<dyn Fn(&PageInfo) -> Vec<LayerText> + 'a>` and `LayerText { text, at: Point, style: Style }`. `PageBands` gains `behind`, `front: Vec<PlacedFragment>`. Each layer text is measured with `paragraph::single_line_run(fonts, text, style, f32::INFINITY)` and placed at `at` (no wrapping, no bounds check, per §2.12). Existing test literals get `..Decorations::none()`. |
| `layout/paginate.rs` | `paginate_root(.., decorations: &Decorations<'_>, ..)`; `paint_pages` order: behind → header → body → footer → front. |
| `layout/lower.rs` | `FlowNode::Columns(_) => matches!(container, Container::Column \| Container::Band)`, so left/right slots can share a line. Tables in bands are still rejected (existing `band_rejects_tables` test unchanged). |
| `lib.rs` | `pub use document::{document, Band, Cell, Columns, Document, Flow, Layer, List, Numbering, Page, Paragraph, Rendered, Table};` No krilla types. |

`PageInfo → Page` is a `From` impl in `document.rs`; `is_last = number == count`.

### Module map

```text
src/document.rs   all public DSL types, wrap_band / wrap_layer, document(), Rendered, replay (cfg backend)
                  → layout::{flow, paragraph, columns, table, list, decorate, paginate}, fonts, canvas
src/layout/*      unchanged except the three rows above
src/backend/*     unchanged; Rendered::to_pdf calls KrillaCanvas::new_with_fonts + finish
tests/document.rs the usage tests above (public API only)
```

The dependency direction stays `document → layout → (fonts, text, canvas)`. `document.rs` names `KrillaCanvas` only inside the `#[cfg(feature = "backend-krilla")]` `to_pdf` body, which is the same position `lib.rs` already gives `backend`. If `document.rs` grows past about 400 lines, split it into `document/{mod, flow, table, band}.rs` in the same PR. Do not split ahead of that.

### Interface depth

The public surface hides `FlowNode`, `Style`, `TextNode`, `ColumnWidth`, `TableSpec`, `ListSpec`, `KeepTogetherNode`, `PageInfo`, `Decorations`, `LayoutCtx`, the page cursor, band measurement, and krilla. Callers see closures, strings, `Length`/`Margins`/`Point`/`Size` from `geom`, and `Rendered`. What is exposed and not yet configurable (column widths, table borders and padding, styles) is left out deliberately and listed under open questions. It is not hidden behind defaults the caller can't see: the defaults are documented on each builder.

## Synthesis decision

*(arena fills this in)*

## Tradeoffs accepted

- We accept `Deref<Target = Flow>` on `Document` (an "inheritance-looking" pattern) in exchange for one copy of the eight flow methods. Rustdoc lists them under "Methods from Deref".
- We accept deferred errors (builders return `()`, the first error surfaces from `document`) in exchange for §1's `doc.paragraph(|p| p.text(..));` shape with no `?` after every call. The error still comes out before any page is painted.
- We accept `UnknownFont { id: 0 }` as the "you forgot `doc.font`" error in exchange for adding no new `Error` variant and no new check. It still fails before painting.
- We accept band slots at x = 0 with no margin inset (literal §2.10) even though string-only slots give the caller no way to indent. See open questions.
- We accept `Cell::col_span(u16)` with 0 rejected as a deferred `InvalidCellSpan` in exchange for not making callers write `NonZeroU16::new(2).unwrap()`. This departs slightly from §2.0's "type first" rule. The error variant is the one the span checker already returns.
- We accept laying out once into a `RecordingCanvas` and replaying it into krilla in exchange for a single layout pass shared by tests and PDF output. It costs one extra in-memory command log.
- We accept a hard-coded list `marker_width = 18 pt` and paragraph spacing 0 until style settings are pinned.
- We accept a lifetime parameter on `Document<'a>` and `Decorations<'a>` in exchange for §1's borrowed captures (`&company.name`) in header closures.

## Alternatives considered

- **Build an internal DSL AST and lower everything in `document()`** (`enum Block { Paragraph(String), Table(..), .. }`, then a `lower(Block) -> FlowNode` pass). This creates a second tree that mirrors `FlowNode` one-to-one, plus a lower pass with nothing to decide. Errors would still surface at the same point. It lost on laziness: same interface, more hidden code, and two places to keep in sync.
- **Builder methods return `Result<&mut Self>`** (`doc.paragraph(..)?;`). This gives immediate errors at the call site, but every §1 line gains a `?`, and nested closures would have to return `Result` too. The caller pays complexity for errors they can't act on mid-build anyway.
- **`document()` returns `Vec<u8>` directly, plus a crate-internal `document_recorded` for tests.** This matches §1 literally, but integration tests in `tests/` can't reach a `pub(crate)` function, and a no-default-features build would have no `document` at all. `Rendered` costs one public type and keeps both paths on one function.
- **`SlotText` overload for `h.right(|page| ..)`.** Rejected for the closure inference failure described above.

## Open questions and risks

1. **Band horizontal inset.** §2.10 says bands span the full width and the caller indents using `margin.left`. With string-only slots the caller *cannot* indent. Should `left`/`right` inset by the page margins (which amends §2.10), or should `Band` also expose `h.paragraph(|p| ..)` with an indent? Until that's decided, a header set with `doc.margins(uniform 72)` touches the paper edge.
2. **`p.bold`.** Does rich text (multiple runs per paragraph with a face per run) belong in knife 8, or should the §1 sample be amended? This design omits it.
3. **Style settings.** No public font size, alignment, line height, or paragraph spacing yet. Which names should be pinned (a `doc.default_style(..)`, or per-paragraph `p.size(..)` / `p.align(..)`)? Tests are fine without them; real contracts are not.
4. **Table widths, borders, padding.** §2.6 confirms they exist but doesn't name them. Is `t.widths([..])` / `t.border(..)` / `t.padding(..)` acceptable, or should knife 8 ship with borderless equal-ratio tables only?
5. **`PageSize` enum and orientation.** Should knife 8 take `geom::Size` (as here), or pin `PageSize::{A3, A4, A5, Letter, Custom}` + `Orientation` now?
6. **Section page numbers.** `Page` carries totals only (`#[non_exhaustive]`, so section fields can be added later). Is that acceptable for the single implicit section, or should `section_number`/`section_count` ship now, equal to the totals?
7. **Risk, unverified:** `single_line_run(.., f32::INFINITY)` assumes `Start` alignment ignores `max_width` (`align_x` returns 0). Verify before relying on it, or pass `page_w - at.x` instead.
8. **Risk, unverified:** the behaviour of an empty paragraph (`TextNode::new("")`) is whatever knife 7 does. Pin it with a test while implementing.
9. **`doc.font` takes only `Vec<u8>`**, collection index 0. Is `font_at(bytes, index)` needed for `.ttc` CJK collections?

## Next implementation step

Land the three layout tweaks (lifetime on `Decorations`, layer slots and paint order, `Band` allowing `Columns`) together with the existing tests still green. Then write `tests/document.rs` exactly as above and fill `document.rs` until it passes.
