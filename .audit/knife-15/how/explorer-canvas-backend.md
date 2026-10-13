# Knife 15 — Canvas `LinkTarget` / `annotate_link` / krilla link annotations

Tree: `feat/inline-blank` @ `a232ce1`. Facts from the working tree. Canvas and `backend-krilla` already accept a non-empty URI and write a PDF link annotation. Layout never calls that path.

## Components Found

### `LinkTarget` (`src/canvas.rs:30-51`)

```30:51:src/canvas.rs
/// A non-empty URI for a hyperlink annotation.
pub struct LinkTarget(String);

impl LinkTarget {
    pub fn uri(uri: &str) -> Result<Self> {
        if uri.is_empty() {
            return Err(Error::EmptyLink);
        }
        Ok(Self(uri.to_owned()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}
```

- Newtype over a private `String`. The only constructor is `LinkTarget::uri`.
- Empty string (`uri.is_empty()`, not trim) is `Err(Error::EmptyLink)`.
- A single space is kept: `LinkTarget::uri(" ")` succeeds and `as_str()` is `" "` (`src/canvas.rs:505-506`).
- No scheme parse, no whitespace strip, no network. The string is stored as given.
- Public via `src/lib.rs:26-28` (`LinkTarget` is in the canvas re-export).

### `Error::EmptyLink` (`src/error.rs:31-32`, display `src/error.rs:134`)

Unit variant. Display text is `"link URI is empty"`. `Error` itself is not `Copy` (later variants hold `String`). Nothing in `document.rs` or `layout/` constructs `EmptyLink`. The only production constructor is `LinkTarget::uri`.

### `Canvas::annotate_link` (`src/canvas.rs:207-210`)

```207:210:src/canvas.rs
    /// Records a hyperlink over `rect`.
    ///
    /// `target` is already a non-empty URI.
    fn annotate_link(&mut self, rect: Rect, target: LinkTarget) -> Result<()>;
```

The trait does not re-check the string. Callers that only have a `LinkTarget` already passed the empty check. `Rect` is never empty: extent is `Size`, width and height must be positive (`src/geom.rs:365-385`).

`CanvasCommand::AnnotateLink { rect, target }` is `src/canvas.rs:173-179`.

### `RecordingCanvas` (`src/canvas.rs:353-357`, replay `375-377`, `pages` `255-286`)

`annotate_link` pushes `AnnotateLink` and returns `Ok`. It does not require an open page and does not look at the URI.

`pages()` copies `AnnotateLink` into the closed page along with fill, stroke, image, and glyph commands (`src/canvas.rs:227-229`, `282-286`). `SetDocumentInfo` is skipped. An unclosed `StartPage` is omitted.

`replay` (`src/canvas.rs:361-380`) reissues `annotate_link(*rect, target.clone())` in log order. `LinkTarget` is `Clone`.

Tests in `src/canvas.rs`:

- `records_draw_order_advances_and_one_page` (`403-497`): command order ends `… "glyphs", "link", "finish"`. Page ops include `AnnotateLink` as the fifth drawing op. URI `"https://example.test/a"`, rect `(72, 90, 40, 14)`.
- `empty_link_is_rejected_and_not_recorded` (`499-513`): `uri("")` is `EmptyLink` and is not pushed. After `start_page`, the log is only `StartPage`.
- `replay_reissues_every_command_in_order` (`516-553`): `LinkTarget::uri("u")` round-trips.

### `KrillaCanvas` queue (`src/backend/krilla_canvas.rs`)

This file does not `use krilla` (`src/backend/krilla_canvas.rs:1-4`). It buffers a page plan. `emit` is the only place that builds `Document` / `Page` / `Surface`.

```39:53:src/backend/krilla_canvas.rs
pub(super) struct PagePlan {
    pub(super) size: Size,
    pub(super) marks: Box<[Mark]>,
    pub(super) links: Box<[QueuedLink]>,
}

struct OpenPage {
    size: Size,
    marks: Vec<Mark>,
    links: Vec<QueuedLink>,
}
```

```88:93:src/backend/krilla_canvas.rs
/// Link recorded while a surface would still be borrowed.
pub(super) struct QueuedLink {
    pub(super) rect: Rect,
    pub(super) target: LinkTarget,
}
```

`Mark` (`55-86`) is fill, stroke, glyphs, or image. Links are not marks.

`Canvas::annotate_link` (`306-310`):

```306:310:src/backend/krilla_canvas.rs
    fn annotate_link(&mut self, rect: Rect, target: LinkTarget) -> Result<()> {
        let page = self.open_page_mut("annotate_link")?;
        page.links.push(QueuedLink { rect, target });
        Ok(())
    }
```

- Requires an open page (`open_page_mut`, `201-209`). No page → `InvalidCanvasState { operation: "annotate_link", state: "no page is open" }`. A prior failure is returned again via `ensure_ok`.
- Does not read `target`. Does not call `add_annotation`.
- `finish_page` (`236-249`) moves `OpenPage.links` into `PagePlan.links` as a boxed slice. It does not touch krilla. The queue survives until `KrillaCanvas::finish`.
- `finish` (`162-185`) refuses an error or a still-open page, drops font and image tables, then `emit::render(info, &closed_pages, serialization)`.

`backend` is `pub(crate)` and feature `backend-krilla` (`src/lib.rs:36-37`, `src/backend/mod.rs:1-6`). `Cargo.toml` pins `krilla = "=0.8.2"`, `default-features = false`, feature `raster-images` only. No HTTP crate.

### `emit::to_annotation` and `add_annotation` (`src/backend/emit.rs:89-113`, `199-207`)

`emit_page`:

```89:112:src/backend/emit.rs
fn emit_page(document: &mut Document, page_plan: &PagePlan) -> Result<()> {
    // ... PageSettings::from_wh(width, height) ...
    let mut page = document.start_page_with(settings);
    {
        let mut surface = page.surface();
        for mark in &page_plan.marks {
            draw_mark(&mut surface, mark)?;
        }
        surface.finish();
    }

    for link in &page_plan.links {
        page.add_annotation(to_annotation(link)?.into());
    }
    page.finish();
    Ok(())
}
```

`surface()` is called once. Marks are drawn. `surface.finish()` runs, and the block ends, before any annotation. Then each `QueuedLink`, in call order, becomes an annotation. `page.finish()` follows.

```199:207:src/backend/emit.rs
fn to_annotation(link: &QueuedLink) -> Result<LinkAnnotation> {
    let rect = to_krilla_rect(link.rect).ok_or(Error::Backend(
        "Rect::from_xywh rejected a positive Rect".to_owned(),
    ))?;
    Ok(LinkAnnotation::new(
        rect,
        Target::Action(LinkAction::new(link.target.as_str().to_owned()).into()),
    ))
}
```

- Imports: `krilla::action::LinkAction`, `krilla::annotation::{LinkAnnotation, Target}` (`emit.rs:13-14`).
- URI is `as_str().to_owned()` passed to `LinkAction::new`. No trim, no scheme check, no fetch.
- Target is only `Target::Action`. No `Target::Destination`, no outline.
- `to_krilla_rect` returning `None` is `Error::Backend(...)`, not `InvalidGeometry`.
- `LinkAnnotation::new` is called with `(rect, target)` only. No border, highlight, or quad points are set here.

Identity mapping (`src/backend/convert.rs:1-5`, `26-33`):

```1:5:src/backend/convert.rs
//! krilla 0.8.2 `Surface` is already top-left, y-down. Do not pre-flip here.
//! Content-stream and annotation numbers may still differ from API coordinates
//! because krilla applies `page_root_transform` when writing the PDF.
```

`to_krilla_rect` is `KRect::from_xywh(origin.x, origin.y, width, height)` in PDF points. Test `to_krilla_rect_is_identity` (`convert.rs:107-114`): public `(10, 30, 80, 44)` becomes krilla left `10`, top `30`, right `90`, bottom `74`. Those are API numbers, still y-down.

### Backend test that pins the written annotation (`src/backend/emit.rs:233-278`)

`probe_page` strokes a path and annotates public rect `(10, 30, 80, 44)` with `LinkTarget::uri("https://example.test/a")` on a `200×200` page.

`observable_probe_contains_path_ops_and_annot_rect` uses pretty, uncompressed streams. It asserts:

- bytes start with `%PDF`
- the PDF text contains `https://example.test/a`
- content stream has `1 0 0 -1 0 200 cm` (krilla page-root transform; page height 200)
- path operands stay in API space: `10 20 m`, `40 20 l`, `40 50 l`
- annotation is `/Rect [10 126 90 170]`

Comment at `emit.rs:254-260`: the `/Rect` is already transformed; do not flip `to_krilla_point`. The y math matches `y' = 200 - y`: API top `30` → `170`, API bottom `74` → `126`, x stays `10` and `90`.

`dump_experiment_pdf` (`570-584`) is `#[ignore]` and writes `probe.pdf`. It is not part of normal `cargo test`.

No other backend test asserts a link. There is no test that a space-only URI is written, and no test that `annotate_link` without an open page fails (the same `open_page_mut` path is tested for `stroke_path` at `emit.rs:326-338`).

## Flow

1. Caller builds `LinkTarget::uri(s)`. `""` stops here as `EmptyLink`. Any other string, including `" "`, is owned inside `LinkTarget`.
2. Caller passes `(Rect, LinkTarget)` to `Canvas::annotate_link`. The method does not inspect the string.
3. `RecordingCanvas` appends `CanvasCommand::AnnotateLink`. Tests read `commands()` or `pages()`.
4. `KrillaCanvas` appends `QueuedLink { rect, target }` on the open page's `links` vec, separate from `marks`.
5. `finish_page` boxes that vec into `PagePlan`. Still no krilla object.
6. `KrillaCanvas::finish` → `emit::render` → `emit_page` per closed page.
7. Inside `emit_page`: one `surface()`, draw every `Mark`, `surface.finish()`, then `page.add_annotation` for every queued link, then `page.finish()`.
8. `to_annotation` identity-maps the rect and copies the URI into `LinkAction`. krilla applies `page_root_transform` (`Transform::from_row(1, 0, 0, -1, 0, height)`, design `docs/quire-design.md:46`) when it writes the annotation dictionary. The adapter does not pre-flip.

Paint marks and link annotations are split at queue time. All marks are drawn before any annotation is added. Order among links on one page is `annotate_link` call order.

## Files Read

- `src/canvas.rs` — `LinkTarget`, `Canvas`, `CanvasCommand`, `RecordingCanvas`, `replay`, canvas tests
- `src/error.rs` — `EmptyLink` and its display string
- `src/lib.rs` — public re-export; `backend` is `pub(crate)` under the feature
- `src/backend/mod.rs` — only this tree may `use krilla`
- `src/backend/krilla_canvas.rs` — `QueuedLink`, `PagePlan`, `annotate_link`, `finish_page`, `finish`
- `src/backend/emit.rs` — `emit_page`, `to_annotation`, observable probe
- `src/backend/convert.rs` — identity `to_krilla_rect` / `to_krilla_point`, no pre-flip
- `src/geom.rs:365-385` — `Rect` extent is a positive `Size`
- `src/layout/flow.rs` — `FragmentCommand`, `with_offset`, `draw_fragment`
- `src/layout/paginate.rs:266-279` — `paint_pages`
- `src/layout/paragraph.rs` — `InlineItem`, `LinePiece`, `fragment_from_lines`
- `src/document.rs` — `Paragraph::{text,bold,blank*}`, `Image`, `ImageBox` (no URI field)
- `src/text/line_break.rs:18-21` — `Span<T>` is a break atom (`Group` / `Blank`), not a hyperlink
- `Cargo.toml` — krilla pin and features; no HTTP dependency
- `docs/提案书.md:613-619` — §8 confirmed: write the URI string, do not fetch, do not validate scheme; empty rejected at construction; v1 is external URI on text (headers/footers if the same pipeline)
- `docs/quire-design.md:45-46`, `:58`, `:107`, `:117`, `:335`, `:426-438`, `:1310-1316`, `:1661-1666` — `page_root_transform`, queue-until-surface-drop, `Span::Link` rect rule, PR 16 (not implemented as specified)

`rg` over `src/layout` and `src/document.rs`: no `annotate_link`, no `LinkTarget`, no `krilla::`.

## Boundaries

- Layout and document code may call `Canvas`. They must not `use krilla`. That rule holds today: `draw_fragment` (`src/layout/flow.rs:347-376`) only calls `draw_glyph_run`, `fill_path`, `stroke_path`, `draw_image`. `paint_pages` (`src/layout/paginate.rs:266-279`) only `start_page`, `draw_fragment`, `finish_page`.
- `krilla` types stay in `src/backend/{emit,convert,images}.rs`. `krilla_canvas.rs` is the `Canvas` impl and stays krilla-free.
- The library does not fetch a URI and does not check that a scheme can be opened. Confirmed in code (`LinkTarget::uri` is an emptiness check; `LinkAction::new` receives the raw string) and in 提案书 §8 (`docs/提案书.md:617`) and the design non-goal (`docs/quire-design.md:107`). `document.rs:415` `fetch_add` is an atomic document-token counter, not a network fetch.
- v1 annotation shape is external URI only. `linkToPage` / `linkToDestination` are listed in the proposal table (`docs/提案书.md:158`) and then limited: internal jumps and image-only links can come later (`docs/提案书.md:619`). Emit never builds `Target::Destination`.

### Already done (canvas + backend)

- Non-empty URI newtype, empty rejected once, spaces kept.
- `annotate_link(Rect, LinkTarget)` on the trait, the recorder, and `KrillaCanvas`.
- Separate link queue, written with `add_annotation` only after `surface.finish()`.
- Identity geometry into krilla; observed PDF `/Rect` pinned for one probe.
- Replay and `RecordingCanvas::pages` preserve the link command.

### What layout still must feed

Nothing in the layout or DSL calls `annotate_link`.

Current inline model:

- `Paragraph` methods are `text`, `bold`, `blank`, `blank_em`, `blank_to_end`, `first_line_indent` (`src/document.rs:795-825`). No link method.
- `InlineItem` is `Text(TextRun)` or `Blank` (`src/layout/paragraph.rs:37-40`). `TextRun` is text plus `Face` (`22-25`).
- `LinePiece` is `Glyphs` or `Stroke` (`75-78`).
- `fragment_from_lines` (`308-337`) emits only `FragmentCommand::Glyphs` and `Stroke`.
- `FragmentCommand` (`src/layout/flow.rs:81-96`) has no link arm. `Fragment::with_offset` (`63-76`) and `draw_fragment` match that enum exhaustively, so a new arm has to be added in both or links will not move with the fragment origin.

What those layers still have to supply, from 提案书 §8 (`616-619`) and the design link section (`docs/quire-design.md:1310-1316`):

- A URI hanging on a run of text. After line breaking, one rectangle per line that run occupies: height of the line box, width of that run's advance on that line.
- Those rectangles in the same top-left, y-down coordinates as other paint, translated by the placed fragment origin the way `draw_fragment` already translates glyphs and images.
- A call to `Canvas::annotate_link` with `LinkTarget::uri(...)`. Empty input stays `Error::EmptyLink` from that constructor. Layout should not trim, parse a scheme, or open the URI.
- Headers and footers use the same `paint_pages` stack (`paginate.rs:269-273`), so a fragment-level link command would cover them if paragraph layout emits it. Image-only hit targets are explicitly later (`提案书.md:619`); `Image` / `ImageBox` (`document.rs:419-451`) have no link field, and `layout/image.rs` only pushes `FragmentCommand::Image`.

`text::line_break::Span` (`src/text/line_break.rs:18-21`) is not that hyperlink span. Design `Span::Link` / `FragmentCommand::Link` (`quire-design.md:1312`) are not in the tree.

Design PR 16 (`quire-design.md:1661-1666`) describes the missing layout work: two `annotate_link` calls when one link wraps to two lines, `EmptyLink` on an empty URI, `RecordingCanvas` rect asserts. The adapter half of that PR (queue, annotate after surface drop, `%PDF`) is already in `emit.rs`. The paragraph half is not.

## Non-Obvious Things

- **Annotation time is `emit_page`, not `Canvas::finish_page`.** The design (`quire-design.md:437-438`) says `finish_page` drops the surface and then `add_annotation`, and that `KrillaCanvas` owns a `krilla::Document`. The code does not. `KrillaCanvas` holds no live krilla value (`krilla_canvas.rs:95-96`). `finish_page` only boxes `QueuedLink`s. The surface is created later, inside `emit_page`, during `KrillaCanvas::finish`. The drop-then-annotate order is the block at `emit.rs:99-110`: `surface.finish()` and the end of the borrow, then the `for link` loop.
- **Do not pre-flip.** Public `(10, 30, 80×44)` is krilla `(left 10, top 30, right 90, bottom 74)` (`convert.rs:107-114`). The PDF annotation is `/Rect [10 126 90 170]` on a height-200 page (`emit.rs:256-278`) because krilla's `page_root_transform` is `1 0 0 -1 0 height`. Path operands stay in API space because that matrix is written into the content stream as `cm`. Annotation numbers are already transformed when written. `to_krilla_rect` stays an identity.
- **`to_krilla_rect` failure is `Error::Backend`, not `InvalidGeometry`.** Design line 438 says `InvalidGeometry`. `emit.rs:200-202` uses `Backend("Rect::from_xywh rejected a positive Rect")`. A branded positive `Rect` makes `None` unlikely; the variant still differs.
- **Empty is rejected only at `LinkTarget::uri`.** `annotate_link` on both canvases trusts the newtype. `" "` is a legal target and would be copied into `LinkAction` unchanged. There is no second check at queue or emit time.
- **Links are not drawn.** They never enter `Mark` or `Surface`. A page can have links and no marks: `emit_page` still opens a surface, finishes it, then adds annotations. The probe always pairs one stroke with one link.
- **RecordingCanvas and KrillaCanvas disagree on lifecycle.** The recorder accepts `annotate_link` with no open page. `KrillaCanvas` rejects it. `pages()` only keeps a link that sits between `StartPage` and `FinishPage`.
- **Design metadata timing is also stale, adjacent to this path.** Design says metadata must be set before the first page (`quire-design.md:436`). `late_document_info_still_applies` (`emit.rs:553-568`) sets the title after `finish_page` and still finds it in the PDF, because `set_document_info` only stores `DocumentInfo` and `render` applies it at the start of emission.

## Open Questions

- Layout shape is not in this tree: whether the URI lives on a new `InlineItem`, how a wrapped run becomes one rect per line, and whether `FragmentCommand` grows a link arm. Design `quire-design.md:1312` states the rect rule; 提案书 §8 states the product rule. Neither is implemented.
- Image hit-testing is deferred by 提案书 §8. The design sentence at `:1314` says an image `link` uses the same queue. No image field exists.
- No code sets annotation appearance (border, highlight). If a reader shows a visible box, that is krilla's default, not a quire setting. This pass did not read krilla's `LinkAnnotation` source beyond the call site.
- `to_krilla_rect` → `None` is untested. The probe only covers one positive rect.
- A space-only URI is tested at construction (`canvas.rs:505-506`) and not in a PDF byte assert.
