# Public Flow DSL seam for a block horizontal rule

Baseline: `HEAD` `7cb1654aca432fffe6d51ca519c17a34b2c55252` (`feat(document): draw line, rect, and ellipse on behind/front layers`). Repo `/mnt/data/demo/quire`.

Scope: how `src/document.rs` exposes block nodes, how cells and list items take nested content, and why `Layer` (behind/front) is a different path. No new API. Proposal text is cited only as already-written constraints (`docs/提案书.md` §2.3, §2.6, §2.8, §2.12, §2.13).

There is no `hline`, `hr`, `rule`, or divider type in `src/` at this commit. `Paragraph` has no blank/underline method. The only public stroke primitives are `Layer::line` and `Shapes::line`.

---

## Components Found

### Who owns the body

| Type | What it is | Lines |
| --- | --- | --- |
| `document()` | Runs the caller closure once, then `Document::paint`. Builder errors return from here. | `src/document.rs:41-55`, comment `1-5`, `33-37` |
| `Document` | Settings, fonts, images, sections. Body methods are **not** declared here; they come from `DerefMut` to `Flow`. | `186-196`, deref `475-493` |
| `Section` | Paper, margins, one `Flow`, plus header/footer/behind/front hooks. Also `DerefMut` to that `Flow`. | `71-79`, deref `171-183` |
| `Flow` | The only public vertical-content builder. Private fields: `style`, `doc`, `nodes: Vec<FlowNode>`, `error`, `reject_pushes`. No public constructor. | `506-513`, `515-664` |
| `FlowNode` | Crate-private enum. Variants: `Paragraph`, `Text`, `Spacer`, `PageBreak`, `KeepTogether`, `Stack`, `Columns`, `Table`, `List`, `Image`. No rule variant. | `src/layout/flow.rs:139-150` |

`Document` and `Section` inherent methods (`behind`, `front`, `header`, `footer`, `margins`, `section`, `font`, `load_image`) do not go through `Flow`. Method resolution prefers the inherent method, so `doc.behind` is `Document::behind` (`270-285`), while `doc.paragraph` / `doc.spacer` / `doc.image` are `Flow` via deref.

After the first `Document::section`, the document's deref target becomes a `Flow::rejecting` (`312-317`, `526-530`). Later `push`es store `Error::ContentOutsideSection` and drop the node (`541-547`). A new `Flow` method that uses `push` inherits that.

### Public `Flow` methods (the block API)

All of these append one `FlowNode` via `push` (`541-553`). First stored error wins; later `push`es return immediately.

| Method | Shape | What it pushes | Lines |
| --- | --- | --- | --- |
| `image(&Image, FnOnce(&mut ImageBox))` | required handle + closure builder | `ImageBox::lower` → `FlowNode::Image` | `575-579`, lower `465-472` |
| `paragraph(FnOnce(&mut Paragraph))` | closure only | `FlowNode::Text(TextNode::new(string, style.clone()))` — **not** `FlowNode::Paragraph` | `582-587`, `568-570` |
| `columns(FnOnce(&mut Columns))` | closure; each `col` is a nested `Flow` | `FlowNode::Columns` | `590-598`, `Columns::col` `687-690` |
| `table(FnOnce(&mut Table))` | closure; cells are values, not `Flow` | `FlowNode::Table` | `603-611`, `Table::lower` `738-769` |
| `keep_together(FnOnce(&mut Flow))` | nested `Flow` | `FlowNode::KeepTogether(Open(nodes))` | `615-620` |
| `spacer(Length<NonNegative>)` | **one immediate argument, no closure** | `FlowNode::Spacer(SpacerNode { height })` | `623-625` |
| `page_break()` | no arguments | `FlowNode::PageBreak` | `628-630` |
| `ul` / `ol` | closure over `List`; each item is a nested `Flow` | `FlowNode::List` | `633-663`, `865-891` |

Private helpers that every nested container shares:

- `child()` clones `style` and `doc` into a fresh `Flow` (`537-539`).
- `nested(FnOnce(&mut Flow))` runs that child and returns `Result<Vec<FlowNode>>` (`555-559`). Child `error` becomes `Err`; it does not push onto the parent (`561-566`).
- `text(String)` builds one `FlowNode::Text` with the **current** `Flow` style (`568-570`). Used by `paragraph` and by string cells.

Root lowering wraps the body's `Vec<FlowNode>` in `FlowNode::Stack` **without** `check_content` (`Section::lower` `160`). Nested containers do call `check_content` (see Boundaries).

### Builder pattern actually used

Two styles exist. Neither is a fluent `self -> Self` chain on the flow block (that pattern is only `Cell::col_span` / `row_span`, because a cell is a value inside a row iterator).

**Closure + `Default` + `&mut self` setters returning `()`.** The closure is `FnOnce`, not `FnOnce(...) -> Result`. Length errors happen in the **outer** `document` closure via `?` on `Positive::pt` / `NonNegative::pt`, before the setter runs.

`ImageBox` (`428-472`) is the multi-field block builder:

- Fields: `width: Option<Length<Positive>>`, `height: Option<Length<Positive>>`, `align: TextAlign` (`428-432`).
- `Default`: both sides `None`, `align: TextAlign::Start` (`434-441`).
- `width` / `height` store `Some` (`446-453`).
- `center()` / `end()` set `TextAlign::Center` / `End` (`456-463`). There is no `start()` and no `align(TextAlign)` argument. `TextAlign` is not a public parameter.
- `lower` is private. It checks `Image.doc == Flow.doc`, then `ImageNode::new(...).map(FlowNode::Image)` (`465-472`). Size is resolved inside `ImageNode::new` (`src/layout/image.rs:19-26`), not left as raw options.
- Call sites pass `|_| {}` when every field stays default (`tests/document.rs:541`, `583`). Configured form: `doc.image(&logo, |image| { image.width(width); image.center(); })` (`480-483`).
- The same `ImageBox` closure is reused by `Cell::image` (`799-807`). One builder, two entry points (`Flow` method vs associated constructor).

`Paragraph` (`668-677`) is the closure-only block with a single infallible setter:

- `#[derive(Default)]`, one private `text: String`.
- `text(&mut self, impl AsRef<str>)` appends. Several calls become **one** string, one `Text` node (`document.rs` unit test `1216-1225`).
- No align, color, spacing, or nested-node methods. It is not a container.

**Immediate arguments, no builder.**

- `spacer(height: Length<NonNegative>)` (`623-625`). One branded length, pushed as `Ok`. Test: `doc.spacer(NonNegative::pt(...)?)` (`tests/document.rs:330`). Negative length fails the outer closure as `Error::NegativeLength` **before** paint (`416-421`), because `NonNegative::pt` returns `Result` (`src/geom.rs:107-112`).
- `page_break()` (`628-630`).
- `Layer::line(from: Point, to: Point, pen: Pen)` (`1066-1068`). Three required args. Not a flow node. See Layer contrast.

`Table::repeat_header(&mut self, bool)` (`734-736`) is a single setter on the table builder, same `&mut self -> ()` shape as `ImageBox::width`.

### `TextAlign` (crate-private; image already uses it for “narrower than frame”)

`src/layout/style.rs:8-14`: `pub(crate) enum TextAlign { Start, Center, End }`. Not re-exported from `src/lib.rs`. `layout` is `pub mod` (`lib.rs:33`) but its items are `pub(crate)`, so downstream crates cannot name `TextAlign`.

`default_style()` sets `align: TextAlign::Start` and `color: Color::rgb(0, 0, 0)` (`document.rs:495-503`). Nothing in `document.rs` assigns `flow.style` after construction (only `.clone()`). There is no public style/color/align setter. “Current text color” at the DSL is that black.

Image placement at split (`src/layout/image.rs:44-48`):

- `Start` → `x = 0`
- `Center` → `(avail.width - width) / 2`
- `End` → `avail.width - width`

`ImageBox`’s **default is Start**. Proposal §2.3 says a shorter rule’s default alignment is **Center** (`docs/提案书.md:277`). The enum and the x-formula are the reuse site; the image default is not the §2.3 default.

`Band::lower` also sets `TextAlign::Start` / `End` on cloned `Style` (`document.rs:964-975`). Bands do not expose align to the caller; `left` / `right` pick it.

### Geom types a rule’s fields already have

`src/lib.rs:12-15` re-exports `Color`, `Length`, `NonNegative`, `Positive`, `Stroke`, `Pen`, `Ink`, `Paint`, `Path`, `Point`, `Rect`. It does **not** re-export `TextAlign`.

| Type | Constraint | Construction | Lines |
| --- | --- | --- | --- |
| `Length<C>` | branded pt; `Finite` / `NonNegative` / `Positive` | factories on the brand, not on `Length` | `src/geom.rs:14-24`, `65-75` |
| `NonNegative` | finite, `>= 0`. `ZERO` exists | `pt` / `mm`; `< 0` → `Error::NegativeLength` | `102-126` |
| `Positive` | finite, `> 0`. No `ZERO` | `pt` / `mm`; `<= 0` → `Error::NonPositiveLength` | `129-150` |
| `Color` | `u8` RGBA, total constructors | `rgb` (alpha 255), `rgba` | `473-498` |
| `Paint` | solid fill wrapper | `Paint::solid(color)` | `501-517` |
| `Stroke` | **width only**, `Length<Positive>` | `Stroke::new(width)` | `520-536` |
| `Pen` | `Color` + `Stroke` | `Pen::new(color, width: Length<Positive>)` | `539-564` |
| `Ink` | `Fill(Color)` / `Stroke(Pen)` / `FillStroke(Color, Pen)` | used by `Layer::rect` / `ellipse`, not by `Layer::line` (line always `Ink::Stroke(pen)`) | `567-578`, `document.rs:1066-1067` |

`Stroke` has no dash, no double-line, no cap. That matches §2.3 “默认实线” and “虚线 / 双线 / 竖线第一版不做” (`docs/提案书.md:278-279`) as an absence, not as an implemented flag.

Table grid lines are a different width type: `layout::table::Border.width` is `Length<NonNegative>`, and the comment says width zero draws nothing (`src/layout/table.rs:16-21`). `Pen` / `Stroke` cannot represent a zero-width stroke (`geom.rs:520`, `539`).

Widening: `Length<Positive>` converts to `NonNegative` and `Finite` (`geom.rs:153-168`). The reverse is not a `From`.

### `Layer` / `Shapes` (must not host the flow rule)

Stored as hooks, **not** run during the build closure. They run once per physical page inside pagination (`document.rs:3-5`, `1158-1172`). Output is `Vec<LayerItem>`, not `Vec<FlowNode>`.

`Layer` methods (`1024-1114`):

| Method | Args | Stored as |
| --- | --- | --- |
| `page` / `is_last` | — | read `Page` (filled at paint time) |
| `text_at(s, Point)` | page point, no wrap | `LayerItem::Text` |
| `image_at(&Image, Rect)` | page rect, stretch, not margin-inset | `LayerItem::Image` |
| `line(Point, Point, Pen)` | any segment; `from == to` → `Error::InvalidGeometry` | `LayerItem::Shape { path, ink: Ink::Stroke(pen) }` |
| `rect(Rect, Ink)` / `ellipse(Rect, Ink)` | page geometry | `LayerItem::Shape` |
| `rotated(degrees, about, FnOnce(&mut Shapes))` | bakes rotation into the path | same `Shape` via `Shapes` |

`Shapes` (`1117-1144`) is path-only: `line`, `rect`, `ellipse`. No text, no image, no flow nodes.

`LayerItem` (`src/layout/decorate.rs:57-71`): `Text`, `Image { image, rect }`, `Shape { path, ink }`. No flow node, no frame width, no split.

`Document::behind` / `front` (`270-285`) and `Section::behind` / `front` (`130-138`) only replace the hook. A later call replaces an earlier one. They are illegal on the document after `section()` (`fail_outside`, `325-328`).

Integration proof that this path is page-absolute, not body flow: `tests/document.rs:816-825` draws `layer.line(Point(0,0), Point(40,40), Pen::new(Color::rgba(200,0,0,64), Positive::pt(0.8)))` inside `doc.behind`, with body margins of 20 pt. The segment is not “current frame width”.

§2.12 (`docs/提案书.md:527-541`): behind/front are page-coordinate escapes, do not enter the body flow, do not consume body height, and do not run flow line-breaking. Path primitives cover watermarks and binding lines. §2.3 (`270-272`) says the divider is a block flow node, not the `behind` escape.

### Module wiring

`src/lib.rs`:

- `pub mod document` (`18`) and re-export (`21-24`): `document`, `Band`, `Cell`, `Columns`, `Document`, `Flow`, `Image`, `ImageBox`, `Layer`, `List`, `Numbering`, `Page`, `Paragraph`, `Rendered`, `Section`, `Shapes`, `Table`.
- `FlowNode`, `SpacerNode`, `TextAlign`, `Style` are not in that list.
- `pub mod layout` (`33`) with no `pub use`. `src/layout/mod.rs:1-15` only declares submodules (`columns`, `decorate`, `dump`, `flow`, `image`, `list`, `lower`, `paginate`, `paragraph`, `style`, `table`). No `hline` module. No re-exports.

`document.rs` is the only file that turns public builders into `FlowNode`s. Imports at `22-28`: `FlowNode`, `KeepTogetherNode`, `SpacerNode`, `ImageNode`, `ListItem` / `ListNode`, `TextNode`, `Style` / `TextAlign`, `table::{Cell, TableNode, TableSpec}`.

A new `FlowNode` variant does not compile until every exhaustive match is updated. The DSL-adjacent ones:

- `FlowNode::split` (`src/layout/flow.rs:152-167`)
- `check_content` / `node_name` (`src/layout/lower.rs:30-79`)

`push` itself is not exhaustive; it takes an already-built `Result<FlowNode>`.

---

## Flow

### 1. Body block (spacer / image / paragraph)

```text
document(|doc| { ... })
  Document deref → Flow
    paragraph(|p| p.text(..))  → push TextNode          (string + style; wrap later)
    image(&img, |b| b.width)   → push ImageNode         (size resolved in ImageNode::new)
    spacer(Length<NonNegative>)→ push SpacerNode        (length stored; split later)
  Section::lower → FlowNode::Stack(nodes)               (no check_content)
  paint → paginate
```

`SpacerNode::split` (`src/layout/flow.rs:170-199`) places empty `commands` (no stroke). It **can** split: leftover height becomes another `Spacer`. That is the opposite of §2.3 “整条不可拆” (`docs/提案书.md:282`). Spacer is the precedent for “one `NonNegative` length, immediate arg, legal in every container”, not for painting or pagination.

`ImageNode::split` (`src/layout/image.rs:29-48`) is the precedent for an atomic block: too wide → `UnbreakableWiderThanFrame`; taller than `full_height` → `UnsplittableTallerThanFrame`; taller than remaining → `MoveToNextPage`; else `Fit` with x from `TextAlign`. It does not take `LayoutCtx`.

`paragraph` does not resolve width at build time. `TextNode` wraps against the frame it is first split in (`src/layout/paragraph.rs:21-22`, `28-30`). A rule whose default width is “current frame `W`” (`docs/提案书.md:276`) matches that timing, not `ImageNode::new`’s build-time pixel size. An optional shorter length stored as `Option<Length<Positive>>` matches `ImageBox.width`, applied later against `avail.width` the way image align is.

### 2. Nested `Flow` (list items, columns, keep-together)

These closures receive `&mut Flow`, so **every** `Flow` method is callable inside them. No extra method on `List` or `Columns` is required for a new block.

| Site | How the child `Flow` is created | When the node list is checked | Lines |
| --- | --- | --- | --- |
| `List::item` | `parent.nested(f)` then `ListItem::new` | `ItemBody::new` → `check_content(ListItem)` | `document.rs:867-869`, `layout/list.rs:106-110`, `lower.rs:100-103` |
| `List::item_marked` | same, then `ListItem::custom` (empty marker → `InvalidGeometry`) | same `ItemBody` | `document.rs:873-879`, `list.rs:113-121` |
| `Columns::col` | `parent.nested(f)` | `ColumnsNode::new` → `check_content(Column)` per column, then each column becomes `FlowNode::Stack` | `document.rs:687-689`, `columns.rs:103-118` |
| `Flow::keep_together` | `self.nested(f)` | **not** checked at the document root. Checked only if the `KeepTogether` node itself sits in a container (see matrix) | `document.rs:615-619`, `lower.rs:38-42` |

`Columns` and `List` hold a `parent: Flow` only to call `nested` / read `style` (`document.rs:680-683`, `860-863`). They do not push onto the outer flow until `lower` returns one node.

`List::lower` hardcodes `marker_width: Positive::pt(18.0)` and `marker_style: parent.style` (`882-890`). Item content does not change the marker style.

Proposal §2.8 item content (`docs/提案书.md:453`): 段落（含填空）、**线**、spacer、嵌套列表；不要 table/columns. The DSL already lets an item call `paragraph`, `spacer`, nested `ul`/`ol`. It cannot call a line method, because none exists. It **can** call `image`, `table`, `columns`, `page_break`, `keep_together`; those fail later in `check_content`, not at the method call.

Proof: `tests/document.rs:593-606` — `doc.ul(|list| list.item(|item| item.image(&logo, |_| {})))` returns

```text
Error::InvalidCellContent { container: "list item", node: "image" }
```

Same error type for a page break in a column (`378-393`): `container: "column", node: "page break"`. The message format is `"{node} is not allowed in a {container}"` (`src/error.rs:88-93`, `165-166`). Container names: `"column"`, `"table cell"`, `"list item"`, `"header/footer band"` (`lower.rs:18-26`). `node_name` strings (`67-78`): `"paragraph"` (both `Text` and `Paragraph`), `"spacer"`, `"page break"`, `"keep together"`, `"stack"`, `"columns"`, `"table"`, `"list"`, `"image"`.

### 3. Table cells do **not** take a nested `Flow`

Public `Cell` (`document.rs:776-824`):

```text
CellContent::Text(String) | CellContent::Image(Image, ImageBox)
```

| Constructor | Content | Span default |
| --- | --- | --- |
| `Cell::new(impl Into<String>)` | `Text` | 1×1 |
| `From<&str>` / `From<String>` / `From<&String>` | `new` | 1×1 |
| `Cell::image(&Image, FnOnce(&mut ImageBox))` | `Image` + configured `ImageBox` | 1×1 |
| `col_span(self, NonZeroU16) -> Self` | unchanged | columns |
| `row_span(self, NonZeroU16) -> Self` | unchanged | rows |

`Table::header` / `row` take `IntoIterator<Item: Into<Cell>>` (`715-731`). Examples: `t.header(["Item", "Amount"])`, `t.row([format!(...), n.to_string()])`, `t.row([Cell::new("Total").col_span(...)])`, `t.row([Cell::new("A"), Cell::image(&seal, |image| image.width(side))])` (`tests/document.rs:304-309`, `505-510`).

`Table::lower` (`738-769`) always builds a **one-element** `Vec<FlowNode>`:

- `Text(text)` → `vec![flow.text(text)]` (parent `Flow`’s style)
- `Image(image, image_box)` → `vec![image_box.lower(&image, flow.doc)?]` (doc-token check happens here, not in `Cell::image`)

Then `table::Cell::new(content)` (`src/layout/table.rs:58-63`) → `CellBody::new` → `check_content(TableCell)`.

There is no `Cell` method that takes `FnOnce(&mut Flow)`, no `Cell::spacer`, no `Cell::paragraph` builder, no way to put two blocks in one cell from the public DSL. Internal `table::Cell::new` already accepts `Vec<FlowNode>`, so the layout side can hold a sequence; the DSL never builds one.

Proposal §2.6 (`docs/提案书.md:377-381`): cells may contain 段落、分隔线、spacer, and image/list as those PRs land; cells forbid `columns`, pageBreak, keep_together, nested table. §2.13 (`553`): spacer is allowed in a cell. The **matrix** already allows `Spacer`, `Text`/`Paragraph`, `Image`, and `List` in `TableCell`. The **public `Cell`** can only emit `Text` and `Image`. Spacer and a divider are legal internally and unreachable from `row([...])` until `CellContent` grows another arm (the `Cell::image` constructor is the existing pattern for a non-text cell).

### 4. `check_content` matrix (`src/layout/lower.rs:30-65`)

`true` means the node is legal in that container. Stack always recurses with the **same** container. A new `FlowNode` arm is a compile error until it is added here and in `node_name`.

| Node | Column | TableCell | ListItem | Band |
| --- | --- | --- | --- | --- |
| `Text`, `Paragraph`, `Spacer` | yes | yes | yes | yes |
| `Stack` | recurse | recurse | recurse | recurse |
| `KeepTogether` | yes, and if `Open`, recurse kids | no | no | no |
| `Columns` | yes (kids already checked when the `ColumnsNode` was built) | no | no | yes, and recurse `column_stacks()` |
| `Table` | yes | no | no | no |
| `List` | yes | yes | yes | no |
| `Image` | yes | yes | **no** | no |
| `PageBreak` | no | no | no | no |

Callers:

- Column: `columns.rs:115` inside `ColumnsNode::new` (needs ≥ 2 columns, else `Error::ColumnWidths { reason: "column-count" }`, `107-110`).
- Table cell: `lower.rs:86-88` via `CellBody::new`.
- List item: `lower.rs:101-103` via `ItemBody::new`.
- Band: `decorate.rs:257` inside `measure_band`, on the `Vec<FlowNode>` that `Band::lower` returned.

Partial tests of the reject side (not the full matrix):

- Cell rejects columns, page break, keep-together, table, and a stack whose child is a page break. The stack itself is allowed; the reported `node` is the child’s name `"page break"` (`table.rs:626-656`).
- List item rejects table, page break, image (`list.rs:496-522`). Columns and keep-together are rejected by the matrix but not named in that test.

The arm that already admits a simple block in **column, cell, and list item** is the `Spacer` arm (`lower.rs:33`: `=> true`, which also admits it in a band). `Image` is the arm that is intentionally **not** legal in a list item.

Root body and a root-level `keep_together`’s children skip this matrix. `page_break` is legal on the document `Flow` and illegal in column/cell/list/band.

### 5. Bands are flow nodes, but not a `Flow` builder

`Band` (`document.rs:941-1000`): `left` / `right` strings only. `lower` builds `FlowNode::Text` and, when both slots or margins are present, a `Columns` of those texts. It never calls `Flow` methods. `check_content(Band)` still runs at measure time (`decorate.rs:248-257`). A variant grouped with `Spacer` (`=> true`) would be **accepted** if a band `Vec` contained it, but `Band::lower` cannot emit it.

Header/footer hooks are page-time, like layers (`band_content` `1003-1012`), and they do consume body-band height. They are still not the §2.3 body node.

### 6. Where a horizontal stroke is drawn today (not a flow block)

- `FragmentCommand::Stroke { path, paint, stroke }` exists (`flow.rs:86-90`). `draw_fragment` calls `canvas.stroke_path` (`362-365`).
- Table borders produce those commands internally (not a caller-inserted flow node).
- `Layer::line` produces a `Path` + `Ink::Stroke(Pen)` on the layer, in page coordinates (`document.rs:1066-1068`, `1146-1151`). `from == to` fails. The path is not clipped to the body frame.

No `Paragraph` or `TextNode` code builds a `Stroke` (`src/layout/paragraph.rs` has no `Stroke` use). Inline blank (§2.2, `docs/提案书.md:250-268`) is not implemented. A rule method on `Paragraph` would be a new inline channel; the block precedents are `Flow::spacer` / `Flow::image` / `Flow::paragraph`.

---

## Files Read

- `src/document.rs` — `document`, `Section`, `Document`, `Image`/`ImageBox`, `Flow`, `Paragraph`, `Columns`, `Table`, `Cell`, `List`, `Band`, `Layer`, `Shapes`, unit test `paragraph_enters_the_flow_unwrapped` (`1216-1225`)
- `src/lib.rs` — re-exports
- `src/layout/mod.rs` — submodule list
- `src/layout/flow.rs` — `FlowNode`, `SpacerNode::split`, `FragmentCommand::Stroke`, `draw_fragment` (split of other nodes not traced)
- `src/layout/lower.rs` — `Container`, `check_content`, `node_name`, `CellBody`, `ItemBody`
- `src/layout/image.rs` — `ImageNode::new` / `split` align (`1-48`)
- `src/layout/style.rs` — `TextAlign`, `Style`
- `src/layout/table.rs` — `Border`, `table::Cell::new`, cell-reject test (`16-21`, `50-73`, `626-656`)
- `src/layout/list.rs` — `ListItem::new` / `custom`, item-reject test (`100-123`, `496-522`)
- `src/layout/columns.rs` — `ColumnsNode::new` content check (`102-124`)
- `src/layout/decorate.rs` — `LayerItem`, `measure_band` (`50-81`, `248-257`)
- `src/layout/paragraph.rs` — `TextNode` is unwrapped text (`21-30`); no stroke
- `src/geom.rs` — `Length`, `NonNegative`, `Positive`, `Color`, `Paint`, `Stroke`, `Pen`, `Ink` (`14-169`, `473-578`)
- `src/error.rs` — `InvalidCellContent` (`88-93`, `165-166`)
- `tests/document.rs` — spacer, image closure, cell image, list-item image reject, column page-break reject, behind `line` (`326-345`, `378-421`, `473-532`, `593-606`, `808-836`)
- `docs/提案书.md` — §2.3 `270-285`, §2.6 cells `377-384`, §2.8 items `421-454`, §2.12 `527-543`, §2.13 spacer `545-554`, §2.14 canvas deferred `556-566`, DSL name still open `661`
- `src/layout/flow.rs` header and `Cargo.toml` package name `quire-pdf` only to confirm crate identity

Not read in full: paginate, table split/draw, backend emit, line-break. Those are the other explorer’s slice.

---

## Boundaries

- **This slice stops at the DSL and the content matrix.** How a rule would `split` / paint (y = space + thickness/2, unsplittable errors) is the layout-flow explorer’s file. Facts used from layout here are only the types the DSL already calls: `FlowNode`, `ImageNode::new`, `SpacerNode`, `check_content`, `LayerItem`.
- **Do not put the divider on `Layer` or `Shapes`.** Those closures are per-page page coordinates, produce `LayerItem`, and do not take a frame width. `Layer::line` is an arbitrary segment plus `Pen`. §2.3 and §2.12 already separate the two.
- **Do not put it on `Paragraph::text`.** That builder only concatenates one string into `FlowNode::Text`. Inline blank is unimplemented. A sibling `Flow` method is how `spacer` and `image` enter the body, a column, a keep-together group, and a list item.
- **A `Flow` method alone does not enter a cell.** `CellContent` has two arms. `Table::lower` maps only those two. `Cell::image` is the precedent for a second cell constructor that reuses a flow builder (`ImageBox`) and returns `Cell`, because `row` takes `Into<Cell>`, not `FnOnce(&mut Flow)`.
- **A `Flow` method is callable in a list item and a column immediately**, then `check_content` may reject it. `Image` is the live example (legal on `Flow`, illegal in `ListItem` and `Band`, legal in `Column` and `TableCell`). §2.8 says a list item should contain 线; §2.6 says a cell should. The spacer arm is the one that is already `true` for both, plus columns. Copying the image arm would **forbid** the rule in list items.
- **Root `Stack` is not checked.** Body `page_break` and `spacer` rely on that. A new variant still has to be named in `check_content` or the crate does not build, even if the root never calls it.
- **`Band` must not gain a flow-rule method** for §2.3. If the new variant is `=> true` like `Spacer`, a band that somehow contained it would pass `measure_band`. Today `Band::lower` cannot produce one. That is an internal allow, not a public header API.
- **`TextAlign` cannot be a public argument** without a new export. Image exposes it as `center()` / `end()` on the builder, default `Start`. §2.3’s default is `Center`. Reuse the enum inside the node; do not copy image’s default.
- **`Stroke` width is `Positive` (cannot be 0). Spacer height and §2.3 `space_before` / `space_after` match `NonNegative` (can be 0).** Table `Border` is the zero-means-none type; `Pen` is the “always leaves a mark” type (`geom.rs:539`). `Layer::line` takes one `Pen`, which covers color+thickness only, not width, align, or the two spaces. `ImageBox` is the precedent for several optional fields as `&mut self` setters. `spacer` is the precedent for a single required `Length`.
- **Proposal DSL name is not closed.** `docs/提案书.md:661` lists `hline` / `rule` as still open. This file does not pick one. No method of either name exists.
- **Explicit `stack` is not a public builder** (§2.4, and there is no `Flow::stack`). The body is an implicit stack. Nested blocks are `columns` / `keep_together` / `ul` / `ol` / table cells, not a stack node the caller writes.
- **First error channel vs second.** `Positive::pt` / `NonNegative::pt` `?` fails the `document` closure and `paint` does not run (`tests/document.rs:416-421`). `Flow::push` stores `Error` and `document()` still returns it after the closure returned `Ok` (foreign image, `582-588`; column count, `397-412`). `Layer` errors are a third channel: stored on `Layer.error` and returned when the hook runs during `paint` (`1100-1112`, `1168-1171`).

---

## Non-Obvious Things

- `Flow::paragraph` pushes `FlowNode::Text`, not `FlowNode::Paragraph`. `check_content` names both `"paragraph"` (`lower.rs:69`). The laid-out `ParagraphNode` appears only after wrap. A rule should not be modeled as a paragraph variant if the goal is “not inline blank”.
- `ImageBox` is shared by `Flow::image` and `Cell::image`, but the **doc-token check** is in `ImageBox::lower`, so `Flow::image` checks at `push` time and `Cell::image` checks only in `Table::lower`. A cell constructor that only stores a builder defers `lower` the same way.
- `Cell::image` copies `Image` (`Copy`) into `CellContent` (`803`). The closure runs immediately, not at paint.
- List-item and column `Flow`s are real `Flow` values (`nested` → `child`). They are not a reduced subset type. Any new `pub fn` on `impl Flow` shows up in `item(|i| i....)`, `col(|c| c....)`, and `keep_together(|g| g....)` with no further wiring. Cell, `Band`, and `Layer` do not.
- `keep_together` at the root does not run `check_content` on its children (`lower.rs:38-42` only recurses when the container is `Column` and the node is still `Open`). A rule inside root `keep_together` is a `Flow` call plus whatever `split` does later.
- `Columns` inside a `Column` is `allowed = true` **without** a recursive `check_content` in that match arm (`lower.rs:44-45`). The inner `ColumnsNode::new` already checked those kids as `Column`.
- `Stack` inside a cell is legal, and an illegal child is reported under the **child’s** `node_name`, not `"stack"` (`table.rs:646-654`).
- `Flow::push` drops work after the first error (`541-543`). `columns` with one column fails the whole `Flow` even if a later `table` is also wrong (`tests/document.rs:397-412`).
- `reject_pushes` makes post-`section()` body calls, including a future rule method, return `ContentOutsideSection` instead of appending (`526-547`).
- `Spacer` is allowed in a table cell by the matrix and by §2.13, but no public `Cell` constructor emits `FlowNode::Spacer`. The gap is the DSL, not `check_content`.
- `List` is allowed in a table cell by the matrix (`lower.rs:53`) and by §2.8 “格内列表”. The public cell still cannot build a `FlowNode::List`. Same shape of gap as spacer.
- Image alignment methods never set `Start` because `Default` already did. A rule whose default is `Center` cannot reuse `ImageBox::default` unchanged.
- `Layer::line` and a flow rule would both end as a stroked `Path`, but the layer path is baked in page space at hook time (`straight_path`, `1146-1151`) and the flow stroke is a `FragmentCommand` relative to a fragment origin (`flow.rs:45-46`, `86-90`). Sharing `Pen`/`Stroke`/`Color` does not share the node type.
- `src/layout/mod.rs` being `pub` does not make `FlowNode` public. Adding a public builder type follows `ImageBox`: defined in `document.rs`, re-exported from `lib.rs:21-24` if callers can name it. Closures today usually do not name `ImageBox`; the type is still public.

---

## Open Questions

- DSL name `hline` vs `rule` is explicitly unfixed (`docs/提案书.md:661`). Code has neither.
- §2.3 does not fix default thickness, default color, or default `space_before` / `space_after`. `Stroke` cannot be 0 pt. `default_style` color is black, and nothing mutates it. “线色默认跟当前文字色或黑” (`docs/提案书.md:257`, written under inline blank §2.2) has no per-paragraph color to follow. §2.3 itself says 线粗/线色 go through §2.0 and the default stroke is solid (`278`), without a numeric default.
- Whether a cell rule is `Cell::hline(...) -> Cell` (precedent: `Cell::image`) or a cell `Flow` closure (no precedent; would be a new content model). The internal `Vec<FlowNode>` can hold either a one-node vec, which is all `Table::lower` builds today.
- Whether `check_content` should treat the new variant like `Spacer` (`true` in all four containers, including `Band`) or carve `Band` out. Public `Band` cannot emit it either way. §2.3 does not mention headers.
- §2.8 forbids table/columns in a list item and allows 线. The matrix already forbids `Table` and `Columns` in `ListItem`. It also forbids `Image` and `KeepTogether`, which §2.8’s one-line allow list does not mention. A rule arm should not inherit the image restriction if §2.8 is the list-item rule.
- Public `TextAlign` vs `start`/`center`/`end` methods: image chose methods and a `Start` default. §2.3 wants `Center` default plus Start/End. No existing builder defaults to `Center`.
- `Flow::image` takes the payload **outside** the closure (`&Image`) because the payload is required and the closure is optional config. A rule whose every field is optional matches `paragraph` / empty `image` closure more than `spacer`’s single required argument, and more than `Layer::line`’s three required arguments. That is a precedent map, not a signature in the tree.
