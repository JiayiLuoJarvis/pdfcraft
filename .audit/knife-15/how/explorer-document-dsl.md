### Components Found

No `src/document/` directory. The public DSL is one module: `src/document.rs`. Run types live in `src/layout/paragraph.rs` and are `pub(crate)`. `src/layout/mod.rs` is `pub mod layout`, but every paragraph/flow/style type below is `pub(crate)`, so they are crate-visible and not nameable from outside the crate. `src/lib.rs` re-exports the public builders (`document`, `Document`, `Flow`, `Paragraph`, `Blank`, `Band`, `Cell`, …) and, separately, `LinkTarget`.

**Public entry (assemble once).** `document()` (`src/document.rs` 42–56) constructs `Document`, runs the caller closure once, then `Document::paint`. `Document` and `Section` both `Deref`/`DerefMut` to `Flow` (571–589, 172–184), so `paragraph` / `columns` / `ul` / … are called on the document or a section. After the first `section()`, further body pushes hit a rejecting `Flow` and become `Error::ContentOutsideSection` (623–628, 642–644).

**`Flow` (`src/document.rs` 604–776).** Fields: `style: Style`, `doc: DocToken`, `nodes: Vec<FlowNode>`, `error: Option<Error>`, `reject_pushes: bool`. All private. The only rich-text builder is public `Flow::paragraph` (678–690). Private `Flow::text` (665–667) is the plain-string path used by table cells. Nested containers (`columns`, `keep_together`, `ul`/`ol` items) clone `style` via `Flow::child` (634–636) and collect `FlowNode`s. There is no public `Flow::text(String)`.

**`Paragraph` (`src/document.rs` 778–873).** Public struct, private fields:

- `items: Vec<InlineItem>`
- `first_line_indent: Length<NonNegative>` (default `NonNegative::ZERO`)
- `error: Option<Error>`

`Default` is public, but nothing accepts an externally built `Paragraph`. `Flow::paragraph` always does `Paragraph::default()`, runs the closure, then lowers. Public methods: `text`, `bold`, `blank`, `blank_em`, `blank_to_end`, `first_line_indent`. No `link`, `uri`, color, font size, or align method.

**`Blank` (`src/document.rs` 876–912).** Public stroke handle for the blank just pushed: `thickness` (default 1 pt), `color` (`Option`, default none → paragraph text color at draw time), `baseline_offset` (default 1 pt). Holds `paragraph: &mut Paragraph` and `index`. If the push was rejected, `index` is `usize::MAX` and setters no-op (`blank_mut` 906–911).

**Run types (`src/layout/paragraph.rs`, all `pub(crate)`).**

- `Face` (15–19): `Body` | `Bold`. `Copy`.
- `TextRun` (22–34): `text: String`, `face: Face`. Constructor `TextRun::new`. No URI, link, color, or font-id field. `PartialEq`.
- `InlineItem` (37–40): `Text(TextRun)` | `Blank(InlineBlank)`.
- `InlineBlank` (50–66): `width: BlankWidth`, `thickness`, `color: Option<Color>`, `baseline_offset`. `BlankWidth` (43–47): `Fixed(Length<NonNegative>)` | `Em(u16)` | `ToEndOfLine`.
- `TextNode` (91–118): `items: Vec<InlineItem>`, `style: Style`. Constructors:
  - `new(text, style)`: empty string → `items: vec![]`; otherwise one `InlineItem::Text(TextRun::new(text, Face::Body))`.
  - `from_runs`: maps each `TextRun` to `InlineItem::Text`, no coalesce, no empty filter.
  - `from_items`: stores the vec as given.
- `split` (116–118) calls `ParagraphNode::layout_runs` → `prepare_inline`.

**`Style` (`src/layout/style.rs` 18–26, `pub(crate)`).** One value per `TextNode`, not per run: `font: FontId`, `font_size: Length<Positive>`, `line_height_factor: f32`, `color: Color`, `paragraph_spacing: Length<NonNegative>`, `first_line_indent: Length<NonNegative>`, `align: TextAlign` (`Start` | `Center` | `End`). `default_style()` (`src/document.rs` 591–601) is the only production constructor used by the DSL: font id 0, 12 pt, line height 1.2, black, spacing 0, indent 0, `TextAlign::Start`. Grep of `src/` found no assignment to a `Flow.style` field after `Flow::new`. Callers cannot change font size, color, align, spacing, or line height through the DSL. `Paragraph::first_line_indent` overwrites that one field at lower time (686–687); last call wins.

**`FlowNode::Text` (`src/layout/flow.rs` 140–161).** Body paragraphs, table-cell strings, and header/footer slots all become `FlowNode::Text(TextNode)`. `FlowNode::split` dispatches that variant to `TextNode::split`. `FlowNode::Paragraph(ParagraphNode)` exists but the DSL never builds it; `TextNode::split` produces it only after line breaking. `FragmentCommand` (81–96) is `Glyphs` | `Fill` | `Stroke` | `Image`. No link variant. `draw_fragment` (347–376) calls `draw_glyph_run` / `fill_path` / `stroke_path` / `draw_image` and never `annotate_link`.

**Other text surfaces (not the `Paragraph` assembler).**

- Table cells (`src/document.rs` 1018–1032, 986–992): `CellContent::Text(String)` via `Cell::new` / `From<&str|String|&String>`. `Table::lower` maps each to `vec![flow.text(text)]` → `TextNode::new` (one `Face::Body` run, or empty items). No `bold` / `blank` / paragraph closure inside a cell. `CellContent` is also `Image` and `HLine` only.
- Header/footer `Band` (1189–1260): `left: Option<String>`, `right: Option<String>`. `left`/`right` store the string (`Some`, including `""`). No `Paragraph`. `Band::lower` (1212–1247) builds, per present slot, `FlowNode::Text(TextNode::new(text, Style { align, ..style.clone() }))` and, when both slots or a non-zero side margin exist, a `Columns` node. Empty `Option` is omitted; `Some("")` is not. Captured `style` is the section style cloned in `Section::lower` (156–164) and closed over by `band_content` (1251–1260). The hook runs per page inside `measure_bands` → `measure_band` (`src/layout/decorate.rs` 248–265), which `FlowNode::Stack(nodes).split`s, so those `TextNode`s do enter `TextNode::split` / `prepare_inline`.
- `Layer::text_at` (1286–1291) pushes `LayerItem::Text(LayerText { text: String, at: Point, style: Style })` (`src/layout/decorate.rs` 49–55). Not a `TextNode`. Drawn later by `single_line_run` (`src/layout/paragraph.rs` 252–281): `ClusteredText::new` + `measure_text` on `style.font` only, no wrap, no `Face`, no `InlineItem`.
- List markers (`src/layout/list.rs` 207–212): label `String` → `single_line_run`, not `TextNode`. Item bodies are nested `Flow`s, so they can contain `paragraph()`. `List::item_marked` rejects an empty marker string (`src/document.rs` 1120–1127, `src/layout/list.rs` 113–119).
- `ImageNode` (`src/layout/image.rs` 11–15): `DecodedImage`, `Size`, `TextAlign`. No URI. `ImageBox` (`src/document.rs` 447–451) is width/height/align only.

**Existing URI type (canvas only, not wired to runs).** `LinkTarget(String)` (`src/canvas.rs` 30–51), public, re-exported from `src/lib.rs` 26–29. `LinkTarget::uri(uri)` returns `Error::EmptyLink` when `uri.is_empty()`; spaces are kept; no scheme check. `as_str()` returns the string. Not `Copy` (`String` inside). `PartialEq + Eq + Hash + Clone`. `Canvas::annotate_link(&mut self, rect: Rect, target: LinkTarget)` (207–210). `CanvasCommand::AnnotateLink { rect, target }` (173–179). `Error::EmptyLink` (`src/error.rs` 31–32, display `"link URI is empty"` at 134). Call sites of `LinkTarget` / `annotate_link` are `src/canvas.rs`, `src/backend/krilla_canvas.rs`, `src/backend/emit.rs` tests. `src/document.rs` does not mention `LinkTarget`, `uri`, or `annotate_link`. Layout never calls `annotate_link`.

**Face resolution is not assemble-time.** `Paragraph::bold` only stores `Face::Bold`. `measure_inline` (`src/layout/paragraph.rs` 491–507) calls `fonts.bold()` only if some run has `Face::Bold` and non-empty text. `FontStore::bold` (`src/fonts.rs` 134–136) is `UnknownFont { id: 1 }` when `Document::bold_font` was never called. That error happens at paint (`src/document.rs` test `bold_and_indent_failures_happen_before_any_page`, 1649–1652), not in `append`. `Document::font` registers bytes; it does not write `Style.font` (still raw id 0). `Document::bold_font` (246–258) requires a body face at id 0, registers a new face, `set_bold`. A later call replaces the bold binding.

**Design draft is not this code.** `docs/quire-design.md` sketches `Span::Text` / `Span::Link { text, uri }` (around 988–991) and `FragmentCommand::Link`, and an image `link: Option<String>` (1017–1023). None of those types exist under `src/`. `docs/提案书.md` §8 (613–619) is the authority: text may carry a URI; after line breaking, write a link annotation rect per line fragment of that text; store the URI string only (no fetch, no scheme check); empty URI rejected at construct; v1 is external URI on body text and on header/footer if they share the pipeline; no in-doc jumps; image-only links are out of this knife. §2.0 (around 213) says a non-empty URI, if links exist, should be a newtype at the boundary. `LinkTarget` is that newtype and is already enforced. No destination / page-link / named-anchor type exists.

### Flow

1. `document(page_size, build)` → `Document::new` → implicit `Section::new(default_style(), doc)` → `Flow { style: default_style(), nodes: [] }` (`src/document.rs` 198–209, 82–92, 591–620).
2. Caller `doc.paragraph(|p| …)` (or the same on a `Section`, a column `Flow`, a `keep_together` `Flow`, or a list-item `Flow`). `Flow::paragraph` (678–690):
   - `Paragraph::default()`.
   - Closure calls `text` / `bold` / `blank*`.
   - On `paragraph.error`: `Flow::push(Err)` and return. Items are dropped. `Flow::push` keeps the first error and does not push a node (638–649).
   - Else clone `self.style`, set `first_line_indent` from the paragraph, `FlowNode::Text(TextNode::from_items(paragraph.items, style))`, push `Ok`.
3. `Paragraph::text` → `append(Face::Body, …)`. `Paragraph::bold` → `append(Face::Bold, …)` (797–804).
4. `append` (828–843):
   - `text.is_empty()` → return. This happens **before** the fill-blank seal check. `text("")` / `bold("")` after `blank_to_end` do not error (test 1541–1557).
   - If last item is `Blank` with `BlankWidth::ToEndOfLine` (`sealed`, 861–866): set `Error::RunAfterFillBlank` if `error` is still `None`, return. Non-empty `text`/`bold` take this path.
   - If last item is `InlineItem::Text` and `last.face == face`: `last.text.push_str(text)` and return. No other equality. URI is not part of the predicate because the field does not exist.
   - Else `items.push(InlineItem::Text(TextRun::new(text, face)))`.
5. `blank` / `blank_em` / `blank_to_end` → `push_blank` (845–859). If `sealed`, note `RunAfterFillBlank` and return a `Blank` with `index == usize::MAX`. Else push `InlineItem::Blank(InlineBlank::new(width))` with default stroke, return `Blank { index: len-1 }`. Any blank, including `Fixed(0)`, is an item. It is not skipped. A following `text`/`bold` with the same `Face` does **not** merge across it (test `blank_breaks_text_coalescing`, 1515–1537: `"A"` + blank + `"B"` stays two `Face::Body` runs).
6. End of `document()` closure: `paint` (350–396) lowers each section: `FlowNode::Stack(flow.into_nodes()?)` plus decoration hooks (142–168). Pagination later `split`s the stack. `TextNode::split` (116–118) does not re-merge items; it calls `layout_runs` → `prepare_inline` (371–393).
7. `prepare_inline` walks `InlineItem`s. Consecutive `Text` runs (any mix of `Face`) are buffered and flushed as **one** `measure_inline` group. A `Blank` flushes the buffer, then pushes `Span::Blank { advance, payload: BlankDraw }` (`src/text/line_break.rs` 18–21). `BlankDraw` (paragraph.rs 360–364) is thickness/color/offset only. End of items flushes remaining text.
8. `measure_inline` (491–533) calls `cluster_tagged(runs.map(|run| (run.face, run.text)))`. `cluster_tagged` (`src/text/cluster.rs` 131–157) requires `T: Copy`, tags **each scalar** with `T`, and drops hard-break scalars from `glyph_tags` while keeping them in the cluster tape. Here `T = Face`. The resulting `TaggedClusteredText<Face>` is the only per-glyph attribute retained. Glyph runs are then re-cut by `FontId` (`glyph_runs_for_line`, 547–600), not by original `TextRun` boundaries. `GlyphRun` (`src/canvas.rs` 119–132) has font, size, color, origin, glyphs, text. No URI.
9. `place_line` (426–478) emits `LinePiece::Glyphs(GlyphRun)` and `LinePiece::Stroke`. `fragment_from_lines` (308–343) turns those into `FragmentCommand::Glyphs` and `Stroke`. A wrapped link would have to be invented here or in `draw_fragment`; nothing in this chain reads a URI.
10. Header/footer, same `TextNode` tail, different head: page hook → `Band::lower` → `TextNode::new(slot_string)` → `measure_band` → `Stack.split` → step 7. The string is not passed through `Paragraph::append`, so steps 4–5 do not run. `TextNode::new("")` yields empty `items` (still a node if the slot is `Some`). One non-empty slot is a single `Face::Body` run.
11. Table cells join at step 2's private `Flow::text` during `Table::lower`, still inside the build closure (assemble time), not at page measure. Same `TextNode::new` shape as a header slot: one body run, no `Paragraph` merge rules.
12. `Layer::text_at` and list markers skip steps 2–9 and call `single_line_run`.

**Where a non-empty external URI can attach, given this shape.**

- Assemble-time carrier that already means “this slice of text”: `TextRun`, beside `face`. `Style` is the whole paragraph (one link would cover every run, including runs the caller did not mark, and blanks are separate items). `InlineBlank` is a stroke, not text; this knife is text URI only. `GlyphRun` is post-break and is rebuilt by font, so it is too late to be the only store unless something earlier copied the URI onto each piece.
- DSL write site: `Paragraph::append`, which is the only function that creates `TextRun`s for `text`/`bold`. A public method has to pass a `LinkTarget` into `append` (or push a `TextRun` the same way). `LinkTarget::uri` already rejects `""` with `EmptyLink` before a run exists. The merge at 836–839 must treat URI as part of the key; today it concatenates whenever `Face` matches, which would glue different URIs into one string and drop the boundary line breaking needs.
- Survival past wrap: a field on `TextRun` is necessary and not sufficient. `prepare_inline` concatenates every consecutive `Text` item into one group, and `cluster_tagged` keeps only a `Copy` tag (`Face`). `LinkTarget` is not `Copy`. Face changes do **not** flush the group; only `Blank` does. After `break_spans`, `Seg::Glyphs` is `(group, start, end)` (`src/text/line_break.rs` 24–29) with no URI payload. `Span`'s payload type is used for blanks (`BlankDraw`), not for text groups (`Span::Group(usize)`). So either (a) a URI change flushes `pending` the way a blank does, and the group carries one `LinkTarget`, and `place_line` emits a rect per `Seg::Glyphs` of that group (a wrap then yields one rect per line from the same group URI), or (b) the per-scalar tag becomes something `Copy` that indexes a URI table. Path (a) matches the existing flush. Neither path exists today.
- Header/footer “same pipeline”: they share steps 7–9 (`TextNode` → `prepare_inline` → `FragmentCommand::Glyphs`). They do not share `Paragraph`. A URI stored only by a new `Paragraph` method does not reach `Band`. `Band` would have to store a `LinkTarget` next to `left`/`right` and `Band::lower` would have to build a `TextRun` that includes it (`TextNode::new` hardcodes `Face::Body` and has nowhere to put a URI). Slot text is chosen at page-hook time, so the URI has to be stored on `Band` until `lower`.
- Not this pipeline, so a `TextRun` URI does not cover them: `Layer::text_at`, list marker labels, images (`ImageNode` / `ImageBox` / `Cell::image` / `Layer::image_at`). Table cell strings do use `TextNode::new` but not `Paragraph::append`; a paragraph-only API misses `Cell::new("…")` unless `Flow::text` / `TextNode::new` also takes a URI.
- Paint: `draw_fragment` has no `annotate_link` arm. `Canvas::annotate_link` and `LinkTarget` are ready and unused by layout. No in-document destination type exists; `LinkTarget` is an external URI string only.

### Files Read

- `docs/提案书.md` §8 (613–619), §2.0 mention at 213, confirm table row 646, pdfmake `link` row 158
- `docs/quire-design.md` 988–1023 and 1310–1316 (unconfirmed sketch only; not implemented)
- `src/lib.rs` 1–37
- `src/document.rs` 1–60, 142–169, 198–259, 571–690, 778–912, 974–1032, 1108–1260, 1285–1292, 1406–1420, 1463–1652
- `src/layout/paragraph.rs` 1–200, 252–281, 308–343, 345–600
- `src/layout/style.rs` 1–50
- `src/layout/flow.rs` 46–96, 140–170, 347–376
- `src/layout/decorate.rs` 36–81, 248–275
- `src/layout/lower.rs` 11–67 (band allows `Text`, rejects `List` / `PageBreak`)
- `src/layout/list.rs` 113–119, 207–212
- `src/layout/image.rs` 9–27
- `src/layout/mod.rs` 1–16
- `src/text/cluster.rs` 120–157
- `src/text/line_break.rs` 12–34
- `src/canvas.rs` 30–51, 114–210
- `src/error.rs` 31–32, 80–81, 128–134
- `src/fonts.rs` 130–136
- Grep across `src/**/*.rs` for `LinkTarget`, `annotate_link`, `TextRun`, `InlineItem`, `EmptyLink`, `.style =`

### Boundaries

| Boundary | What crosses | What does not |
| --- | --- | --- |
| Public crate API (`src/lib.rs`) | `Paragraph`, `Blank`, `Flow::paragraph`, `Band::{left,right}`, `Cell::new(String)`, `Layer::text_at`, `LinkTarget::uri`, `Error::EmptyLink` | `TextRun`, `Face`, `InlineItem`, `TextNode`, `Style`, `FlowNode` (`pub(crate)`) |
| `Paragraph::append` → `TextNode` | `Vec<InlineItem>` plus one `Style` clone (`from_items`) | Per-run color/size; any URI (field absent) |
| `TextNode` → line break | `prepare_inline` groups + `Face` scalar tags + `BlankDraw` payloads | Original `TextRun` boundaries once faces share a group; URI |
| Fragment → canvas | `FragmentCommand::{Glyphs,Fill,Stroke,Image}` via `draw_fragment` | `annotate_link` (API exists, no caller in `document` or `layout`) |
| Header/footer vs body assembler | Same `TextNode::split` after `Band::lower` | `Paragraph` merge, `bold`, `blank`, assemble-time items. Hook runs at page measure, not inside `document()`'s build closure |
| Layer text / list markers | `single_line_run` → one `GlyphRun` | `InlineItem`, wrap, `Face::Bold` |
| Table cell vs `paragraph()` | `TextNode::new` during `Table::lower` (assemble time) | `InlineItem` list, `bold`, `blank` |
| Image | `ImageNode` / `draw_image` | URI field (out of this knife; also absent) |

`document.rs` imports run types at line 27: `BlankWidth`, `Face`, `InlineBlank`, `InlineItem`, `TextNode`, `TextRun`. It imports `Style` at line 28. It does not import `LinkTarget`. Tests in `document.rs` assert `TextNode` equality, so they see crate-private fields (1464–1501, 1515–1557).

### Non-Obvious Things

- Empty string is ignored only in `Paragraph::append`, and it returns before `sealed()`. A paragraph closure that only calls `text("")` still pushes a `TextNode` with `items: []` (678–689 does not drop empty item lists). `TextNode::new("")` also stores empty `items`, but `Band::left("")` still creates a column slot because `filter_map` only drops `None` (1221–1223). `Cell::new("")` likewise becomes `TextNode::new("")`.
- Whitespace-only text is not empty. `"A"` + `"B"` becomes `"AB"` (`push_str`). No trim, no inserted separator.
- Merge key is `Face` only, and only when the previous item is `Text`. A blank of any width splits runs even if both sides are `Face::Body`. Different faces never merge. `from_runs` / `from_items` do not merge; only `append` does.
- Second coalesce is in layout, not the DSL: `prepare_inline` joins consecutive `Text` runs of **different** faces into one `measure_inline` / `cluster_tagged` group. Bold vs body is a per-scalar `Face` tag, then glyph draws split again on `FontId`. A URI that is not part of that tag or of a flush boundary is discarded before `break_spans`.
- `cluster_tagged::<T>` needs `T: Copy` (cluster.rs 131). `Face` is `Copy`. `LinkTarget` is not. A URI cannot be dropped in as the existing tag type without changing that bound or using a `Copy` index.
- `blank_to_end` seals the item list. Later non-empty `text`/`bold` and any further `blank` / `blank_em` / `blank_to_end` set `RunAfterFillBlank` and the whole paragraph is discarded (no node). Later `text("")` / `bold("")` do not. First error sticks (`note_run_after_fill`, 868–871). Stroke setters on a rejected blank do nothing.
- `bold` does not select a font at assemble time and does not change `Style`. Missing `bold_font` is `UnknownFont { id: 1 }` when the bold run is measured. Empty bold text never asks for the bold face (`measure_inline` 493–496 skips empty bold runs).
- `Style` is frozen at `default_style()` for the DSL. The only mutations of a style value on the way into a `TextNode` are `first_line_indent` (`Flow::paragraph`) and `align` (`Band::lower` Start vs End). Header/footer therefore share body font id, 12 pt, 1.2, black, if they share this pipeline. They do not inherit a paragraph's indent.
- `LinkTarget::uri(" ")` succeeds (canvas test 505). Only `is_empty()` fails. No trim. That matches §8 “do not validate scheme”; the type does not know about http.
- `FlowNode::Paragraph` vs the public type `Paragraph`: the public builder lowers to `FlowNode::Text(TextNode)` **before** wrapping. The name `ParagraphNode` is the post-break line list (`src/layout/paragraph.rs` 123–128).
- Design-doc `Span::Link` and image `link` are not in the tree. Do not treat them as the current AST.

### Open Questions

- Public shape is unspecified by code and by §8: a method on `Paragraph` such as `link(LinkTarget, impl FnOnce(&mut …))`, or `text_uri(s, LinkTarget)`, or a builder that returns a handle the way `Blank` does. §8 says the URI is on text and empty is rejected at construct. It does not name the method. `LinkTarget` is already the construct-time rejection.
- Whether header/footer “同管线” means “whatever `TextNode` carries, `Band::lower` must set it” or “`Band` grows a `Paragraph`”. Today they share `TextNode::split` and do not share `Paragraph`. `left`/`right` are plain strings chosen in the per-page hook.
- Table cells are body text via `TextNode::new`, not via `Paragraph`. §8 says 正文. The cell DSL cannot express `bold` or a sub-span either. Whether v1 links are paragraph-only or also `Cell::new` is not stated in §8.
- `Layer::text_at` is page-fixed text, not the flow pipeline. §8 names 正文 and 页眉页脚, not behind/front. No URI field there.
- Empty **text** with a valid URI: `append` would drop the string and emit no run, so no rect. §8 does not say whether that is success or an error. Empty **URI** is already `EmptyLink` if the caller goes through `LinkTarget::uri`.
- A link that wraps: §8 says one rect per line for the glyphs that belong to that URI. `place_line` has per-seg x advances and a line box, but no line-box height on `LinePiece` (height is `metrics.line_height` on `ParagraphNode`, paragraph.rs 179–180). Rect geometry is a layout concern; the DSL does not store boxes.
- `prepare_inline` groups mixed faces. If two adjacent runs share a URI and differ in `Face`, one group can still carry one URI **if** flush-on-URI-change is the rule. If the implementation instead requires `cluster_tagged` to tag URIs per scalar, the `T: Copy` bound has to change or the tag has to be an index. That choice is not in the code.
- In-doc jumps and image links: no types. `LinkTarget` is a single URI string. Adding an enum of destinations would be outside §8's “外链 URI” sentence.
