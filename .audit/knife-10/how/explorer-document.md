# Explorer: public DSL document assembly (knife 10)

Scope: public DSL in `src/document.rs`, re-exports in `src/lib.rs`, the paint → paginate → decorate path those methods call, and `tests/document.rs` pins for header inset and section counters. Crate package name is `quire-pdf` (`Cargo.toml:2`). Rust path prefix is `quire_pdf`.

No `section`, `PageSize`, `Orientation`, `A3`, or `A5` identifier exists anywhere under `src/`.

## Components Found

### Entry and re-exports

- `pub fn document<'a>(page_size: Size, build: impl FnOnce(&mut Document<'a>) -> Result<()>) -> Result<Rendered>` at `src/document.rs:39-53`.
- Module comment (`src/document.rs:1-6`): `document` runs the caller closure **once** to assemble the body flow. Header, footer, behind, and front closures run on every page **after** pagination, so only they see page numbers.
- Function comment (`src/document.rs:31-38`): one document of `page_size` pages. Builder mistakes are deferred and returned from `document` before any page is drawn. A missing glyph anywhere, including headers and layers, fails the whole document. Text uses the first font passed to `Document::font` at 12 pt, line height 1.2, black, start-aligned. Margins default to zero.
- `src/lib.rs:8` `pub mod document`. `src/lib.rs:21-24` re-exports: `document`, `Band`, `Cell`, `Columns`, `Document`, `Flow`, `Image`, `ImageBox`, `Layer`, `List`, `Numbering`, `Page`, `Paragraph`, `Rendered`, `Table`.
- Geometry re-exports used as the paper/margin surface (`src/lib.rs:12-15`): `Length`, `Margins`, `NonNegative`, `Point`, `Positive`, `Rect`, `Size`, `A4`, `LETTER`, `MM_TO_PT`. There is no `PageSize` type.
- `PageInfo`, `Decorations`, `BandContent`, `LayerContent`, `PaginateConfig` are `pub(crate)` and are **not** re-exported.

### `Document` fields

`src/document.rs:58-71`. All fields private. Constructed only by private `Document::new` (`src/document.rs:74-89`), which `document()` calls. There is no public page-size setter and no `section` method.

| Field | Type | Initial value |
| --- | --- | --- |
| `page_size` | `Size` | argument to `document` / `new` |
| `doc` | `DocToken` | `DocToken::next()` (`src/document.rs:193-197`), process-wide `AtomicU32` |
| `fonts` | `FontStore` | empty |
| `margins` | `Margins` | `Margins::default()` = four zeros (`src/geom.rs:359-362`) |
| `body` | `Flow` | `Flow::new(default_style(), doc)` |
| `header` / `footer` | `Option<BandHook<'a>>` | `None` |
| `behind` / `front` | `Option<LayerHook<'a>>` | `None` |
| `images` | `ImageTable` | only with `feature = "backend-krilla"` |

Hook type aliases (`src/document.rs:55-56`):

- `BandHook<'a> = Box<dyn Fn(&mut Band) + 'a>`
- `LayerHook<'a> = Box<dyn Fn(&mut Layer) + 'a>`

`Fn`, not `FnMut`. A later call replaces the previous hook (`Option` assignment), it does not compose.

### Document methods that are not body flow

- `load_image(&mut self, bytes: Vec<u8>, format: ImageFormat) -> Result<Image>` (`src/document.rs:94-110`). With `backend-krilla`, decodes immediately via `ImageTable::register` and stamps `DocToken`. Without the feature, always `Error::Image { message: "decoding needs feature `backend-krilla`" }`.
- `font(&mut self, bytes: Vec<u8>) -> Result<()>` (`src/document.rs:113-120`). Registers `FontSource { bytes: Arc, index: 0 }` and drops the returned `FontId`. Store assigns ids by `faces.len()` before push, so the first face is 0 (`src/fonts.rs:75-76`, `src/fonts.rs:108-111`). DSL text style is hard-wired to `FontId::from_raw(0)` (`src/document.rs:289-297`). Extra `font()` calls register more faces; the public DSL never selects them.
- `margins(&mut self, margins: Margins)` (`src/document.rs:122-125`). Replaces the single document-wide value. Comment: top and bottom are also the header and footer band heights.
- `header` / `footer` / `behind` / `front` (`src/document.rs:127-145`). Each stores one closure. Comments say a later call replaces an earlier one, and that header/footer/behind/front run on every page.

`Document` derefs to `Flow` (`src/document.rs:275-287`), so body methods are called on `&mut Document`.

### Body `Flow` (single implicit stack)

`src/document.rs:300-441`. Private fields: `style`, `doc`, `nodes: Vec<FlowNode>`, `error: Option<Error>`.

`push` (`src/document.rs:322-330`) keeps the first `Err` and ignores later nodes. `into_nodes` (`src/document.rs:338-343`) returns that error or the vec. `document()` only calls `into_nodes` inside `paint`, after the build closure has returned `Ok(())`. Closure-returned errors (for example `font()?`, `NonNegative::pt(...)?`) abort before paint. Builder errors that go through `push` (bad column count, foreign image, …) surface from `paint`.

Public body methods:

- `image`, `paragraph`, `columns` (≥2, equal `Ratio(1)`, gap `default_column_gap()` = 12 pt, `src/layout/columns.rs:91-93`), `table` (equal columns, `repeat_header` default `true`), `keep_together`, `spacer`, `page_break`, `ul`, `ol(Numbering, …)`.
- `page_break` (`src/document.rs:404-407`) pushes `FlowNode::PageBreak`. Comment: “Only allowed in the body.” The method is still on `Flow`, so a nested column/list closure can call it. Rejection is not in the DSL; `check_content` marks `PageBreak` illegal in every nested container (`src/layout/lower.rs:55`). Integration pin: `tests/document.rs:377-393` → `Error::InvalidCellContent { container: "column", node: "page break" }`.

`paint` lowers the body with `FlowNode::Stack(body.into_nodes()?)` (`src/document.rs:162`). One stack for the whole document. No section node.

`default_style` (`src/document.rs:289-298`): font id 0, 12 pt, line-height factor 1.2, `Color::rgb(0,0,0)`, paragraph spacing 0, `TextAlign::Start`. Nothing in the public DSL mutates `Flow.style`. `paint` clones this style once and passes the clone into every band/layer hook (`src/document.rs:161-167`).

### `Document::paint` → `Decorations` + `PaginateConfig`

`fn paint(self, canvas: &mut dyn Canvas) -> Result<Painted>` (`src/document.rs:147-177`). Consumes the document.

```text
decorations = Decorations {
    header: header.map(|hook| band_content(hook, style.clone())),
    footer: footer.map(|hook| band_content(hook, style.clone())),
    behind: behind.map(|hook| layer_content(hook, style.clone(), doc)),
    front:  front.map(|hook| layer_content(hook, style.clone(), doc)),
}
config = PaginateConfig { page_size, margins }   // one size, one margins
paginate_root(FlowNode::Stack(...), &mut LayoutCtx { fonts }, config, &decorations, canvas)
```

`Painted` (`src/document.rs:180-184`) returns the font store and, with the feature, the image table. `document()` wraps that in `Rendered` plus the `RecordingCanvas` it created (`src/document.rs:45-52`).

`Rendered` (`src/document.rs:858-883`): `pages()` copies `RecordingCanvas::pages()`. `to_pdf(self)` exists only under `backend-krilla`: replay the recording into `KrillaCanvas` and `finish()`. Default feature set includes `backend-krilla` (`Cargo.toml:14`).

`document()` never calls `Canvas::set_document_info`. `DocumentInfo` (`src/canvas.rs:17-28`) is unused by this assembly path.

### Public `Page` and `From<&PageInfo>`

`Page` (`src/document.rs:671-709`):

- Doc comment: “The document is one implicit section, so section counters equal document totals.”
- `#[non_exhaustive]`, `Copy`.
- `number: u32` — one-based page number in the document.
- `count: u32` — pages in the document, “blank ones included.”
- `section_number: u32` — documented as one-based page number in the section.
- `section_count: u32` — documented as pages in the section.
- `size: Size` — “Paper size.”
- `margins: Margins` — “The four margins of this page.”
- `is_last(&self) -> bool` is `self.number == self.count` (document last page, not a separate section-last flag).

`PageInfo` (`src/layout/decorate.rs:36-43`) has only:

- `number: PageNumber` (`NonZeroU32` newtype, `src/layout/decorate.rs:16-24`)
- `count: PageCount` (`src/layout/decorate.rs:26-34`; comment: “Total pages in the document, blank body pages included.”)
- `size: Size`
- `margins: Margins`

No section fields on `PageInfo`.

`From<&PageInfo> for Page` (`src/document.rs:699-709`) copies:

- `number` ← `info.number.get()`
- `count` ← `info.count.get()`
- `section_number` ← `info.number.get()` (same value)
- `section_count` ← `info.count.get()` (same value)
- `size` ← `info.size`
- `margins` ← `info.margins`

This `From` impl is the only assignment to `section_number` / `section_count` in the repo.

### `Band` / `Layer` and the hook wrappers

`Band` (`src/document.rs:712-777`):

- Holds `page: Page`, `left: Option<String>`, `right: Option<String>`. No center slot. No paragraph/image/columns API.
- `page()` returns the `Page` copy.
- `left` / `right` store owned strings.
- `lower` builds a `ColumnsNode` with gap `NonNegative::ZERO`:
  - Each present slot is `ColumnWidth::Ratio(Ratio(1))` plus one `FlowNode::Text`. Left uses `TextAlign::Start`, right uses `TextAlign::End`. Other style fields come from the cloned document default.
  - If both slots are `None`, returns an empty vec (band draws nothing).
  - If `margins.left()` or `margins.right()` is non-zero, a fixed-width **empty** column of that many points is inserted on that side (`src/document.rs:756-768`).
  - A single remaining track is returned as bare text nodes (no columns wrapper).
- Doc comment (`src/document.rs:712-716`): band is as tall as its margin; slots inset by left and right margins so they line up with the body; content that does not fit is an error.

`band_content` (`src/document.rs:779-789`) returns `BandContent` = `Box<dyn Fn(&PageInfo) -> Result<Vec<FlowNode>> + 'a>` (`src/layout/decorate.rs:45`). Each call builds a fresh `Band { page: Page::from(info), left: None, right: None }`, runs the user hook, then `lower`.

`Layer` (`src/document.rs:791-839`):

- Doc comment: coordinates are page coordinates, not inset by margins.
- `page()`, `is_last()` (delegates to `Page::is_last`), `text_at(s, at: Point)`, `image_at(image, rect: Rect)`.
- `text_at` pushes `LayerItem::Text(LayerText { text, at, style: self.style.clone() })`. Comment: one line; top-left of the line box at `at`; not wrapped or clipped.
- `image_at` checks `image.doc == self.doc`. Mismatch stores `Error::UnknownImage` if no earlier layer error, and does not push. Comment (`src/document.rs:822-824`): rectangle is not clipped and is not inset by margins. `LayerItem::Image` comment in decorate (`src/layout/decorate.rs:59-63`): not checked against the page.
- No path primitives (`line` / `rect` / `ellipse`) on `Layer`.

`layer_content` (`src/document.rs:841-856`) returns `LayerContent` = `Box<dyn Fn(&PageInfo) -> Result<Vec<LayerItem>> + 'a>` (`src/layout/decorate.rs:66`). Fresh `Layer` per call; hook error or `Ok(items)`.

`Decorations` (`src/layout/decorate.rs:68-80`): four `Option`s, `none()` is all `None`. One set for the whole document.

### Paper size types that exist today

`Size` (`src/geom.rs:212-249`): both sides `Length<Positive>`. Constructors: `new`, `try_pt`, `try_mm`. Accessors `width()` / `height()`.

Constants (`src/geom.rs:251-255`):

- `A4`: `210 * MM_TO_PT` by `297 * MM_TO_PT`.
- `LETTER`: 612 pt by 792 pt.

`Margins` (`src/geom.rs:305-357`): four `Length<NonNegative>`. `new(top, right, bottom, left)`, `uniform`, side accessors. `Default` is uniform zero.

`document` takes a finished `Size`. Orientation is not a separate value; a landscape page would be a `Size` whose width and height are already swapped by the caller. Nothing in `Document` can override `page_size` per range of pages.

### Pagination types the DSL fills

`PaginateConfig` (`src/layout/paginate.rs:11-39`):

- `page_size: Size`
- `margins: Margins`
- `body_width` = page width − left − right
- `body_height` = page height − top − bottom
- `body_origin` = `(left, top)`
- `validate`: body width and height must be finite and `> 0`, else `Error::InvalidGeometry`

`PagePlan` (`src/layout/paginate.rs:48-52`): `{ size: Size, placed: Vec<PlacedFragment> }`. No margins field, no section id. Every plan is built with `config.page_size` (`src/layout/paginate.rs:155-161`, `177-179`, `201-203`).

`paginate_root` (`src/layout/paginate.rs:54-68`):

1. `plan_pages` — body only. Comment: bands are measured after the body plan, so decorations can never move body content.
2. Collect `sizes: Vec<Size>` from plans.
3. `measure_bands(&sizes, config.margins, decorations, ctx)` — **one** `Margins` for every page.
4. `paint_pages`.

`measure_bands` (`src/layout/decorate.rs:91-137`):

- `count = NonZeroU32::new(sizes.len() as u32)` or `Error::InvalidGeometry` (zero pages cannot occur given `finalize_eof`, but an empty slice would fail here).
- For `(number, size)` in `1..` zipped with `sizes`, builds `PageInfo { number, count, size: *size, margins }` using the **same** margins for every index.
- Header band origin y = 0, height limit = `margins.top()`.
- Footer band origin y = `page_height - margins.bottom()`, height limit = `margins.bottom()`.
- Then measures behind and front for that same `PageInfo`.

Per-page hook call order inside `measure_bands`: header, footer, behind, front. This is measurement order, not paint order.

`measure_band` (`src/layout/decorate.rs:180-207`): empty nodes → `None`. Otherwise `check_content(Container::Band, …)` then `FlowNode::Stack(nodes).split` against a frame whose width is the **full page width** and whose height is the **full page height**. Fit with `height <= margin` is kept. Taller than the margin → `Error::DecorationTallerThanMargin { height, margin }`. Partial / move-to-next / unsplittable → same error with `margin` replaced by the page height in the `taller(page_h)` arm. Wider than the frame → `Error::DecorationWiderThanFrame`.

`Container::Band` (`src/layout/lower.rs:44-55`) allows text, spacer, and columns (and recurses into column stacks). It rejects tables, lists, images, page breaks, and keep-together. The public `Band` API cannot emit those nodes; the check is on the lowered `FlowNode`s.

`measure_layer` (`src/layout/decorate.rs:139-178`): each `LayerText` is `single_line_run(..., f32::INFINITY)` with align forced to `Start`, then `run.origin` is shifted by `item.at`. Images become `FragmentCommand::Image` with the given rect. Empty item list → `None`. Fragment height is 0. Origin of the placed fragment is `(0, 0)`.

`paint_pages` (`src/layout/paginate.rs:216-229`), for each plan zipped with its `PageBands`:

1. `canvas.start_page(page.size)`
2. Draw, in order: `behind`, `header`, `page.placed` (body), `footer`, `front`
3. `canvas.finish_page()`

`Canvas::start_page(&mut self, size: Size)` (`src/canvas.rs:187-188`). `RecordingCanvas` logs `CanvasCommand::StartPage(size)` (`src/canvas.rs:312-315`). `RecordedPage::size()` is that size (`src/canvas.rs:216-224`). `Rendered::pages()` is this scan (`src/canvas.rs:261-296`).

Body fragments are placed at `(margins.left, margins.top + y)` (`src/layout/paginate.rs:107-110`). Header/footer fragments are placed by `measure_bands` at y = 0 and y = page_h − bottom, x = 0. Side inset is inside the band’s column tracks, not by translating the fragment.

### Page-break cursor (what “count” includes)

`plan_pages` (`src/layout/paginate.rs:70-153`) plus cursor helpers:

- Leading `PageBreak` while `Fresh`: stays `Fresh` (`src/layout/paginate.rs:171`). Does not add a page.
- `Fresh` at EOF with no pages yet: one empty `PagePlan` (`src/layout/paginate.rs:199-204`). An empty body is one blank page.
- Content then one `PageBreak`: commits the content page, sets `PendingBlank`; EOF `PendingBlank` drops `current` and does **not** push an extra page (`src/layout/paginate.rs:172-174`, `210-212`). Trailing single break is not an extra page.
- Content then two `PageBreak`s: the second pushes an empty `PagePlan` (`src/layout/paginate.rs:176-181`). That blank page is in `sizes`, so it increments `Page.count` / `section_count`.
- Unit pins: `src/layout/paginate.rs:347-469` (`leading_page_break_is_noop_one_blank_page`, `content_then_one_page_break_drops_pending`, `content_then_two_page_breaks_yields_blank_body_page`, `content_break_break_content_is_three_pages`).

Decorations are measured for every plan, including blank-body pages, because `measure_bands` iterates `sizes` with no “has body” filter.

### Integration tests that pin the sugar

`tests/document.rs`.

`header_slots_inset_by_side_margins_and_body_sits_below_the_band` (`tests/document.rs:187-197`), via `full_template` (`tests/document.rs:134-163`): `Margins::uniform(36 pt)`, A4, header left `"ACME"` and right page `number`/`count` string.

- Left glyph x ≈ 36 (left margin), y ≈ fixture baseline 13.812 (`BASELINE`, `tests/document.rs:16`).
- Right run’s x + width ≈ `A4.width − 36`.
- Body x ≈ 36, body y ≈ `36 + BASELINE` (top margin, not under a content-sized header).

`footer_sees_section_counters_equal_to_document_totals` (`tests/document.rs:222-241`):

- Uniform 20 pt margins, footer `format!("{}/{}", page.section_number, page.section_count)`.
- Body `"A"`, `page_break`, `"B"`.
- Page 0 texts `["A", "1/2"]`, page 1 texts `["B", "2/2"]`.
- Footer glyph y ≈ `A4.height − 20 + BASELINE` (footer band top = page height − bottom margin; baseline sits that far below the band top).

Related pins in the same file, not section-specific but part of this assembly:

- `header_numbers_every_page_and_layers_stack_behind_header_body_footer_front` (`tests/document.rs:166-184`): two pages; paint order of glyph texts is `DRAFT` (behind), header left, header right (`第 1 页 / 共 2 页` then `第 2 页 / 共 2 页`), body, and `SEAL` only on the last page (`front` + `is_last`).
- `a_later_header_call_replaces_the_earlier_one` (`tests/document.rs:244-256`): second `header` wins; only `"new"` plus body.
- `header_with_zero_margin_is_rejected` (`tests/document.rs:259-271`): `DecorationTallerThanMargin { height ≈ LINE 17.244, margin: 0.0 }`.
- `text_at_places_the_top_left_of_the_line_box` (`tests/document.rs:211-219`): glyph y = requested y + `BASELINE` (line-box top, not baseline, is what the caller passes).

Unit tests in `src/document.rs:916-962` repeat “missing glyph in header/front” and “header without margin” and assert the recording stays empty. That matches `paginate_root`: the canvas is only written in `paint_pages`, which runs after `measure_bands` succeeds.

## Flow

1. Caller calls `document(size, |doc| { …; Ok(()) })` (`src/document.rs:39-44`).
2. `Document::new(size)` stores that one `Size`, zero margins, empty font store, empty body stack, no hooks (`src/document.rs:74-89`).
3. Build closure runs **once**. `font` / `margins` / `load_image` mutate document fields immediately. `paragraph` / `table` / `columns` / `page_break` / … push `FlowNode`s (or the first error) into `body.nodes`. `header` / `footer` / `behind` / `front` store closures and do **not** run them. Page numbers do not exist yet. An `if` in the build closure selects which body nodes are pushed (`docs/提案书.md:40` matches this; the code has no `If` node).
4. If the closure returns `Err`, `document` returns that error and does not paint (`src/document.rs:44`).
5. `paint` (`src/document.rs:147-171`):
   - Snapshot `body.style` (always the 12 pt default unless something outside the public API mutated it).
   - Lower body to `FlowNode::Stack`. First deferred builder error returns here, canvas still empty.
   - Wrap the four hooks into `Decorations`.
   - Build `PaginateConfig { page_size, margins }`.
6. `plan_pages` (`src/layout/paginate.rs:70-153`) walks the stack. Frame is body width/height from that single config. Page breaks and overflow produce `PagePlan`s that all carry `config.page_size`. Empty input yields one blank plan. This step does not call the hooks and does not touch the canvas.
7. `measure_bands` (`src/layout/decorate.rs:91-137`) sets `count` to the number of plans (blanks included). For each index `n` from 1:
   - `PageInfo` gets document `number=n`, document `count`, that plan’s `size` (today always the document size), and the document `margins`.
   - Header/footer hooks run via `band_content` → `Page::from` (section counters aliased to document totals) → `Band::lower`.
   - Behind/front hooks run via `layer_content`.
   - Any error (missing glyph, band taller than margin, band wider than page, unknown image, …) returns before `paint_pages`. `document()` drops the recording; direct `paint` tests see an empty command list.
8. `paint_pages` (`src/layout/paginate.rs:216-229`) opens each page at `PagePlan.size` and draws behind → header → body → footer → front. Hooks are **not** called again here; paint uses fragments already measured in step 7.
9. `Rendered` holds the recording. Callers read `pages()` or, with `backend-krilla`, `to_pdf()`.

Single-section sugar, as implemented: the build closure is the only body, `Document` holds one size and one margins and one of each hook, and `Page::from` copies document number/count into the section fields. There is no second section to restart counters, change paper, or swap hooks.

## Files Read

- `src/document.rs` (full)
- `src/lib.rs` (full)
- `src/layout/decorate.rs` lines 1–207 (types, `measure_bands`, `measure_layer`, `measure_band`); lines 220–238 (internal test helper `numbered` formats `PageInfo` number/count only)
- `src/layout/paginate.rs` lines 1–230 (config, plan, paint); lines 347–469 (page-break cursor tests)
- `src/layout/lower.rs` lines 1–79 (`Container::Band`, `PageBreak` rejected)
- `src/layout/columns.rs` lines 91–93 (`default_column_gap` = 12 pt)
- `src/geom.rs` lines 209–363 (`Size`, `A4`, `LETTER`, `Margins`)
- `src/canvas.rs` lines 1–40 (`DocumentInfo`), 182–320 (`Canvas::start_page`, `RecordedPage`, `RecordingCanvas::pages`)
- `src/fonts.rs` lines 70–111 (first `FontId` is 0)
- `tests/document.rs` lines 1–271 (template, header inset, section-counter footer, hook replace, zero-margin header); remainder scanned for other `section_` uses — only the one test
- `Cargo.toml` lines 1–15 (package name, default feature)
- `docs/提案书.md` lines 502–535 and 596–608 (confirmed section/paper direction the current sugar only partly covers; not executed code)

Grep of `src/**/*.rs` for `section`, `PageSize`, `Orientation`, `A3`, `A5`: no matches. The only `section_number` / `section_count` writes are `src/document.rs:704-705`. The only production read of those fields by a caller is the test at `tests/document.rs:228`.

## Boundaries

In scope of this slice and present:

- One `document(Size, …)` call, one body stack, one `PaginateConfig`, four optional document-level hooks.
- Public page facts are `Page`. Internal facts are `PageInfo`. The alias from document counters to section counters is entirely in `From<&PageInfo>` (`src/document.rs:699-709`).
- Paper at the DSL boundary is `geom::Size` plus `A4` / `LETTER`. Per-page `Size` already flows `PagePlan.size` → `measure_bands`’s `&[Size]` → `PageInfo.size` → `Page.size` → `Canvas::start_page`. The planner never puts two different sizes in that slice.
- Margins are one `Margins` on `Document`, copied into `PaginateConfig` and then passed as a single argument to `measure_bands`. `Page.margins` repeats that value on every page. Band side inset and footer y both read it at measure time.
- `is_last` compares document `number` and `count` (`src/document.rs:694-696`). With one section that is also the section’s last page. There is no section-last method.

Out of this slice (other explorers / not in the public DSL):

- Paragraph breaking, table header repeat, column split, image layout math, krilla emit. This path only calls `paginate_root` and `FlowNode::split` inside band measurement.
- `docs/提案书.md` §2.11 / §7 specify multi-section, per-section paper override, an orientation enum, a paper-size enum (A-series + `Custom`), and per-section margin/header/footer. None of those types or methods exist in `src/`. The implemented subset is the sentence at §2.11 “单 section 文档：`document` 闭包可糖成隐式唯一 section” plus `Page` fields that display that sugar.
- §2.10 text says header/footer span the full page width and the caller indents using `margin.left` / `margin.right`. The DSL instead auto-inserts empty fixed columns (`src/document.rs:756-768`). `tests/document.rs:187-197` locks the auto-inset. Knife 10 changing `Page.margins` per page would change that inset, because `Band::lower` reads `self.page.margins`.
- §2.12 path drawing on behind/front is not on `Layer`.
- Metadata (`DocumentInfo`) is not set by `document()`.

Seams knife 10 has to extend if it keeps this pipeline:

- `PageInfo` has nowhere to put a section index or a section length. Until it does, `Page::from` can only alias document totals.
- `PaginateConfig` and `Document` store one `page_size` and one `margins`. `PagePlan` can already record a different `size` per physical page, and `measure_bands` already reads `sizes[i]`, but every current writer uses `config.page_size`. Margins are not per page: `measure_bands`’s signature takes one `Margins` (`src/layout/decorate.rs:91-94`).
- Hooks live on `Document`, not on a section value. `paint` builds one `Decorations` (`src/document.rs:163-168`). `measure_bands` calls that same `BandContent` / `LayerContent` for every page index.
- `is_last` and `Layer::is_last` are document-scoped. A non-final section’s last physical page would still see `is_last() == false`.
- Blank plans count toward both `count` and, via the alias, `section_count`. A future per-section count has to use the same page list (including blank-body plans) or the footer test’s “blank ones included” wording on `Page.count` (`src/document.rs:679`) will diverge from section totals.
- New section start is not representable. `FlowNode` has `PageBreak` but no section break. `page_break` does not change `page_size`, margins, or hooks, and a single trailing break does not even allocate a page.

## Non-Obvious Things

- Section counters are not computed. They are a second copy of `PageInfo.number` / `PageInfo.count` (`src/document.rs:704-705`). `PageInfo` is filled from the index in the planned-page vector (`src/layout/decorate.rs:103-109`), which includes blank-body pages.
- Hooks run during `measure_bands`, before any `start_page`, once per page. They do not run during the build closure and they do not run again in `paint_pages`. Side effects in a header closure happen for page 1 before page 2 is measured, and if page 2’s hook returns `Err`, page 1’s commands are still absent from the canvas.
- Measurement call order per page is header, footer, behind, front (`src/layout/decorate.rs:127-134`). Paint order is behind, header, body, footer, front (`src/layout/paginate.rs:219-223`). The integration test asserts paint order of glyph text, not hook call order.
- Hook type is `Fn`. The closure cannot mutably accumulate across pages without interior mutability. Each invocation gets a new `Band` or `Layer`; slot strings do not carry over.
- Replacing a hook drops the previous closure. `tests/document.rs:244-256`: `left("old")` then `right("new")` yields only `"new"`.
- Band slots are equal-ratio columns of the area between the side insets, gap 0, not two independently positioned full-width strings. With both left and right set, each gets half of `(page_width - left_margin - right_margin)`. End alignment puts the right string’s end at `page_width - right_margin` only because that edge is the end of the right half. A single slot (left or right alone) gets the full content width, so a lone `right("new")` still ends at the right margin.
- Zero side margin omits the inset track (`src/document.rs:757-758`). Zero top margin does not omit a non-empty header; it fails `DecorationTallerThanMargin` because the line box is taller than margin 0 (`tests/document.rs:259-271`, `src/document.rs:951-962`). A header hook that sets neither slot lowers to an empty vec, and `measure_band` returns `None` (`src/layout/decorate.rs:186-188`) — that case is not what the zero-margin test covers.
- Band split frame width is the full page width (`src/layout/decorate.rs:191-195`), while the visible text is inset by empty columns. Overflow is “wider than the page,” not “wider than the body.”
- Body frame is fixed from margins before hooks run (`src/layout/paginate.rs:54-56`, `81-83`). A longer page-number string cannot change where the body breaks.
- `Page.size` is whatever `PagePlan.size` was. Today that is always `Document.page_size`. `tests/document.rs` never reads `page.size()`; it compares against the `A4` constant the test passed in.
- First registered font is the only one the DSL uses, because style font id is the constant 0, not “whatever `font()` returned.” `font()` discards the id (`src/document.rs:114-119`).
- `DocToken` binds `Image` to one `Document` (`src/document.rs:186-206`, check in `ImageBox::lower` at `src/document.rs:266-269` and `Layer::image_at` at `src/document.rs:826-832`). Foreign handles fail at lower/measure time with `Error::UnknownImage`.
- `document` returns `Rendered`, not PDF bytes. PDF is a second step, `Rendered::to_pdf`, and only with `backend-krilla`.
- `Page` is `#[non_exhaustive]` (`src/document.rs:675`). Adding fields later is source-compatible for struct literals outside this crate; it does not by itself create a second section.

## Open Questions

- `Page::is_last` is defined as document-last (`src/document.rs:692-696`). 提案书 §2.12 says closures may use `is_last` but does not say whether that becomes section-last once multiple sections exist. The code has only the document comparison.
- 提案书 §2.11 / §7 require a paper enum plus orientation, with `Custom` already final. The code’s public paper argument is a raw `Size`, and `A4` / `LETTER` are constants, not enum variants. No in-tree type records “A4 landscape” separately from a swapped `Size`. Whether knife 10 adds that enum or keeps `Size` is not decided in `src/`.
- `measure_bands` can already see a different `Size` per page, but it cannot see different margins: one `Margins` argument (`src/layout/decorate.rs:93`). Per-section margins need a signature change; per-section paper can be represented by varying `PagePlan.size` without changing `measure_bands`’s size slice, as long as every plan’s size is written by the planner instead of `config.page_size`.
- No test asserts `page.section_number` on a blank inserted page (double `page_break`). The alias would number that blank, because `count` is `sizes.len()` and numbering is the index. `footer_sees_section_counters_equal_to_document_totals` only covers content, one break, content (two non-blank pages).
- No test reads `Page.size` or `Page.margins` from inside a hook. Those fields are filled (`src/document.rs:706-707`) but only the counter alias and the geometric consequences (inset x, footer y, body y) are pinned in `tests/document.rs`.
