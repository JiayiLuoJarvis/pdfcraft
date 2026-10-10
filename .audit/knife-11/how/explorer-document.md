# Explorer: document DSL / Layer API / section sugar (knife 11)

Repo tip: `feat/layer-path` @ `9eda8da` (`feat(document): paginate multiple sections with per-section paper and dual page counters`).

Scope: public `doc.behind` / `doc.front` / `section.behind` / `section.front`, `Layer` (`text_at`, `image_at`), `LayerHook`, `layer_content`, how those hooks become `Decorations`, and the style / `DocToken` rules those paths use. Measure and paint are cited only where the DSL hands off. Path/stroke/fill/rotation/opacity facts are what the public caller can already construct, and which types the Layer API does not accept.

Product text that is **not** implemented on `Layer`: `docs/提案书.md` §2.12 (lines 527–543). Confirmed primitives still missing from the DSL: `line` / `rect` / `ellipse`, stroke/fill, rotation, and color opacity as a layer argument. `text_at` and `image_at` are the only draw methods.

## Components Found

### Entry and re-exports

- `pub fn document<'a>(page_size: PageSize, build: impl FnOnce(&mut Document<'a>) -> Result<()>) -> Result<Rendered>` — `src/document.rs:39-53`.
- Module comment `src/document.rs:1-6`: the build closure runs **once**. Header, footer, behind, and front closures run again on every page **after** pagination, so only they see page numbers.
- Function comment `src/document.rs:31-38`: builder mistakes are returned from `document` before any page is drawn. A missing glyph anywhere, including headers and layers, fails the whole document. Text uses the first font from `Document::font` at 12 pt, line height 1.2, black, start-aligned. Margins default to zero.
- `src/lib.rs:21-24` re-exports `document`, `Band`, `Cell`, `Columns`, `Document`, `Flow`, `Image`, `ImageBox`, `Layer`, `List`, `Numbering`, `Page`, `Paragraph`, `Rendered`, `Section`, `Table`.
- `src/lib.rs:12-15` re-exports geometry the caller can name without going through `quire_pdf::geom`: `Color`, `Finite`, `Length`, `Margins`, `NonNegative`, `Orientation`, `PageSize`, `Paint`, `Paper`, `Path`, `PathBuilder`, `PathVerb`, `Point`, `Positive`, `Rect`, `Size`, `Stroke`, `A4`, `LETTER`, `MM_TO_PT`.
- Not re-exported (crate-private or private): `LayerHook`, `layer_content`, `LayerItem`, `LayerText`, `LayerContent`, `Decorations`, `PageInfo`, `DocToken`, `Style`.

### Hook type

`src/document.rs:55-56`:

- `BandHook<'a> = Box<dyn Fn(&mut Band) + 'a>`
- `LayerHook<'a> = Box<dyn Fn(&mut Layer) + 'a>`

`Fn`, not `FnMut` and not `FnOnce`. The same closure is invoked once per physical page. A later `behind` / `front` call replaces the previous hook (`Option` assignment). Hooks do not compose.

### Where hooks are stored

`Section` (`src/document.rs:68-77`) owns the real slots:

| Field | Type | Initial (`Section::new`, lines 80–89) |
| --- | --- | --- |
| `page_size` | `Option<PageSize>` | `None` (document default used at lower) |
| `margins` | `Margins` | `Margins::default()` (four zeros) |
| `flow` | `Flow` | `Flow::new(style, doc)` |
| `header` / `footer` | `Option<BandHook<'a>>` | `None` |
| `behind` / `front` | `Option<LayerHook<'a>>` | `None` |

`Document` (`src/document.rs:183-193`) does **not** store hooks itself. It stores `page_size`, `doc: DocToken`, `fonts`, `body: Sections<'a>`, a deferred `error`, and (feature `backend-krilla`) `images`.

`Sections` (`src/document.rs:58-66`):

- `Implicit(Section<'a>)` — the lead section created in `Document::new` (`src/document.rs:195-206`) with `default_style()` and a fresh `DocToken`.
- `Explicit { sections: Vec<Section<'a>>, closed: Flow }` — after the first successful `Document::section`. `closed` is `Flow::rejecting` so later body pushes become `Error::ContentOutsideSection`.

### Document sugar vs section methods

Both pairs assign `Some(Box::new(f))`. Comments on all four say a later call replaces an earlier one.

| Method | Lines | Behavior |
| --- | --- | --- |
| `Section::behind` | `src/document.rs:128-130` | Always stores on this section. |
| `Section::front` | `src/document.rs:132-137` | Always stores on this section. |
| `Document::behind` | `src/document.rs:267-274` | Stores only when `body` is `Sections::Implicit`. Otherwise `fail_outside()`. |
| `Document::front` | `src/document.rs:276-283` | Same as `Document::behind`. |

`Document::margins` / `header` / `footer` (`src/document.rs:241-265`) use the same implicit-only gate.

`fail_outside` (`src/document.rs:323-327`) sets `self.error = Some(Error::ContentOutsideSection)` if it is still `None`. The `behind` / `front` / `margins` / `header` / `footer` methods return `()`. They do not return `Result`. The stored error is returned from `Document::paint` (`src/document.rs:339-340`), which `document()` calls after the build closure returns `Ok`.

`Section::written` (`src/document.rs:92-99`) is true when the flow has nodes or a flow error, **or** margins are not default, **or** any of header/footer/behind/front is `Some`. An empty body with only `behind` still counts as written.

`Document::section` (`src/document.rs:291-321`):

1. If `self.error` is already set, return that error.
2. If the implicit section `written()` (including a behind/front hook), store and return `Error::ContentOutsideSection` immediately. The `section` closure does not run.
3. Clone `flow.style` from the implicit section, or from `closed` once explicit.
4. `Section::new(style, self.doc)` — same `DocToken` as the document, not a new one.
5. Run `f(&mut section)`. On error, store it and return it. The section is not pushed.
6. First success replaces `Implicit` with `Explicit { sections: vec![section], closed: Flow::rejecting(style, doc) }`. Later successes push onto `sections`.

`Section` derefs to `Flow` (`src/document.rs:169-181`), so `section.paragraph` / `page_break` / `image` write the section body. `Layer` does not deref to `Flow`. A behind/front closure cannot call `paragraph`, `table`, `columns`, and so on.

`Document` also derefs to `Flow` (`src/document.rs:473-491`): implicit → that section's flow; explicit → the rejecting `closed` flow. Body methods after `section()` therefore hit `Flow::push`'s `reject_pushes` path (`src/document.rs:539-545`) and set `ContentOutsideSection` on that flow. `paint` calls `closed.into_nodes()?` (`src/document.rs:344-346`), which surfaces it.

### Lowering hooks into `Decorations`

`Section::lower` (`src/document.rs:139-166`), called from `Document::paint` (`src/document.rs:352-353`) **after** the build closure, once per section:

```text
style = flow.style.clone()
config = PaginateConfig { page_size: page_size.unwrap_or(default_page).resolve(), margins }
root = FlowNode::Stack(flow.into_nodes()?)
Decorations {
  header: header.map(|hook| band_content(hook, style.clone())),
  footer: footer.map(|hook| band_content(hook, style.clone())),
  behind: behind.map(|hook| layer_content(hook, style.clone(), doc)),
  front:  front.map(|hook| layer_content(hook, style, doc)),
}
```

`band_content` does not receive `DocToken`. `layer_content` does, because `image_at` checks image ownership.

`paint` zips each `(PaginateConfig, FlowNode, Decorations)` into a `SectionJob` (`src/document.rs:358-367`) and calls `paginate_sections` (`src/layout/paginate.rs:93-118`). Body pages are planned first. `measure_bands` runs next. `paint_pages` runs only if measure succeeds. A layer error therefore happens before `canvas.start_page`.

`Decorations` (`src/layout/decorate.rs:70-76`):

- `header` / `footer`: `Option<BandContent<'a>>` where `BandContent<'a> = Box<dyn Fn(&PageInfo) -> Result<Vec<FlowNode>> + 'a>` (`src/layout/decorate.rs:47`).
- `behind` / `front`: `Option<LayerContent<'a>>` where `LayerContent<'a> = Box<dyn Fn(&PageInfo) -> Result<Vec<LayerItem>> + 'a>` (`src/layout/decorate.rs:68`).

`Decorations::none()` (`src/layout/decorate.rs:78-82`) is `Default`: all four `None`.

### `layer_content`

`src/document.rs:1063-1078`. The returned closure, invoked later with `&PageInfo`:

1. Builds a fresh `Layer` with `page: Page::from(info)`, the captured `style` clone, the captured `doc` token, empty `items`, `error: None`.
2. Calls `hook(&mut layer)` — the caller's `Fn`.
3. If `layer.error` is `Some`, returns that `Err` and **drops `items`**. Otherwise returns `Ok(layer.items)`.

The hook is not called at `behind()` / `front()` time. It is called from `measure_layer` (`src/layout/decorate.rs:180-189`) once per physical page, after pagination, when page numbers exist.

### `Layer` public API

`pub struct Layer` (`src/document.rs:1013-1020`). Comment: coordinates are page coordinates, not inset by margins. Fields are private: `page`, `style`, `doc`, `items: Vec<LayerItem>`, `error`.

| Method | Lines | Signature / effect |
| --- | --- | --- |
| `page` | 1025–1027 | `fn page(&self) -> Page`. Copy of the page facts. |
| `is_last` | 1031–1033 | `self.page.is_last()`. |
| `text_at` | 1036–1042 | `fn text_at(&mut self, s: impl AsRef<str>, at: Point)`. Pushes `LayerItem::Text(LayerText { text: owned String, at, style: self.style.clone() })`. Doc: one line; top-left of the line box at `at`; not wrapped; not clipped. Does not set `error`. Does not look up glyphs. |
| `image_at` | 1047–1060 | `fn image_at(&mut self, image: &Image, rect: Rect)`. If `image.doc != self.doc`, set `Error::UnknownImage { id }` only when `self.error` is still `None`, and return **without** pushing. A matching image is pushed even if `error` is already set. Doc: stretched into `rect`; not clipped; not inset by margins. |

No other methods. No `line`, `rect`, `ellipse`, `fill`, `stroke`, `rotate`, `opacity`, `color`, or style setter.

`LayerItem` (`src/layout/decorate.rs:57-66`), `pub(crate)`:

- `Text(LayerText)` — `LayerText` (`src/layout/decorate.rs:49-55`): `text: String`, `at: Point`, `style: Style`. Comment: unwrapped line; `at` is the line-box top-left; alignment is ignored.
- `Image { image: DecodedImage, rect: Rect }` — comment: page-coordinate rect; not inset by margins; not checked against the page.

Order in the `Vec` is paint order inside that layer (`LayerItem` comment, line 57).

### `Page` facts the closure can read

`Page` (`src/document.rs:892-919`), `#[non_exhaustive]`, `Copy`:

| Field | Meaning |
| --- | --- |
| `number` / `count` | One-based document page and document page count, blank body pages included. |
| `section_number` / `section_count` | One-based index and length of this section's page span. |
| `size` | This physical page's `Size` (section override or document default, already resolved). |
| `margins` | This section's four margins. |

`Page::is_last` (`src/document.rs:916-918`) is `number == count`. That is the **document** last page, not the section last page. There is no `section_is_last` method. A caller can compare `section_number == section_count`.

`From<&PageInfo> for Page` (`src/document.rs:921-932`) copies `NonZero` counters via `.get()` plus `size` and `margins`.

`PageInfo` (`src/layout/decorate.rs:36-45`) is filled in `measure_bands` (`src/layout/decorate.rs:125-132`): document `number`/`count` from the flat page index; `section_number`/`section_count` from the span offset and `span.len`; `size` from the planned page; `margins` from `span.margins` (the section's `PaginateConfig.margins`).

`Layer::is_last` is the same predicate the proposal example uses (`docs/提案书.md:112-116`, `page.is_last()`). Integration test `tests/document.rs:156-159` draws `"SEAL"` only when `page.is_last()`.

### Style rules (there is no public style API)

`default_style` (`src/document.rs:493-502`):

| Field | Value |
| --- | --- |
| `font` | `FontId::from_raw(0)` — the first font registered with `Document::font` (`src/document.rs:230-237`) |
| `font_size` | `Positive::pt(12.0)` |
| `line_height_factor` | `1.2` |
| `color` | `Color::rgb(0, 0, 0)` → `rgba(0, 0, 0, 255)` (`src/geom.rs:485-487`) |
| `paragraph_spacing` | `NonNegative::ZERO` |
| `align` | `TextAlign::Start` |

`Style` is `pub(crate)` (`src/layout/style.rs:16-25`). Fields: `font`, `font_size`, `line_height_factor`, `color: Color`, `paragraph_spacing`, `align`. No opacity field separate from `Color`. No bold/italic/decoration.

Nothing in `src/document.rs` assigns `flow.style` after `Flow::new` / `Section::new`. `Paragraph` (`src/document.rs:664-675`) only has `text`. `Flow::paragraph` (`src/document.rs:580-585`) wraps that string in `TextNode::new(text, self.style.clone())`. There is no caller-facing color, font size, alignment, or opacity setter on `Document`, `Flow`, `Paragraph`, `Section`, or `Layer`.

Style snapshots:

- Implicit section: `default_style()` at `Document::new`.
- Each explicit section: clone of the previous flow's style at the `section()` call (`src/document.rs:301-305`), which is still `default_style` because nothing mutates it.
- `layer_content` captures another clone at `lower()` / paint time (`src/document.rs:153-163`), not at `behind()` registration time.
- `text_at` clones `Layer.style` into each `LayerText` (`src/document.rs:1037-1041`).

`measure_layer` (`src/layout/decorate.rs:191-201`) then **overwrites** `align` to `TextAlign::Start` before `single_line_run`, even if the stored style were `Center` or `End`. Decorate test `layer_ignores_alignment_and_page_bounds` (`src/layout/decorate.rs:594-618`) pins that. `paragraph_spacing` is not read for layers: the resulting fragment has `height: 0.0` and `spacing_after: 0.0` (`src/layout/decorate.rs:211-217`). `line_height_factor` does affect the baseline via `single_line_run` → `line_metrics`.

### `DocToken` rules

`DocToken(u32)` (`src/document.rs:384-396`) is a process-wide `AtomicU32`, one per `Document::new`. Comment (`src/document.rs:384-387`): image ids are dense indexes; a handle moved into another document would paint that document's image at the same index.

`Image` (`src/document.rs:398-418`) is `Copy` and holds `decoded: DecodedImage` plus `doc: DocToken`. `Document::load_image` (`src/document.rs:209-228`) stamps `self.doc`. Without `backend-krilla`, every `load_image` returns `Error::Image { message: "decoding needs feature \`backend-krilla\`" }`.

`Layer` stores the section's `DocToken` (the document's token). `image_at` rejects a different token with `Error::UnknownImage { id: image.decoded.id.as_u32() }` (`src/error.rs:116-118`, display `"image {id} is not registered"`). Flow images use the same check in `ImageBox::lower` (`src/document.rs:463-468`), but that error is stored on the flow at **build** time. Layer's check runs when the hook runs, at **measure** time.

`text_at` does not consult `DocToken`.

### What measure does with the items (handoff, not a full layout trace)

`measure_layer` (`src/layout/decorate.rs:180-219`):

- `None` content → `Ok(None)`.
- For each item, in `Vec` order:
  - `Text`: `single_line_run(fonts, text, &style_with_start_align, f32::INFINITY)`, then `run.origin = at + run.origin`. Push `FragmentCommand::Glyphs`.
  - `Image`: push `FragmentCommand::Image { image, rect }` unchanged.
- Empty `commands` → `Ok(None)` (hook ran, drew nothing, no fragment).
- Otherwise one `PlacedFragment` at `Point::try_pt(0.0, 0.0)` with `Fragment { height: 0.0, spacing_after: 0.0, commands }`.

`single_line_run` (`src/layout/paragraph.rs:141-169`) measures the whole string as one run. Width is checked against `max_width`. The layer passes `f32::INFINITY`, so `UnbreakableWiderThanFrame` does not fire for a finite advance. The function then forces `run.origin` to `(0, metrics.baseline)` (line 168), discarding `align_x`. `measure_layer` adds `at` on top (lines 197–200). Glyph baseline is therefore `(at.x, at.y + baseline)`. Integration pin: `tests/document.rs:211-218` (`DRAFT` at x=180, y=400+baseline; `SEAL` at x=430, y=680+baseline).

Missing glyphs fail inside `single_line_run` / `measure_text` during `measure_bands`, before `paint_pages`. `src/document.rs:1158-1173` (`missing_glyph_in_front_fails_before_any_page`) and `src/layout/decorate.rs:622-641` assert an empty canvas. The front hook in the document test only calls `text_at` when `page.is_last()`; measure still runs the hook on every page, and the missing scalar is on the last page, so the whole document fails.

`paint_pages` (`src/layout/paginate.rs:266-278`) draws, per page:

`bands.behind` → `bands.header` → `page.placed` (body) → `bands.footer` → `bands.front`

That matches §2.12 (`docs/提案书.md:533-535`). Integration pin: `tests/document.rs:166-183` text order on page 0 is `DRAFT`, header slots, body; page 1 adds `SEAL` last. Decorate pin: `src/layout/decorate.rs:565-590`.

`draw_fragment` (`src/layout/flow.rs:338-364`) adds the fragment origin to glyph origins, stroke paths (`Path::translated`), and image rects. Layer origin is `(0, 0)`, so layer command coordinates are already page coordinates when they hit the canvas.

### Public color / paint / stroke the caller can construct today

These types are public and re-exported. **No `Layer` method takes them.**

`Color` (`src/geom.rs:473-499`):

- `rgb(r, g, b)` → alpha 255.
- `rgba(r, g, b, a)` — alpha is a `u8` channel. This is the only opacity carrier in geom.
- `channels() -> (u8, u8, u8, u8)`.
- Construction is total. No `Result`.

`Paint` (`src/geom.rs:501-518`): comment "A solid fill." `solid(color)` and `color()`. No gradient, no separate opacity field. Opacity is `color`'s alpha.

`Stroke` (`src/geom.rs:520-537`): `new(width: Length<Positive>)` and `width()`. Width is strictly positive because `Positive::pt(0.0)` is `Error::NonPositiveLength` (`src/geom.rs:739-744`, test `stroke_rejects_zero_through_positive` at `src/geom.rs:797-805`). No cap, join, miter, or dash fields. No opacity field of its own.

`Path` / `PathBuilder` / `PathVerb` (`src/geom.rs:539-627`):

- Verbs: `MoveTo(Point)`, `LineTo(Point)`, `Close`. No curve, no ellipse, no arc.
- `Path::begin(at)`, `Path::line(from, to)`, `verbs()`.
- Builder: `line_to`, `move_to`, `finish` (no `Close`), `close` (appends `Close` then finishes).
- `Path::translated` is `pub(crate)` (`src/geom.rs:583`).
- A path is non-empty and starts with `MoveTo` by construction. There is no public `Path` constructor from a `Rect`.

`Point` (`src/geom.rs:174+`): `new(Length<Finite>, Length<Finite>)`, `try_pt`, `try_mm`. `Rect` (`src/geom.rs:365+`): `new(origin, size)`, `try_pt(x, y, width, height)`, `try_mm`. Width and height must be `Positive` (non-empty). `text_at` takes `Point`. `image_at` takes `Rect`.

`Canvas` already accepts paint (`src/canvas.rs:193-197`):

- `fill_path(&mut self, path: &Path, paint: &Paint)`
- `stroke_path(&mut self, path: &Path, paint: &Paint, stroke: &Stroke)`

`CanvasCommand::FillPath` and `StrokePath` (`src/canvas.rs:145-160`) record them. `RecordingCanvas` implements both (`src/canvas.rs:322-337`). Krilla marks `Mark::Fill` / `Mark::Stroke` (`src/backend/krilla_canvas.rs:252-263`). Emit (`src/backend/emit.rs:117-129`) sets one pen and clears the other, then `draw_path`.

Alpha → krilla opacity is already in the backend, not in the DSL (`src/backend/convert.rs:52-72`):

- `to_fill`: `opacity = NormalizedF32::new(a as f32 / 255.0)`, `FillRule::NonZero`, RGB from the color.
- `to_stroke`: same opacity, `width: stroke.width().as_pt()`, other stroke fields `..KStroke::default()` (comment: cap, join, miter, dash stay at krilla defaults).
- Glyph runs go through the same `to_fill(Paint::solid(*color))` (`src/backend/emit.rs:161-162`). `GlyphRun.color` is `Color` (`src/canvas.rs:124-125`). Layer text is opaque black because `Style.color` is `Color::rgb(0, 0, 0)`, not because glyphs drop alpha.

`FragmentCommand` (`src/layout/flow.rs:77-89`) is `Glyphs`, `Stroke { path, paint, stroke }`, `Image`. There is **no** `Fill` variant. `Fragment::with_offset` (`src/layout/flow.rs:62-74`) and `draw_fragment` translate `Stroke` paths and images. Table cell borders are the only producer of `FragmentCommand::Stroke` (`src/layout/table.rs:236-237`, `260-266`). They build a closed polyline with a private `rect_path` (`src/layout/table.rs:393-399`): `MoveTo` origin, three `LineTo`, `Close`. That helper is not public and is not used by `Layer`. Table paint is `Paint::solid(grid.border.color)`.

Rotation: no geom type, no `Layer` method, no `Canvas` transform method. The only `push_transform` in backend emit is `Transform::from_translate` for image placement (`src/backend/emit.rs:172-180`). Ellipse: not a `PathVerb` and not a `Layer` method.

## Flow

```text
document(page_size, build)
  Document::new
    DocToken::next()
    Sections::Implicit(Section::new(default_style(), doc))
      behind: None, front: None
  build(&mut doc)                         // once; hooks are stored, not run
    doc.behind(f)  / doc.front(f)         // only if still Implicit; else fail_outside()
    doc.section(|s| { s.behind/front })   // per-section Option; fails if implicit already written
  Document::paint
    if self.error { return Err }          // deferred ContentOutsideSection
    for section in sections:
      Section::lower
        layer_content(hook, flow.style.clone(), doc) -> LayerContent
        -> Decorations { behind, front, header, footer }
    paginate_sections(jobs)
      plan body pages first               // hooks still not called
      measure_bands
        per physical page:
          PageInfo { doc number/count, section number/count, size, margins }
          measure_layer(behind) and measure_layer(front)
            hook(&mut Layer)               // caller's Fn, once per page per slot
            LayerItem::Text | Image -> FragmentCommand::Glyphs | Image
            one PlacedFragment at (0, 0), or None if no commands
      paint_pages
        behind, header, body, footer, front
        draw_fragment adds origin (0, 0) then Canvas::draw_glyph_run / draw_image
```

Call-time vs measure-time errors:

| Event | When it fails |
| --- | --- |
| `doc.behind` after `section()` | `error` set immediately; `document()` returns it from `paint` if `build` returns `Ok`. |
| `section()` after implicit `behind`/`front`/margins/header/footer/body | `section()` returns `Err(ContentOutsideSection)` immediately. |
| `image_at` with another document's `Image` | `Layer.error` set inside the hook; `layer_content` returns `Err` during `measure_bands`. |
| Missing glyph in `text_at` | `single_line_run` during `measure_bands`. Canvas untouched. |
| Hook pushes nothing | `Ok(None)`. That layer is skipped. The other layer and the body still paint. |
| Hook sets `error` after also pushing items | `layer_content` returns `Err` and discards the vec (`src/document.rs:1073-1076`). |

Within one hook, items paint in push order as commands of a single fragment. `behind` and `front` are separate fragments. They are not interleaved with each other.

## Files Read

- `src/document.rs` — `document`, `Sections`, `Section`, `Document` sugar, `section()`, `paint`, `DocToken`, `Image`, `default_style`, `Flow` (style is never mutated), `Page`, `Layer`, `layer_content`, unit tests for missing front glyph and content-outside-section.
- `src/lib.rs` — public re-exports (`Layer`, `Section`, `Color`, `Paint`, `Path`, `Stroke`, …).
- `src/layout/decorate.rs` — `PageInfo`, `LayerText`, `LayerItem`, `LayerContent`, `Decorations`, `measure_bands`, `measure_layer`, layer tests (stack order, alignment ignored, missing glyph).
- `src/layout/paginate.rs` — `SectionJob`, `SectionSpan`, `paginate_sections` (plan, then measure, then paint), `paint_pages` stack order. Lines 1–118 and 266–278.
- `src/layout/flow.rs` — `FragmentCommand` (no `Fill`), `with_offset`, `draw_fragment`.
- `src/layout/paragraph.rs` — `single_line_run` origin reset to `(0, baseline)`; `glyph_run_for_line` copies `style.color`.
- `src/layout/style.rs` — `Style` fields.
- `src/layout/table.rs` — only the existing stroke producer (`Paint::solid`, `FragmentCommand::Stroke`, private `rect_path`). Lines 236–266 and 393–399.
- `src/geom.rs` — `Color`, `Paint`, `Stroke`, `Path`, `PathVerb`, `PathBuilder`, `Rect` (public constructors and what they do not contain).
- `src/canvas.rs` — `GlyphRun.color`, `Canvas::fill_path` / `stroke_path`, `CanvasCommand::FillPath` / `StrokePath`.
- `src/backend/convert.rs` — `to_fill` / `to_stroke` alpha → krilla opacity. Lines 52–72.
- `src/backend/emit.rs` — fill/stroke pen clearing; glyph `to_fill(Paint::solid(*color))`; image `from_translate` only. Lines 115–180.
- `src/error.rs` — `UnknownImage`, `ContentOutsideSection`, `MissingGlyph`.
- `tests/document.rs` — `full_template` behind/front (`155-163`), stack-order test (`166-183`), `text_at` line-box pin (`211-218`), last-page `image_at` (`498-531`). No test calls `section.behind` or `section.front`.
- `docs/提案书.md` §1 example (`108-120`) and §2.12 (`527-543`); §2.14 (`556-562`) says flow canvas is out of v1 and escape drawing is behind/front path primitives.

## Boundaries

- This note stops at the `LayerItem` → `measure_layer` match and the `FragmentCommand` / `Canvas` types a new path item would have to join. It does not trace line breaking, image decoding, or krilla path conversion beyond the opacity and pen facts above.
- `LayerItem` is `pub(crate)`. Callers cannot construct it. New path primitives have to be methods on `Layer` that push new variants (or reuse `Text` / `Image`, which cannot carry a path).
- `measure_layer` is a closed `match` on `Text` | `Image` (`src/layout/decorate.rs:190-206`). A new variant does not compile until that match, and until `Fragment` can hold the command.
- `FragmentCommand::Stroke` exists and `draw_fragment` already calls `canvas.stroke_path` after translating by fragment origin. Layer fragments use origin `(0, 0)`, so a stroke stored in page coordinates would pass through with a zero translation. `FragmentCommand` has no fill arm, and `draw_fragment` has no `fill_path` call. A filled layer path cannot be represented by today's enum.
- Table `rect_path` is private to `src/layout/table.rs`. It is not a public rect primitive and it is not on `Layer`.
- `Canvas::fill_path` / `stroke_path` and krilla emit already accept `Path` + `Paint` + `Stroke`, including alpha. That sink is below layout. The DSL does not call it.
- No rotation API exists at geom, `Layer`, `Canvas`, or `FragmentCommand`. Image translation in emit is not a caller-facing rotate.
- No ellipse verb exists. A public `Path` can only be polylines plus `Close`.
- Style is not a document token and not a caller parameter. `DocToken` is only an image-ownership id. Do not treat them as the same mechanism.
- §2.12 also says coordinates are not auto-inset by margins, and the caller uses `page.size` / `page.margins` to align with the body. `Layer` exposes those fields and does not apply them. Header/footer bands **do** inset (`Band::lower`, `src/document.rs:962-998`). Layers do not go through `Band`.
- Flow canvas (§2.14) is a different API and is not present. `Layer` is not a flow node and does not affect body height (`Fragment.height == 0`, placed at `(0, 0)`, measured after the body plan — `paginate_sections` comment at `src/layout/paginate.rs:73`).

## Non-Obvious Things

- `Document::behind` / `front` return `()`. After the first `section()`, they set a deferred error instead of returning `Err`. `document()` still fails, but only when `paint` runs, and only if `build` itself returns `Ok`. Contrast `Document::section`, which returns `Err` immediately when the implicit section is already written — and a behind hook alone makes it written (`src/document.rs:92-99`, `295-299`).
- Hooks are `Fn`, so they cannot mutably capture across pages unless the capture uses interior mutability. They must be callable once per page. `Image` is `Copy`, which is why `tests/document.rs:513` can `move` the seal into `doc.front`.
- One slot per section. The second `behind` replaces the first. There is no list of hooks. `behind` and `front` are independent.
- The hook runs at measure time, not at registration and not at paint time. Page numbers are final. `is_last` is document-scoped. A multi-section document's earlier section never sees `is_last() == true` on its own last page unless that page is also the document's last page.
- `layer_content` clones style at `lower()` (paint setup), from `flow.style`. Registration order relative to `font()` does not change the style: the font id is always raw `0`, and color stays opaque black. Registering a font does not count as writing the document (`Document::section` doc comment, `src/document.rs:285-290`).
- `image_at` on a foreign `Image` does not panic and does not push. The first such error sticks. Later valid `image_at` calls still push, but `layer_content` returns the error and the vec is discarded, so the whole document fails inside `measure_bands`.
- `text_at` never sets `Layer.error`. Glyph failure is a different error (`MissingGlyph`) from the measure match arm, not from the hook's `Result`.
- An empty successful hook (no `text_at` / `image_at`, or `is_last` false so the body of the `if` does not push) becomes `Ok(None)`, not an empty painted fragment. `tests/document.rs:166-183`: `"DRAFT"` is on both pages; `"SEAL"` is only on page 2.
- Layer text is not wrapped, not clipped, and not rejected for being outside the page. `max_width` is `f32::INFINITY`. Decorate test places a run at `(190, 395)` on a 200×400 page (`src/layout/decorate.rs:594-618`).
- `single_line_run` resets the glyph origin to `(0, baseline)` after alignment math. `measure_layer` then adds `at`. The public contract "top-left of the line box" is `at.y + baseline` on the canvas, not `at.y` as the baseline. Pinned by `tests/document.rs:211-218`.
- All of a layer's items share one fragment. `draw_fragment` walks commands in order. A future path command in that vec would paint among the text and images of the same layer, in push order, still under header (if behind) or over footer (if front).
- `Color::rgba` and `Paint::solid` are callable by any user of the crate today, and the backend maps alpha to krilla fill/stroke opacity. The layer DSL never reads a caller-supplied color. Layer glyphs use `Style.color`, which is opaque black.
- `Stroke` cannot represent "no stroke" with width 0. Zero is `NonPositiveLength` at the `Positive` constructor. Table code skips `stroke_path` by not pushing a command when border width is not positive (`Positive::pt(border).ok().map(Stroke::new)`, `src/layout/table.rs:236`).
- Proposal sample (`docs/提案书.md:110-114`) writes `Point::new(180.0, 400.0)` and `Rect::new(430.0, 680.0, 90.0, 90.0)`. Actual constructors are `Point::new(Length<Finite>, Length<Finite>)` or `Point::try_pt`, and `Rect::new(Point, Size)` or `Rect::try_pt`. Tests use `try_pt`.

## Open Questions

- §2.12 names `line` / `rect` / `ellipse` plus stroke/fill, rotation, and半透明, but it does not fix method signatures, whether stroke and fill are separate calls or flags, whether rotation is per item or a layer transform, or whether opacity is `Color::rgba` only. The code has no partial implementation to infer a signature from.
- Whether a layer path should be a new `LayerItem` that lowers to `FragmentCommand::Stroke` (exists) and a new fill command (does not exist), or whether `measure_layer` should call `Canvas` directly. Today every layer item becomes a `FragmentCommand` inside one zero-height fragment at `(0, 0)`.
- Whether `rect` on the layer should accept the public `Rect` (axis-aligned, positive extent) or a path built like table `rect_path`. Ellipse cannot be expressed with current `PathVerb`s.
- Whether layer text should gain a color/opacity argument. Today it cannot. `GlyphRun.color` already carries alpha into `to_fill`. Adding a path opacity channel does not require a new color type if `Color::rgba` / `Paint::solid` is the argument.
- No test covers `section.behind` / `section.front`, replacing a layer hook, `image_at` with a foreign `DocToken`, or `doc.behind` after `section()`. The code paths are the ones cited above; the pins that exist are implicit-section `doc.behind` / `doc.front` in `tests/document.rs`.
