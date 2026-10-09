# Knife 13 — public document DSL (style / bold / first-line indent)

Tree: `feat/style-builders` @ `f208329`. Facts from the working tree at that commit. No style-builder, `bold`, or first-line-indent implementation is present.

## Components Found

### Public re-exports (`src/lib.rs`)

`src/lib.rs:21-24` re-exports from `document`:

`document`, `Band`, `Cell`, `Columns`, `Document`, `Flow`, `HLine`, `Image`, `ImageBox`, `Layer`, `List`, `Numbering`, `Page`, `Paragraph`, `Rendered`, `Section`, `Shapes`, `Table`.

`src/lib.rs:30` re-exports `FontId`.

Not re-exported, and not part of the callable DSL: `Style`, `TextAlign`, `TextNode`, `FlowNode`, `ParagraphNode`, `FontStore`. `src/lib.rs:34` is `pub mod layout`, but `Style` / `TextNode` / `FlowNode` are `pub(crate)` (`src/layout/style.rs:10-25`, `src/layout/paragraph.rs:23-26`, `src/layout/flow.rs:140-152`). An outside crate can name the `layout` module and still cannot name those types.

`Error` has no `UnknownStyle` variant (`src/error.rs:15-124`).

### Entry: `document()`

`src/document.rs:42-56`:

```rust
pub fn document<'a>(
    page_size: PageSize,
    build: impl FnOnce(&mut Document<'a>) -> Result<()>,
) -> Result<Rendered>
```

Constructs `Document::new`, runs `build` once, then `doc.paint` into a `RecordingCanvas`. The closure's `Err` returns before paint (`:47`). Doc comment (`:36-41`): builder mistakes are deferred and the first one is returned from `document()` before any page is drawn; a missing glyph anywhere, including headers and layers, fails the whole document. Text uses the first font passed to `Document::font`, 12 pt, line height 1.2, black, start-aligned. Margins default to zero.

### `Document` / `Section` / `Flow`

`Document` (`src/document.rs:187-196`) holds `page_size`, a `DocToken`, `FontStore`, `body: Sections`, a deferred `error`, and (with `backend-krilla`) an image table.

`Sections` (`:62-69`):

- `Implicit(Section)` — initial state.
- `Explicit { sections, closed: Flow }` — after the first successful `section()`. `closed` rejects later body pushes.

`Document` derefs to `Flow` (`:553-570`). Implicit: the lead section's flow. Explicit: the rejecting `closed` flow, not any section flow. `Section` also derefs to its own `Flow` (`:172-184`). So `doc.paragraph` and `section.paragraph` are both `Flow::paragraph`.

`Document::new` (`:199-209`) starts `Sections::Implicit(Section::new(default_style(), doc))` with an empty `FontStore`.

There is no public `stack()`. The body list becomes one node only at lower time: `FlowNode::Stack(flow.into_nodes()?)` (`:161`).

### `Flow` (`:585-750`)

Private fields: `style: Style`, `doc: DocToken`, `nodes: Vec<FlowNode>`, `error: Option<Error>`, `reject_pushes: bool`.

Public methods (all return `()`, not `Result`): `image`, `paragraph`, `columns`, `table`, `keep_together`, `spacer`, `hline`, `page_break`, `ul`, `ol`.

`paragraph` (`:660-665`):

```rust
pub fn paragraph(&mut self, f: impl FnOnce(&mut Paragraph)) {
    let mut paragraph = Paragraph::default();
    f(&mut paragraph);
    let node = self.text(paragraph.text);
    self.push(Ok(node));
}
```

One call appends exactly one node. The closure cannot see `Flow.style`. Style is applied after the closure returns.

### `Paragraph` (`:753-764`)

```rust
#[derive(Default)]
pub struct Paragraph {
    text: String,
}

impl Paragraph {
    pub fn text(&mut self, s: impl AsRef<str>) {
        self.text.push_str(s.as_ref());
    }
}
```

Only method is `text`. No `bold`, `style`, `font`, `font_size`, `color`, `align`, `line_height`, or indent. Repeated `text` calls concatenate into that one `String` (`push_str`, no separator). The closure that adds nothing still lowers to `TextNode` with `""`.

`Flow::text` (`:646-648`), private:

```rust
fn text(&self, text: String) -> FlowNode {
    FlowNode::Text(TextNode::new(text, self.style.clone()))
}
```

### `Style` (`src/layout/style.rs:16-48`)

`pub(crate) struct Style`:

| field | type | value from `default_style()` (`src/document.rs:573-581`) |
| --- | --- | --- |
| `font` | `FontId` | `FontId::from_raw(0)` |
| `font_size` | `Length<Positive>` | 12 pt |
| `line_height_factor` | `f32` | `1.2` |
| `color` | `Color` | `Color::rgb(0, 0, 0)` |
| `paragraph_spacing` | `Length<NonNegative>` | `NonNegative::ZERO` |
| `align` | `TextAlign` | `TextAlign::Start` |

`TextAlign` (`style.rs:10-14`): `Start`, `Center`, `End`. Crate-private. No justify.

`Style::new` (`style.rs:28-47`) rejects non-finite or `<= 0` `line_height_factor` as `InvalidGeometry`. `default_style()` builds the struct literal and does not call `Style::new`. The constants above pass that check anyway.

`Style` is `Clone + PartialEq`, not `Copy`. No `bold`, `italics`, weight, background, character spacing, decoration, opacity, margin, or first-line indent field. No `Default` impl on `Style` itself (`Paragraph`'s `Default` only zeros the `String`).

Nothing in `src/` assigns `Flow.style` after `Flow::new` / `Flow::rejecting`. Grep of `.style` writes in `src/` only finds clones and the band/layer align override (`document.rs:1066-1068`, `decorate.rs:197-200`).

### Font registration

`Document::font` (`document.rs:233-241`):

```rust
pub fn font(&mut self, bytes: Vec<u8>) -> Result<()> {
    self.fonts.register(FontSource { bytes: Arc::new(bytes), index: 0 }).map(drop)
}
```

`FontSource.index` is the collection face index, always `0` here, not the `FontId`.

`FontStore::register` (`src/fonts.rs:76-122`): id is `FontId::from_raw(faces.len() as u32)` before push. First success is `0`, then `1`, `2`, … Returned id is dropped. No public setter puts a `FontId` onto a paragraph. Every `TextNode` keeps `FontId(0)` from `default_style()`, assigned in `Document::new` before any `font()` call.

Consequences:

- Zero fonts registered: nodes still carry font `0`. Lookup is `FontStore::face` (`fonts.rs:129-132`) → `Error::UnknownFont { id }`.
- Two successful `font()` calls: faces `0` and `1` exist; DSL text still uses `0`. There is no handle to select face `1`.

`font()` and `load_image()` do not count as writing the document (`document.rs:290-293`, `Section::written` at `:95-102` looks at flow nodes/error, non-default margins, and page hooks only).

`FontId` (`fonts.rs:18-30`) is a public newtype `FontId(pub(crate) u32)` with `from_raw` / `as_u32`.

### `TextNode` / `ParagraphNode` (`src/layout/paragraph.rs`)

`TextNode` (`:23-39`): private `text: String` + `style: Style`. `new` stores both by value. `split` (`:37-38`):

```rust
ParagraphNode::layout(ctx.fonts, &self.text, &self.style, avail.width)?.split(avail)
```

Comment (`:36`): every outcome hands back a laid-out `Paragraph`, so a page move never wraps twice.

`ParagraphNode` (`:44-49`) keeps `lines`, `line_height`, `spacing_after`, `width`. It does not keep `Style` or the source string. `layout` (`:52-91`):

- Rejects non-finite or `<= 0` width.
- `ClusteredText::new(text)` then `measure_text(fonts, style.font, …)`.
- `break_lines(&clustered, width, …)` — one width for the whole paragraph (`:65-67`).
- Line box x from `glyph_run_for_line` → `align_x(frame_width, line_width, style.align)` (`:248`, `:260-265`). `Start` → `0.0`, `Center` → `(frame_width - line_width) / 2`, `End` → `frame_width - line_width`. Same `frame_width` for every line. No first-line x offset.
- `spacing_after` is `style.paragraph_spacing.as_pt()` (`:89`).
- `GlyphRun` receives `style.font`, `style.font_size`, `style.color` only (`:250-257`).

`break_lines` (`src/text/line_break.rs:43-47`) takes one `avail: f32`. `pack_lines` (`:101-151`) compares `line_width + width <= avail` for every line (`:128`). No per-line width argument.

`FlowNode` (`src/layout/flow.rs:140-160`): `Text` and `Paragraph` are different variants. DSL lower emits `Text`. `Text` splits into `Paragraph`. `Paragraph::split` does not re-wrap; it only slices the baked `lines` (`paragraph.rs:95-137`).

Downstream paint record: `GlyphRun` (`src/canvas.rs:119-132`) has `font`, `font_size`, `color`, `origin`, `glyphs`, `text`. No weight, no indent.

Test `paragraph_enters_the_flow_unwrapped` (`document.rs:1316-1325`) asserts the implicit flow is `[FlowNode::Text(text)]` with `*text == TextNode::new("A B", default_style())`.

### Other builders on the same DSL (pattern, not style)

Closure mutates a small builder, then the parent lowers it:

- `ImageBox` (`document.rs:429-473`): `width` / `height` / `center` / `end`. `align` is `TextAlign`, default `Start`.
- `HLine` (`:483-550`): width, thickness, color, spacing, `start` / `end`. Default align `Center`. No text style.
- `Columns` (`:767-790`): `col` takes `&mut Flow`. `lower` forces every column `ColumnWidth::Ratio(1.0)` and `default_column_gap()`.
- `Table` (`:794-858`): `header` / `row` / `repeat_header` (default on). Column count is the span sum of the first row (`:826-831`). All columns equal ratio. `..TableSpec::new` (`layout/table.rs:35-47`) sets padding `0` and border width `0`.
- `List` (`:960-991`): `item` / `item_marked`. `lower` sets `marker_width` 18 pt, `nest_indent: None`, `marker_style: self.parent.style` (move).

`Flow::child` (`:615-617`) clones `style` and `doc` into an empty flow. Used by `columns` (`:670`), `ul`/`ol` via `list` (`:744`), and `nested` (`:633-636`) which `keep_together`, `Columns::col`, and `List::item` / `item_marked` use.

### `Cell` (`document.rs:861-942`)

Public `Cell` is not `layout::table::Cell`.

`CellContent` (`:871-875`): `Text(String)` | `Image(Image, ImageBox)` | `HLine(HLine)`.

Constructors: `Cell::new(impl Into<String>)`, `Cell::image`, `Cell::hline`, then consuming `col_span` / `row_span`. `From<&str>`, `From<String>`, `From<&String>` (`:926-941`).

No paragraph closure, no style, no runs. `Table::header` / `row` take `Into<Cell>` (`:802-817`).

`Table::lower` (`:825-857`) turns `CellContent::Text(text)` into `vec![flow.text(text)]` — one `TextNode` with the enclosing `Flow`'s style, cloned at lower time, not at `Cell::new` time. Image and hline do not use that style.

`layout::table::Cell::new` (`table.rs:58-63`) wraps nodes in `CellBody::new` → `check_content(Container::TableCell, …)` (`lower.rs:90-92`). Allowed in a cell (`lower.rs:33-58`): text/paragraph/spacer/hline, image, stack of those. Rejected: columns, table, page break, keep-together. List is allowed in a cell (`List` unless container is `Band`). The public `Cell` never builds those nested nodes; only the three `CellContent` arms exist.

Cell wrap width is later, not at DSL time. `measure` (`table.rs:201-221`): `inset = border/2 + padding` (both `0` from `TableSpec::new`), `inner_w = span width - 2*inset`, then `natural_height` on a frame of that width. With current defaults, text wraps at the column-span width. Same `TextNode::split` → one `avail.width` for every line.

### `Band` (`document.rs:1041-1113`)

Fields: `page`, `left: Option<String>`, `right: Option<String>`. Methods: `page`, `left`, `right`. Each setter replaces the slot (`Some(...)`), last call wins, no concatenation, no runs, no style argument.

`Band::lower` (`:1064-1100`): for each present slot, `Style { align, ..style.clone() }` with `Start` or `End`, then one `TextNode`. Both slots become equal-ratio columns with gap `0`, plus fixed empty columns for non-zero left/right margins. Empty band returns an empty vec. Single slot returns that slot's nodes without a columns wrapper (`:1093-1094`).

`band_content` (`:1103-1112`) moves the `Style` into the hook. The hook runs at page-measure time (`decorate.rs:141-145`), builds a `Band`, calls `lower`.

`measure_band` (`decorate.rs:248-274`) splits `FlowNode::Stack(nodes)` against a frame whose width is the full page width (`size.width()`), height the full page height. Fit with `height <= margin` is kept. Taller than the margin, a partial split, or move-to-next-page → `DecorationTallerThanMargin`. `UnbreakableWiderThanFrame` is rewritten to `DecorationWiderThanFrame`. So band text does go through normal wrapping, then must fit on one band. It is not a separate single-line API, unlike layer text.

### `Layer` (`document.rs:1116-1272`)

Fields: `page`, `style: Style`, `doc`, `items: Vec<LayerItem>`, `error`.

`text_at` (`:1138-1144`): pushes `LayerItem::Text(LayerText { text, at, style: self.style.clone() })`. Doc comment: one line, top-left of the line box at `at`, not wrapped or clipped. No style argument.

`layer_content` (`:1258-1272`) clones `style` into each invocation. First `layer.error` fails the hook and drops `items`.

`measure_layer` (`decorate.rs:196-206`) rebuilds `Style { align: TextAlign::Start, ..item.style }` and calls `single_line_run(..., f32::INFINITY)`. `single_line_run` (`paragraph.rs:142-169`) measures the whole string as one run and errors with `UnbreakableWiderThanFrame` only if advance exceeds `max_width`. Infinity disables that check. Origin is then `at + run.origin`. Alignment stored on the layer style is overwritten to `Start` before drawing. This path does not call `break_lines`.

`LayerText` (`decorate.rs:49-55`): `text: String`, `at: Point`, `style: Style`. Comment: unwrapped; `at` is line-box top-left; alignment is ignored.

### Where the section style is captured for bands and layers

`Section::lower` (`document.rs:142-168`), called from `Document::paint` (`:355-356`):

```rust
let style = flow.style.clone();
// ...
header: header.map(|hook| band_content(hook, style.clone())),
footer: footer.map(|hook| band_content(hook, style.clone())),
behind: behind.map(|hook| layer_content(hook, style.clone(), doc)),
front: front.map(|hook| layer_content(hook, style, doc)),
```

Capture is at paint, from that section's `Flow.style`. Today that value is still `default_style()` because nothing mutates it. Bands and layers do not read a separate document-level default, and they do not read per-paragraph overrides (there are none).

`section()` (`:294-323`) clones style into the new `Section` from the implicit flow, or from `closed` if sections already exist. After the closure returns `Ok`, the rejecting `closed` flow is built from `section.flow.style.clone()` (`:314-318`). A later `section()` clones `closed.style` (`:306`). If a future API mutated `section.flow.style` inside the closure, `closed` would observe that mutation. No such API exists.

## Flow

### Assembly (one shot, no page count)

1. `document(page_size, build)` → `Document::new` → `FontStore::new()` + implicit `Section` whose `Flow.style` is `default_style()` (`FontId(0)`, 12 pt, 1.2, black, spacing 0, `Start`).
2. Caller may `doc.font(bytes)`. Id `0` on success is discarded. Style is not updated.
3. `doc.paragraph(|p| …)` via `DerefMut` to the implicit flow. `Paragraph::text` concatenates. Then `TextNode { text, style: flow.style.clone() }` is pushed.
4. `columns` / `ul` / `ol` / `keep_together` clone that same style into child flows (`Flow::child`). Nested `paragraph` calls clone it again into their `TextNode`s.
5. `table` stores public `Cell`s. At `Table::lower`, each text cell becomes `flow.text(string)` with the table's enclosing flow style.
6. `ul`/`ol` `lower` moves the list parent style into `ListSpec.marker_style`. Item bodies were already lowered with clones of that style.
7. `header` / `footer` / `behind` / `front` on an implicit document store hooks on the lead `Section` and do not run yet.
8. `section(|s| …)` clones the current style into a new `Section`. Inside it, `s.paragraph` uses that section flow. After the first success, further `doc.paragraph` hits the rejecting flow.

### Paint

1. `Document::paint` (`:332-377`): if `Document.error` is set, return it. Explicit mode also runs `closed.into_nodes()?`. Each section `lower`s: root `Stack`, decorations close over `flow.style`.
2. `paginate_sections` (`:371-372`). Body `Text` nodes wrap when first split, at that frame's `avail.width` (`paragraph.rs:37-38`).
3. Column frame width is the column width (layout columns; DSL does not pass a width). List body frame width is `avail.width - indent` where `indent` is the marker width (18 pt) because `nest_indent` is `None` (`list.rs:141-142`, `:172-187`). Marker is `single_line_run` in the 18 pt column (`list.rs:207-209`), and only while `pending_marker` is set (first fragment). That indent is the list hanging indent, not a paragraph first-line indent.
4. Cell text wraps at `inner_w` (`table.rs:211-221`).
5. After pagination, band/layer hooks run (`decorate.rs:101-165`). Band text wraps at full page width with slot align overridden. Layer `text_at` is one unwrapped run at page coordinates.

`W` is not known when the DSL closure runs. It is the frame width at `TextNode::split`. Proposal §2.1 `W' = W − indent` for the first line only is not applied: `break_lines` and `align_x` both see a single width, and `ParagraphNode` then forgets `Style`.

### Style clone graph (all copies are `default_style()` today)

```
default_style()
  └─ implicit Section.flow.style
       ├─ Flow::text → TextNode.style          (body paragraphs)
       ├─ Flow::child → nested Flow.style      (column, list, keep_together)
       │    ├─ their TextNodes
       │    └─ ListSpec.marker_style           (moved at List::lower)
       ├─ Table::lower → flow.text(cell string)
       ├─ section() clone → next Section.flow.style
       └─ Section::lower clone → Band hook and Layer hook
            ├─ Band::lower copies again, overrides align only
            └─ Layer::text_at copies onto LayerText
                 measure_layer forces align = Start, no wrap
```

There is no named-style map, no per-node override struct, and no inheritance merge. "Inheritance" is clone-by-value of one style. A child flow does not share the parent `Style` allocation; later mutation of the parent would not update already-cloned nodes. Nothing mutates it now.

### Plug-in points (absent; these are the seams)

**Inline `p.bold` (提案书 §1 template, lines 85-88: `p.text` then `p.bold`).**

- Must accumulate more than one `String` on `Paragraph`. `text` + `bold` today would be one concatenated string and one style if both wrote into `Paragraph.text`.
- Must not lower as two `FlowNode::Text` siblings. `Flow::paragraph` pushes one node; a second `Flow::text` push would be the next block, with its own wrap and `paragraph_spacing`.
- `TextNode` is one string + one `Style`. Mixed runs need a different node (runs inside one wrap) or a change to `TextNode`. `ParagraphNode` bakes `GlyphRun`s and drops `Style`, so run style must be read inside `ParagraphNode::layout` / `glyph_run_for_line`.
- `Style` has no weight bit. `GlyphRun` has one `FontId`. Bold is either another face or a new field. `Document::font` drops the id, so a second face cannot be named from the DSL.
- Cells do not use `Paragraph`. `CellContent::Text(String)` → `flow.text` (`document.rs:839`). A cell bold needs a change to `Cell` / `Table::lower`, or cells stay one style.
- `Band::left` / `right` are one `String` each. `Layer::text_at` is one `String`. Neither has a run builder.

**Paragraph style builder (font size, color, align, spacing, named style).**

- Public surface to extend is `Paragraph` and/or `Document` / `Flow`, not `layout::style` (crate-private, not re-exported).
- Today the only style input to a paragraph is `self.style.clone()` after the `Paragraph` closure (`document.rs:663-664`). A builder on `Paragraph` has to be read there; `Paragraph` is not given `&Flow`.
- A document-level named map does not exist. `Document` fields are `page_size`, `doc`, `fonts`, `body`, `error`, `images`. No `styles`, no `default_style` field separate from the flow's copy.
- `Flow.style` is private. No setter.
- Align is already writable on `ImageBox`, `HLine`, and band slots. Those write `TextAlign` on those objects, not on `Style` used by paragraphs. Band is the only text path that overrides `Style.align` (`document.rs:1066-1068`).
- `paragraph_spacing` is already on `Style` and becomes `ParagraphNode.spacing_after`, but the DSL cannot set it, and the default is `0` pt not the design draft's 6 pt.

**First-line indent (提案书 §2 attribute row and §2.1 `W' = W − indent`).**

- No field to store it. Proposal: it is part of style, not a new node; default amount is explicitly not decided (提案书 §2, line 155, 「具体默认值以后定」).
- Value would be stored at assembly (on the style or on the paragraph node) and consumed only in `ParagraphNode::layout`, because `split` of `ParagraphNode` does not re-wrap and no longer has `Style`.
- `break_lines` / `pack_lines` need a first-line avail distinct from later lines (`line_break.rs:128` uses one `avail`). `align_x` (`paragraph.rs:260-265`) would also have to place line 0. `Start` currently returns `0.0`, so a narrower first line still starts at x = 0 unless the origin is shifted.
- Frame `W` depends on where the paragraph sits, and is applied later: body frame, column width, cell `inner_w`, list `body_w`. Indent is a further reduction of the first line only. List hanging indent (`list.rs:172-173`) already shrinks every line of the item; it is a different mechanism (`nest_indent: None`, width = 18 pt marker column, `document.rs:983-987`).
- Band text uses the same `Style` and the same wrapper. An indent field on the shared `Style` would affect header/footer unless band `lower` cleared it.
- Layer text does not wrap (`single_line_run` + `f32::INFINITY`). 提案书 §2.1 (line 206): `behind` / `front` text is not flow wrapping. Indent does not belong on that path unless a new field is explicitly applied there. `measure_layer` already ignores `align`.

## Files Read

- `src/lib.rs` (re-exports, `pub mod layout`)
- `src/document.rs` (full DSL: `document`, `Sections`, `Section`, `Document`, `default_style`, `Flow`, `Paragraph`, `Columns`, `Table`, public `Cell`, `List`, `Band`, `Layer`, `paint`, tests through `margins_after_section_fail_before_any_page`)
- `src/layout/style.rs` (full)
- `src/layout/paragraph.rs` (`TextNode`, `ParagraphNode::layout` / `split`, `single_line_run`, `glyph_run_for_line`, `align_x`)
- `src/layout/flow.rs` `FlowNode` and `split` dispatch (`:140-169`)
- `src/layout/lower.rs` `check_content` / `CellBody` / `ItemBody`
- `src/layout/decorate.rs` `LayerText`, `measure_bands`, `measure_layer`, `measure_band`
- `src/layout/list.rs` `ListSpec`, `ListNode::new`, `split` marker + body width (`:90-219`)
- `src/layout/table.rs` `TableSpec::new` defaults and cell `inner_w` (`:25-47`, `:191-221`)
- `src/layout/mod.rs` (module list)
- `src/fonts.rs` `FontId`, `FontStore::register` / `face` (`:1-149`)
- `src/error.rs` `Error` variants through `ContentOutsideSection`
- `src/canvas.rs` `GlyphRun` (`:114-132`)
- `src/text/line_break.rs` `break_lines` / `pack_lines` (`:42-160`)
- `docs/提案书.md` §1 template (`:76-118`), §2 style attribute table (`:150-180`, `:183-191`), §2.0 typing (`:208-239`), §2.1 `W` / `W'` (`:193-206`)
- `docs/quire-design.md` style section (`:873-927`), public `Document` / `Paragraph` sketch (`:933-1015`), sample (`:1133-1199`), decoration note (`:857-869`)

## Boundaries

| Boundary | What crosses it |
| --- | --- |
| `document` → `layout::style::Style` | Same crate. `Style` stays `pub(crate)`. DSL never returns it. |
| `document` → `layout::paragraph::TextNode` | `Flow::text` and `Band::lower` construct `TextNode::new(string, style)`. One string, one style. |
| `document` → `layout::flow::FlowNode` | Paragraph → `Text`. Columns / table / list / image / hline / spacer / page break / keep-together have their own variants. Root is `Stack`. |
| `TextNode` → `ParagraphNode` | At first `split`, using that frame's width. Style is consumed. Later splits only slice lines. |
| `ParagraphNode` → `break_lines` | One `avail` width. Cluster widths use `style.font_size`. |
| `ParagraphNode` → `GlyphRun` | `font`, `font_size`, `color`, aligned origin. |
| `Document::font` → `FontStore` | Registers face, drops `FontId`. Does not write `Style.font`. |
| Public `Cell` → `layout::table::Cell` | Text arm calls `flow.text`. Then `table::Cell::new` runs `check_content(TableCell)`. |
| `List` DSL → `ListNode` | Marker style is the parent flow style. Body nodes already carry cloned style. Hanging indent is list geometry, fixed 18 pt from the DSL. |
| `Section::lower` → `decorate` | Hooks close over one `Style` clone. Band may override `align`. Layer text is not wrapped; measure forces `Start`. |
| `layout` → `krilla` | Not on this path. `GlyphRun` is the canvas type. Backend is feature-gated and not used while assembling the DSL. |

`document()` closure `Result` vs deferred builder `Result`: see Non-Obvious. Layout errors (`MissingGlyph`, `UnknownFont`, `UnbreakableWiderThanFrame`, decoration errors) happen in `paint` / `paginate` / `measure_bands`, after the build closure has returned `Ok`.

## Non-Obvious Things

### First-error-wins, two channels

**Immediate `Result` (closure or method returns it, paint may also store it):**

- `document`: `build(&mut doc)?` (`:47`). Closure `Err` skips paint.
- `Document::font`, `Document::load_image`: return `Err` now. They do not set `Document.error`.
- `Document::section` (`:294-323`):
  - If `self.error` is already `Some`, return that clone and do not run the closure (`:295-297`).
  - If the implicit section is `written()`, store `ContentOutsideSection` and return it (`:298-303`).
  - If the section closure returns `Err`, store that error and return it. The section is not appended (`:309-312`).
  - Display text of `ContentOutsideSection` (`error.rs:177-178`): `"body or decorations written both on the document and in sections"`.

**Deferred, first stored error wins, later errors dropped:**

- `Flow::push` (`document.rs:619-630`): if `self.error` is already `Some`, return without keeping the new node or the new error. If `reject_pushes`, set `ContentOutsideSection` and return. Otherwise `Ok` appends and the first `Err` is stored.
- `Flow::into_nodes` (`:639-644`): `Some(error)` fails; else returns `nodes`.
- Nested flows have their own `error`. `Columns::lower` (`:779-785`) and `List::lower` (`:988`) use `collect::<Result<_>>()?`, so the first column / first item error wins. `Table::lower` (`:833-849`) is row-major `?`, first cell error wins. `keep_together` pushes the `nested()` `Result` (`:693-697`).
- `Document::fail_outside` (`:326-330`): sets `ContentOutsideSection` only when `self.error.is_none()`. Does not overwrite a previous document error. Used by `margins` / `header` / `footer` / `behind` / `front` once `body` is `Explicit` (`:244-285`).
- `Document::paint` (`:342-349`): returns `Document.error` before lowering. Then, for `Explicit`, `closed.into_nodes()?`. Body methods after `section()` do not set `Document.error`; they set `closed.error` via `reject_pushes`. That surfaces at `into_nodes`, which is why `paragraph_after_section_fails_before_any_page` (`:1442-1450`) fails with an empty canvas. `margins_after_section` (`:1454-1462`) fails via `Document.error` instead. Both variants are `ContentOutsideSection`. If both ran, `paint` checks `Document.error` first.
- `Layer` (`:1150-1211`): `image_at`, non-finite `rotated` (does not run the draw closure; test `:1368-1377`), and `push_shape` set `error` only if it is `None`. `text_at` never fails. `layer_content` returns that error and discards `items` (`:1268-1271`).
- `Band` has no error field. Its `lower` `Result` is produced when the hook runs during `measure_bands`, inside paint. `measure_bands` documents that failure leaves the canvas untouched (`decorate.rs:97`).

`Paragraph::text` cannot fail. An empty paragraph is still `push(Ok(TextNode))`.

`section()` after a paragraph fails inside `section()` itself (`:1424-1438`), stores the error, and paint fails the same way without drawing.

### `ContentOutsideSection` vs "writing"

`Section::written` (`:95-102`) is true if the flow has nodes or a flow error, or margins are not default, or any of header/footer/behind/front is set. `page_size` is not part of `written`. Font and image registration are not either.

After the first successful `section()`, `Document`'s deref target is the rejecting flow (`:564-569`). Decoration methods on `Document` no longer forward; they `fail_outside`. The same methods on `Section` (`:117-140`) still set that section's hooks. A later call replaces the previous hook (`Option` assignment).

### One string, one style, even when the template shows two calls

提案书 §1 (`docs/提案书.md:85-88`) is `p.text("合同编号 "); p.bold(&contract.id);` inside one `paragraph` closure. The implementation has only `Paragraph::text`, and several calls become one `String`. There is no span boundary left by the time `Flow::text` runs.

### Font id 0 is a convention, not a lookup of "the first font" at paint

`default_style` hardcodes `FontId::from_raw(0)` at document construction (`document.rs:575`). It matches the first successful `register` only because ids are dense from 0 (`fonts.rs:108-111`). Registering does not patch existing `Style` values. The doc comment on `document()` (`:40-41`) and on `font()` (`:233`) say the first font is the one text uses. That is this hardcoded id, not a search at layout time.

### `Style::new` is not on the DSL path

Validation of `line_height_factor` exists (`style.rs:36-38`) and is skipped by `default_style()`. Fields are `pub(crate)`, so `document.rs` can construct `Style` directly. Band align override also uses a struct update, not `Style::new`.

### Empty text still becomes a line

`break_lines` on empty text returns one zero-width line record (`line_break.rs:52-65`). `ParagraphNode::split` treats `lines.is_empty()` as a zero-height fit (`paragraph.rs:99-103`), but the empty-string layout path does not produce an empty `lines` vec. A `paragraph` closure that never calls `text` still pushes a `TextNode`.

### List indent vs first-line indent

`List::lower` always passes `nest_indent: None` and `marker_width` 18 pt (`document.rs:983-987`). `ListNode` sets `indent = marker_width` (`list.rs:142`). Every line of the item body is wrapped at `avail.width - indent` (`:172-187`). The marker is drawn only when `pending_marker` is still `Some` (`:207-212`), i.e. the item's first fragment. Continuation pages keep the body indent and do not repeat the marker. That matches list hanging indent. It does not implement `W' = W − indent` on the first line of a paragraph, and the DSL cannot set `nest_indent`.

### Cell / band / layer do not share the paragraph builder

- Cell text is a `String` lowered with the table flow's style. No per-cell style.
- Band slots are `Option<String>`, last write wins, one `TextNode` per slot, align forced to `Start` or `End`, other style fields copied.
- Layer `text_at` copies the section style, then measure forces `Start` and skips wrapping. 提案书 §2.1 says page-coordinate text does not use flow line breaking. The code matches that. A first-line indent implemented only inside `break_lines` would not affect `text_at`.

### Design draft is not what the code does

`docs/quire-design.md` 「样式」 (`:873-927`) and the v1 `Document` sketch (`:933-1015`, sample `:1133-1199`) describe:

- Public `Style` with raw `f32` fields. Code: `pub(crate)`, `Length<Positive>` / `Length<NonNegative>`, `line_height_factor` still `f32`.
- `paragraph_spacing` default 6 pt in the sample (`:1172`) and in the struct comment (`:881`). Code default is `0` (`document.rs:579`).
- `StyleOverride` with per-field `Option`, plus `resolve_style(default, styles: BTreeMap<String, Style>, name, override)` (`design :885-925`). Resolution order: override `Some`, else named style, else default; unknown name is `UnknownStyle` and does not fall through. None of `StyleOverride`, `resolve_style`, the map, or `UnknownStyle` exist under `src/`.
- `Paragraph { spans: Vec<Span>, style: Option<String>, override_style }` with `Span::Text` and `Span::Link` (`design :982-991`). Code `Paragraph` is one `String`. No link span on the paragraph builder. `Error::EmptyLink` exists (`error.rs:32`) but is unused by `document.rs`.
- `Document` as a public struct with `default_style`, `styles`, and `content: Vec<Block>` (`design :936-946`). Code `Document` is a closure builder; `content` is a private `Vec<FlowNode>` inside `Flow`.
- Decorations use `default_style` only, not a paragraph `StyleOverride`; three slots of `Inline` including `PageNumber` / `PageCount`; no callbacks (`design :857-869`). Code: callbacks; `Band` is left and right strings only (no center); the caller formats page numbers (`Page` is passed in, `:1000-1021`). The style those callbacks receive is the section flow style, which is currently equal to `default_style()` only because nothing else writes it. There is no second "default vs override" value to ignore.
- Draft font table returns the `FontId` from `insert` (`design :1143`). Code `Document::font` returns `()`.

提案书 header (`docs/提案书.md:7-8`) says unconfirmed design-draft decisions do not count. §1's closure DSL is the confirmed call shape, including `p.bold`, and that call is not implemented. §2 lists `bold`, `italics`, `font`, `fontSize`, `color`, `background`, `alignment`, `lineHeight`, `characterSpacing`, `decoration`, `opacity`, `margin`, and 首行缩进 as node attributes (`:154-155`). Of those, the internal `Style` only has font, font size, line-height factor, color, paragraph spacing, and align — and the DSL can set none of them on a paragraph. §2.0 says positive lengths and non-negative lengths should be newtypes (`:212-218`). `font_size` and `paragraph_spacing` already are. A first-line indent added as a raw `f32` would not match that rule. §2.1 `W' = W − indent` (`:204`) is confirmed and not implemented. §2 also says the numeric default for 首行缩进 is not decided yet (`:155`).

`src/layout/style.rs:1-3` allows `dead_code`. `Style::new` is part of that; the DSL does not need it.

## Open Questions

- Bold as a second registered face vs a flag on a run. The DSL drops every `FontId`, and `Style` / `GlyphRun` have no weight. The proposal template does not say which.
- Whether a style builder lives on `Paragraph` (per paragraph / per run), on `Document` (named styles / default), or both. The confirmed §1 snippet only shows `p.bold` inside one paragraph. §2 also lists document-level `styles` / `defaultStyle` (`提案书.md:176`) in the pdfmake inventory; the "still enumerating" intro at §2 (`:126-128`) conflicts with later "已定" subsections. This file does not resolve that product question. The code has neither.
- First-line indent's default (em count vs pt) is explicitly postponed (`提案书.md:155`). Interaction with `TextAlign::Center` / `End` is not specified. Current `align_x` uses the full frame width for every line.
- Whether indent is paragraph-level (one value for the paragraph's first line) while bold is run-level. The code has a single string, so both seams are unbuilt. Putting indent on the shared `Style` would also flow into band text and list markers unless those paths clear it.
- Cell, band, and layer text cannot express `p.bold` or a paragraph style without new builders. Whether knife 13 includes them is not stated in the code.
- `line_height_factor` remains a raw `f32` checked in `Style::new`, which the DSL bypasses. §2.0's "positive length" rule is stated for 字号, not for the unitless factor.
