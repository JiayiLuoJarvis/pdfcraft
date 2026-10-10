# Candidate 3. Split-tape session, emit-only krilla

`KrillaCanvas` implements the existing `Canvas` trait and never stores a `krilla::Document`, `Page`, or `Surface`. Each in-scope call lowers straight into a page program whose content-stream ops and link annotations are different vectors. `finish` either refuses or hands that program to `emit`, the only function that constructs krilla values. Glyph runs and images are counters on the session, not operations the emitter can match.

Crate name is `quire-pdf`. `KrillaCanvas` stays `pub(crate)`. `lib.rs` does not gain a re-export.

## Usage

The caller this knife is a crate test, or the later in-crate `to_pdf` that does not exist yet. A downstream user of `quire-pdf` still draws on `RecordingCanvas`. They do not name krilla, and they do not call `finish`.

### Internal quickstart

```rust
use crate::backend::session::KrillaCanvas;
use crate::canvas::{Canvas, DocumentInfo, LinkTarget};
use crate::geom::{Color, Paint, Path, Point, Positive, Rect, Size, Stroke};

let mut canvas = KrillaCanvas::new();
canvas.set_document_info(&DocumentInfo {
    title: Some("Probe".to_owned()),
    author: None,
    subject: None,
    keywords: Vec::new(),
    language: None,
})?;
canvas.start_page(Size::try_pt(200.0, 200.0)?)?;
let path = Path::begin(Point::try_pt(10.0, 20.0)?)
    .line_to(Point::try_pt(40.0, 20.0)?)
    .line_to(Point::try_pt(40.0, 50.0)?)
    .finish();
canvas.stroke_path(
    &path,
    &Paint::solid(Color::rgb(0, 0, 0)),
    &Stroke::new(Positive::pt(1.0)?),
)?;
canvas.annotate_link(
    Rect::try_pt(10.0, 30.0, 80.0, 44.0)?,
    LinkTarget::uri("https://example.test/a")?,
)?;
canvas.finish_page()?;
let bytes = canvas.finish()?;
assert!(bytes.starts_with(b"%PDF"));
```

`?` on the canvas methods uses `quire_pdf::Result`. `finish` does not. Its error is `EmitError`.

### Call site A. Empty page

```rust
let mut canvas = KrillaCanvas::new();
canvas.start_page(Size::try_pt(200.0, 200.0)?)?;
canvas.finish_page()?;
let bytes = canvas.finish()?;
assert!(bytes.starts_with(b"%PDF"));
```

### Call site B. Glyph or image on this knife

```rust
let mut canvas = KrillaCanvas::new();
canvas.start_page(Size::try_pt(200.0, 200.0)?)?;
canvas.draw_glyph_run(&run)?;
canvas.draw_image(&image, Rect::try_pt(10.0, 10.0, 20.0, 20.0)?)?;
canvas.finish_page()?;
let err = canvas.finish().expect_err("no pdf");
assert_eq!(err, EmitError::Deferred { glyphs: 1, images: 1 });
```

`draw_glyph_run` and `draw_image` return `Ok(())`. They do not panic. `finish` returns `Err` and no bytes, including when the same page also contained a path. A later knife replaces the counters with real paint operations. This knife has no `draw_glyphs` call to delete.

### Call site C. Identity convert

```rust
let point = crate::geom::Point::try_pt(10.0, 20.0)?;
let got = crate::backend::convert::to_krilla_point(point);
let expect = krilla::geom::Point::from_xy(10.0, 20.0);
assert_eq!(got.x().to_bits(), expect.x().to_bits());
assert_eq!(got.y().to_bits(), expect.y().to_bits());
```

If `krilla::geom::Point` exposes `x` and `y` as fields instead of methods, the test reads the fields. It still compares against `from_xy(10.0, 20.0)`. It does not read the PDF.

## Module map

```text
src/backend/          feature backend-krilla only, pub(crate), no lib re-export
  mod.rs              declares convert, session, emit
  convert.rs          identity Length accessors -> krilla geom. No Document.
  session.rs          KrillaCanvas, Session, PageBuf, PaintOp, EmitError.
                      Zero krilla imports.
  emit.rs             the only file that constructs Document, Page, Surface,
                      Metadata, PathBuilder, Fill, Stroke, LinkAnnotation.
```

Delete the `krilla_canvas.rs` stub. Do not add a fourth wrapper module.

Dependency direction. `session` calls `emit` and does not import krilla. `emit` calls `convert` and krilla. `layout` and `document` stay unaware of both. `canvas.rs` is unchanged.

A reader answers "where do the bytes come from?" in `emit.rs`, "what can a canvas call store?" in `session.rs`, and "did we flip y?" in `convert.rs`.

## Cargo.toml

`publish = false` stays. No `skrifa`. No `simple-text`. No `rayon`.

```toml
[features]
default = ["backend-krilla"]
backend-krilla = ["dep:krilla"]

[dependencies]
krilla = { version = "=0.8.2", optional = true, default-features = false, features = ["raster-images"] }
```

`lib.rs` already gates `pub(crate) mod backend` on the feature. Leave that line alone.

## Types

`quire::Error` is not edited. It stays `Copy` with `NonFiniteLength`, `NegativeLength`, `NonPositiveLength`, and `EmptyLink`.

```rust
/// PDF emission failure. Crate-private. Not a quire::Error variant.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum EmitError {
    /// fill, stroke, link, or finish_page with no start_page in force.
    NoOpenPage,
    /// start_page while a page is open, or finish while a page is open.
    PageStillOpen,
    /// set_document_info after the first start_page.
    InfoAfterPage,
    /// finish with no closed page.
    NoPage,
    /// draw_glyph_run or draw_image was called. No PDF bytes.
    Deferred { glyphs: u32, images: u32 },
    /// PageSettings::from_wh returned None.
    RejectedPage,
    /// Rect::from_xywh returned None.
    RejectedRect,
    /// PathBuilder::finish returned None.
    RejectedPath,
    /// NormalizedF32::new returned None for a color alpha.
    RejectedOpacity,
    /// Document::finish failed. Text is that error's Display.
    Finish(String),
}
```

`Display` names the variant in plain text. It does not print a krilla type path. `Finish` prints the `String` it already holds.

```rust
enum Session {
    Open(OpenSession),
    Fault(EmitError),
}

struct OpenSession {
    info: DocumentInfo,
    closed: Vec<PageBuf>,
    page: Option<PageBuf>,
    deferred_glyphs: u32,
    deferred_images: u32,
}

struct PageBuf {
    size: Size,
    paints: Vec<PaintOp>,
    links: Vec<QueuedLink>,
}

enum PaintOp {
    Fill { path: Path, paint: Paint },
    Stroke { path: Path, paint: Paint, stroke: Stroke },
}

struct QueuedLink {
    rect: Rect,
    target: LinkTarget,
}

#[must_use]
pub(crate) struct KrillaCanvas {
    session: Session,
}
```

Invariants the types hold.

- `PaintOp` has no glyph arm and no image arm. `emit` cannot call `Surface::draw_glyphs`, `Surface::draw_text`, or `Surface::draw_image`.
- `links` is not a `PaintOp`. The emitter paints inside `surface`, calls `surface.finish()`, then reads `links`. The borrow of `Surface<'_>` is over before `add_annotation`.
- `Session::Fault` carries the first sequencing error. Later canvas calls leave it in place.
- `Path` is already non-empty and starts with `MoveTo`. `Stroke::width` is `Length<Positive>`. `Size` is two `Length<Positive>` values. The session does not re-check those brands.
- `LinkTarget` is already non-empty. `annotate_link` does not look at the string.

`PageBuf` is the single stored representation. `emit` consumes it and does not build a second command log.

## Signatures

```rust
impl KrillaCanvas {
    pub(crate) fn new() -> Self { /* Open, empty info, no page, counters 0 */ }

    /// Consumes the session.
    ///
    /// Order, first match wins.
    /// Fault, then Deferred, then PageStillOpen, then NoPage, then emit.
    /// Drop of a canvas that was not finished does not build a Document.
    pub(crate) fn finish(self) -> Result<Vec<u8>, EmitError> {
        unimplemented!()
    }
}

impl Canvas for KrillaCanvas {
    fn set_document_info(&mut self, info: &DocumentInfo) -> Result<()>;
    fn start_page(&mut self, size: Size) -> Result<()>;
    fn finish_page(&mut self) -> Result<()>;
    fn fill_path(&mut self, path: &Path, paint: &Paint) -> Result<()>;
    fn stroke_path(&mut self, path: &Path, paint: &Paint, stroke: &Stroke) -> Result<()>;
    fn draw_image(&mut self, image: &DecodedImage, rect: Rect) -> Result<()>;
    fn draw_glyph_run(&mut self, run: &GlyphRun) -> Result<()>;
    fn annotate_link(&mut self, rect: Rect, target: LinkTarget) -> Result<()>;
}
```

Every `Canvas` method returns `Ok(())` on this knife, including after a fault and including glyph and image. Sequencing failures and deferred draws surface at `finish`. That keeps `quire::Error` unchanged. The trait cannot return `EmitError`.

Transitions inside `Session::Open`.

| Call | When | Effect |
| --- | --- | --- |
| `set_document_info` | no `start_page` yet | replace `info` |
| `set_document_info` | any page started | `Fault(InfoAfterPage)` |
| `start_page` | `page` is `None` | `page = Some(PageBuf { size, empty vecs })` |
| `start_page` | `page` is `Some` | `Fault(PageStillOpen)` |
| `fill_path` / `stroke_path` / `annotate_link` | `page` is `Some` | push one owned op |
| those three | `page` is `None` | `Fault(NoOpenPage)` |
| `finish_page` | `page` is `Some` | move it onto `closed` |
| `finish_page` | `page` is `None` | `Fault(NoOpenPage)` |
| `draw_glyph_run` | still `Open` | `deferred_glyphs += 1`, drop the run |
| `draw_image` | still `Open` | `deferred_images += 1`, drop the image |

`Session::Fault` ignores further calls. The first error stays.

A repeated `set_document_info` before any page replaces `info`. The same inputs through a new canvas produce the same request. `finish` consumes `self`, so a second `finish` does not compile. A failed `finish` returns `Err` and drops the local `Document` without returning bytes. The caller builds a new canvas and draws again. `creation_date` is not set.

## convert.rs

Accessors only. `Rect` has `origin()` and `size()`, not `x` or `width` fields. Do not pre-flip.

```rust
pub(crate) fn to_krilla_point(p: crate::geom::Point) -> krilla::geom::Point {
    krilla::geom::Point::from_xy(p.x().as_pt(), p.y().as_pt())
}

pub(crate) fn to_krilla_rect(r: crate::geom::Rect) -> Option<krilla::geom::Rect> {
    krilla::geom::Rect::from_xywh(
        r.origin().x().as_pt(),
        r.origin().y().as_pt(),
        r.size().width().as_pt(),
        r.size().height().as_pt(),
    )
}

pub(crate) fn to_krilla_size(s: crate::geom::Size) -> Option<krilla::geom::Size> {
    krilla::geom::Size::from_wh(s.width().as_pt(), s.height().as_pt())
}
```

`to_krilla_size` exists for the later image knife. This knife's pages pass width and height straight into `PageSettings::from_wh`. A `None` from that call is `EmitError::RejectedPage`, not a `quire::Error` variant.

## emit.rs

```rust
pub(crate) fn emit(program: OpenSession) -> Result<Vec<u8>, EmitError> {
    let mut doc = krilla::Document::new_with(probe_settings());
    doc.set_metadata(to_metadata(&program.info));
    for page in program.closed {
        emit_page(&mut doc, page)?;
    }
    match doc.finish() {
        Ok(bytes) => Ok(bytes),
        Err(err) => Err(EmitError::Finish(err.to_string())),
    }
}

fn probe_settings() -> krilla::SerializeSettings {
    krilla::SerializeSettings {
        pretty: true,
        compress_content_streams: false,
        enable_tagging: false,
        ..krilla::SerializeSettings::default()
    }
}
```

`enable_tagging: false` is required. `..Default::default()` leaves tagging on. Pretty and uncompressed streams exist so the probe can read operands. This knife has no second settings argument.

```rust
fn emit_page(doc: &mut krilla::Document, page: PageBuf) -> Result<(), EmitError> {
    let settings = krilla::page::PageSettings::from_wh(
        page.size.width().as_pt(),
        page.size.height().as_pt(),
    )
    .ok_or(EmitError::RejectedPage)?;
    let mut krilla_page = doc.start_page_with(settings);
    let mut surface = krilla_page.surface();
    for op in &page.paints {
        paint_one(&mut surface, op)?;
    }
    surface.finish();
    for link in page.links {
        let rect = to_krilla_rect(link.rect).ok_or(EmitError::RejectedRect)?;
        let action = krilla::action::LinkAction::new(link.target.as_str().to_owned());
        let annot = krilla::annotation::LinkAnnotation::new(
            rect,
            krilla::annotation::Target::Action(action.into()),
        );
        krilla_page.add_annotation(annot.into());
    }
    krilla_page.finish();
    Ok(())
}
```

`Page::surface` resets the page if called again. `emit_page` calls it once. `Surface::finish` consumes the surface, which ends the borrow, and panics only when push and pop are unbalanced. This knife never pushes.

```rust
fn paint_one(surface: &mut krilla::surface::Surface<'_>, op: &PaintOp) -> Result<(), EmitError> {
    let path = to_krilla_path(path_of(op))?;
    match op {
        PaintOp::Fill { paint, .. } => {
            surface.set_stroke(None);
            surface.set_fill(Some(to_fill(*paint)?));
        }
        PaintOp::Stroke { paint, stroke, .. } => {
            surface.set_fill(None);
            surface.set_stroke(Some(to_stroke(*paint, *stroke)?));
        }
    }
    surface.draw_path(&path);
    Ok(())
}

fn to_krilla_path(path: &crate::geom::Path) -> Result<krilla::geom::Path, EmitError> {
    let mut builder = krilla::geom::PathBuilder::new();
    for verb in path.verbs() {
        match verb {
            PathVerb::MoveTo(p) => builder.move_to(p.x().as_pt(), p.y().as_pt()),
            PathVerb::LineTo(p) => builder.line_to(p.x().as_pt(), p.y().as_pt()),
            PathVerb::Close => builder.close(),
        }
    }
    builder.finish().ok_or(EmitError::RejectedPath)
}
```

`to_fill` and `to_stroke` read `paint.color().channels()`. Opacity is `NormalizedF32::new(a as f32 / 255.0)`. `None` is `RejectedOpacity`, not a panic. Alpha 0 still sets a fill or stroke. Stroke width is `stroke.width().as_pt()`. Cap, join, miter, and dash stay `Stroke::default()`. Fill rule is `FillRule::NonZero`.

Metadata, applied once before the page loop.

| `DocumentInfo` | krilla `Metadata` | Rule |
| --- | --- | --- |
| `title` | `title` | `Some` and non-empty |
| `author` | `authors` | non-empty becomes `vec![author]` |
| `subject` | `description` | non-empty |
| `keywords` | `keywords` | drop empty strings, skip if none remain |
| `language` | `language` | non-empty, no RFC check |
| none | `creator` | always `"quire"` |
| none | `creation_date`, `producer`, `text_direction` | leave unset |

## Tests

Place them in `emit.rs` and `convert.rs` under `#[cfg(test)]`. An integration test in `tests/` cannot see `pub(crate)` items.

Behavior the tests call.

1. `to_krilla_point(Point::try_pt(10.0, 20.0))` equals `krilla::geom::Point::from_xy(10.0, 20.0)` by bits. Same for `to_krilla_rect(Rect::try_pt(10.0, 30.0, 80.0, 44.0))` against `Rect::from_xywh(10.0, 30.0, 80.0, 44.0)` and for `to_krilla_size(Size::try_pt(200.0, 200.0))` against `Size::from_wh(200.0, 200.0)`.
2. Empty page `finish` bytes start with `%PDF`.
3. The quickstart page. Bytes start with `%PDF`. The lossy UTF-8 of the file contains the URI `https://example.test/a` and the title `Probe`.
4. The same file contains the path operands and the `Annot` rectangle numbers from the first green run. Hypothesis, labeled as a guess from `Transform::from_row(1, 0, 0, -1, 0, height)` on a 200 pt page. Path operands `10 180`, `40 180`, `40 150`. Annotation rectangle `10 126 90 170`. On mismatch, replace the literals with the numbers in the failure output. Do not change `convert.rs` to chase the file.
5. `draw_glyph_run` then `draw_image` then `finish` equals `EmitError::Deferred { glyphs: 1, images: 1 }`.
6. `finish` on a new canvas equals `EmitError::NoPage`. `start_page` without `finish_page` equals `EmitError::PageStillOpen`.

Do not assert `paints.len()`, do not snapshot the whole file, and do not compare compressed stream bytes.

## Out of this knife

No `Surface::draw_text`. No font map. No image decode. No `document` DSL. No public error variant. No `unsafe`. No second PDF crate. `publish` stays false.
