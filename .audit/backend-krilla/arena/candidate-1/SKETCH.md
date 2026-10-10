# Candidate 1 sketch: plan-then-serialize krilla backend

Sketch only. No production code under `src/`. Bodies are `todo!()`.

krilla API facts are taken from `docs/quire-design.md` (which records a prior
verification pass). krilla is not vendored and not in the local cargo registry,
so nothing here was re-checked against krilla 0.8.2 this session. Items whose
exact shape I could not confirm are marked `// UNVERIFIED`.

## The one-sentence shape

`KrillaCanvas` holds **no krilla `Document`**. It converts each `Canvas` call
into an owned, already-validated krilla value and files it into a `Plan`.
`finish(self)` hands the `Plan` to one function where `Document`, `Page` and
`Surface` live as locals in a single stack frame and never escape it. The
self-referential struct problem does not get solved; it stops existing.

## Usage (caller's view)

The caller this knife serves is crate-internal. There is no document DSL yet, so
the three real call sites are the future `Document::to_pdf`, a hand-driven
backend test, and the failure path.

### Call site 1: the future `to_pdf` (not built this knife)

```rust
// src/document.rs, once the DSL lands.
#[cfg(feature = "backend-krilla")]
pub fn to_pdf(&self) -> Result<Vec<u8>> {
    let mut canvas = crate::backend::KrillaCanvas::new();
    self.write_to(&mut canvas, &WriteOptions::default())?;
    canvas.finish()
}
```

Three lines. The caller never sees a page, a surface, a link queue, or a
serialize setting. `write_to` already takes `&mut dyn Canvas`, so `KrillaCanvas`
drops into the slot `RecordingCanvas` occupies today with no other change.

### Call site 2: driving the canvas by hand (a backend unit test)

```rust
let mut canvas = KrillaCanvas::new();
canvas.set_document_info(&DocumentInfo {
    title: Some("询证函".to_owned()),
    ..DocumentInfo::default()
})?;
canvas.start_page(Size::try_pt(200.0, 200.0)?)?;
canvas.stroke_path(
    &Path::begin(Point::try_pt(10.0, 20.0)?)
        .line_to(Point::try_pt(40.0, 20.0)?)
        .line_to(Point::try_pt(40.0, 50.0)?)
        .finish(),
    &Paint::solid(Color::rgb(0, 0, 0)),
    &Stroke::new(Positive::pt(1.0)?),
)?;
canvas.annotate_link(
    Rect::try_pt(10.0, 30.0, 80.0, 44.0)?,
    LinkTarget::uri("https://example.test/a")?,
)?;
canvas.finish_page()?;

let pdf = canvas.finish()?;
assert!(pdf.starts_with(b"%PDF"));
```

Ordering rules the caller does **not** have to know, because the `Plan` absorbs
them: metadata may be set before, between, or after pages; a link may be
recorded at any point during a page and still lands after the surface closes;
`finish` needs no prior `finish_page` bookkeeping beyond the page itself.

### Call site 3: a failure surfaces at the offending call, not at `finish`

```rust
let mut canvas = KrillaCanvas::new();
canvas.start_page(A4)?;

// A path with a single MoveTo has no drawable segment.
let degenerate = Path::begin(Point::try_pt(5.0, 5.0)?).finish();
assert!(matches!(
    canvas.stroke_path(&degenerate, &paint, &stroke),
    Err(Error::InvalidGeometry)
));

// Glyphs and images are deferred, not stubbed out with a panic.
assert!(matches!(
    canvas.draw_glyph_run(&run),
    Err(Error::UnknownFont { id: 1 })
));

// The canvas is still usable and still produces a whole PDF.
canvas.finish_page()?;
assert!(canvas.finish()?.starts_with(b"%PDF"));
```

The rejected calls left nothing behind. There is no half-built krilla
`Document` to abandon, because there is no krilla `Document` yet.

## Module map

No new files. The two placeholder modules that already exist take the code.

```text
src/backend/
  mod.rs            mod convert; mod krilla_canvas; pub(crate) use KrillaCanvas;
  convert.rs        crate geom/paint -> owned krilla values. Identity. Pure. No borrows.
  krilla_canvas.rs  KrillaCanvas (Canvas impl) + Plan + PlannedPage + PageOp + Plan::serialize.
```

`convert.rs` and `krilla_canvas.rs` are private modules inside `backend`, which
is itself `pub(crate)` and feature-gated. `mod.rs` re-exports exactly one name.

Why `Plan` and its serializer share a file with `KrillaCanvas` rather than
getting a `plan.rs`: splitting "build the plan" from "render the plan" into
separate modules is textbook temporal decomposition (`design-red-flags.md`).
They protect one decision, which is what a page is made of. One file, two types,
one flow a reader traces top to bottom.

Dependency direction is unchanged: `layout -> canvas`, `backend -> canvas +
krilla`. Nothing in `layout/`, `text/`, `fonts/` or `lib.rs` learns that krilla
exists.

## Types

### `src/backend/krilla_canvas.rs`

```rust
//! `Canvas` implementation that plans a whole document, then serializes it.
//!
//! krilla's `Document`, `Page<'_>` and `Surface<'_>` form a borrow chain that
//! no struct can hold under `#![forbid(unsafe_code)]`. They appear only as
//! locals inside [`Plan::serialize`], so the chain never outlives one stack
//! frame and nothing needs a self-referential struct.

use krilla::geom as kgeom;
use krilla::metadata::Metadata;
use krilla::page::PageSettings;
use krilla::paint::{Fill, Stroke as KStroke};
use krilla::path::Path as KPath;

use crate::canvas::{Canvas, DecodedImage, DocumentInfo, GlyphRun, LinkTarget};
use crate::error::{Error, Result};
use crate::geom::{Paint, Path, Rect, Size, Stroke};

use super::convert;

/// One drawing operation, already converted and already valid.
///
/// The variant list is the scope statement for this knife. Glyph runs and
/// images have no variant because they are rejected at the `Canvas` call, so
/// [`Plan::serialize`] has no arm that could panic or silently skip.
enum PageOp {
    Fill { path: KPath, fill: Fill },
    Stroke { path: KPath, stroke: KStroke },
}

/// A hyperlink waiting for the page's surface to close.
struct Link {
    rect: kgeom::Rect,
    uri: String,
}

/// One page's whole content.
///
/// `ops` and `links` are separate fields, not one ordered list, because krilla
/// requires every annotation to be added after the surface is dropped. The
/// ordering rule is structural here: the surface loop cannot reach `links`.
struct PlannedPage {
    settings: PageSettings,
    ops: Vec<PageOp>,
    links: Vec<Link>,
}

impl PlannedPage {
    fn new(settings: PageSettings) -> Self {
        todo!("empty ops and links")
    }
}

/// A validated document with no krilla document state attached.
///
/// Every value inside is owned and borrow-free, so a `Plan` can be built,
/// moved, and dropped without touching krilla's serializer.
struct Plan {
    /// Applied before the first page regardless of when the caller set it.
    metadata: Option<Metadata>,
    /// Pages in the order `finish_page` closed them.
    pages: Vec<PlannedPage>,
}

impl Plan {
    /// Writes the whole document.
    ///
    /// The only function in the crate that constructs a krilla `Document`.
    /// `settings` is a parameter so tests can ask for the uncompressed,
    /// pretty, untagged serialization without widening any public surface.
    ///
    /// Pure in the sense that matters: the same `Plan` yields the same bytes
    /// every time, because no creation date is set (`quire-design.md` metadata
    /// table), per make-operations-idempotent.
    fn serialize(self, settings: krilla::SerializeSettings) -> Result<Vec<u8>> {
        // let mut doc = krilla::Document::new_with(settings);
        // if let Some(metadata) = self.metadata { doc.set_metadata(metadata); }
        // for planned in self.pages {
        //     let mut page = doc.start_page_with(planned.settings);
        //     {
        //         let mut surface = page.surface();   // borrows `page`
        //         for op in &planned.ops {
        //             match op {
        //                 PageOp::Fill { path, fill } => {
        //                     surface.set_fill(Some(fill.clone()));
        //                     surface.set_stroke(None);
        //                     surface.draw_path(path);
        //                 }
        //                 PageOp::Stroke { path, stroke } => {
        //                     surface.set_fill(None);
        //                     surface.set_stroke(Some(stroke.clone()));
        //                     surface.draw_path(path);
        //                 }
        //             }
        //         }
        //     }                                      // surface dropped; `page` free again
        //     for link in planned.links {
        //         page.add_annotation(convert::to_annotation(link));
        //     }
        //     page.finish();
        // }
        // doc.finish().map_err(|err| Error::Backend(err.to_string()))
        todo!("single stack frame owns Document, Page and Surface")
    }
}

/// Production serialization: compressed streams, krilla defaults.
fn release_settings() -> krilla::SerializeSettings {
    todo!("SerializeSettings::default() with enable_tagging false")
}

/// Experiment serialization for the content-stream and annotation tests.
///
/// Matches `quire-design.md`: pretty, uncompressed, tagging off.
#[cfg(test)]
fn experiment_settings() -> krilla::SerializeSettings {
    todo!("pretty: true, compress_content_streams: false, enable_tagging: false")
}

/// Draws a document into PDF bytes.
///
/// Crate-private. Never named in `lib.rs` re-exports, and no method on it
/// mentions a krilla type.
pub(crate) struct KrillaCanvas {
    plan: Plan,
    /// The page opened by `start_page` and not yet closed by `finish_page`.
    open: Option<PlannedPage>,
}

impl KrillaCanvas {
    pub(crate) fn new() -> Self {
        todo!("empty plan, no open page")
    }

    /// Serializes everything closed so far.
    ///
    /// Deliberately not on [`Canvas`]: `RecordingCanvas` has no bytes to
    /// return, and taking `self` is what makes "no half PDF after a failure"
    /// a type-level fact rather than a convention.
    ///
    /// An `open` page is dropped, matching `RecordingCanvas::pages()`, which
    /// leaves out a `StartPage` with no matching `FinishPage`.
    pub(crate) fn finish(self) -> Result<Vec<u8>> {
        // self.plan.serialize(release_settings())
        todo!("serialize the plan with release settings")
    }

    /// Files an already-valid op on the open page, or drops it when no page
    /// is open.
    ///
    /// Dropping matches `RecordingCanvas::pages()`, which keeps only the
    /// commands between a `StartPage` and its `FinishPage`. The two `Canvas`
    /// implementations stay interchangeable, which is what the cross-check
    /// test asserts.
    fn record(&mut self, op: PageOp) {
        todo!("push onto self.open when there is one")
    }
}

impl Canvas for KrillaCanvas {
    /// Stores metadata. Last write wins; the call may come at any time.
    ///
    /// krilla needs `set_metadata` before the first page. The plan hoists it,
    /// so that requirement never reaches the caller.
    fn set_document_info(&mut self, info: &DocumentInfo) -> Result<()> {
        // self.plan.metadata = Some(convert::to_metadata(info));
        todo!()
    }

    /// Opens a page.
    ///
    /// An already-open page is discarded, matching `RecordingCanvas::pages()`
    /// on a `StartPage` that follows an unclosed `StartPage`.
    fn start_page(&mut self, size: Size) -> Result<()> {
        // self.open = Some(PlannedPage::new(convert::to_page_settings(size)?));
        todo!()
    }

    fn finish_page(&mut self) -> Result<()> {
        // if let Some(page) = self.open.take() { self.plan.pages.push(page); }
        todo!("move the open page into the plan; a second call is a no-op")
    }

    fn fill_path(&mut self, path: &Path, paint: &Paint) -> Result<()> {
        // let op = PageOp::Fill {
        //     path: convert::to_krilla_path(path)?,
        //     fill: convert::to_fill(paint)?,
        // };
        // self.record(op);
        todo!("convert first, record second: a rejected call records nothing")
    }

    fn stroke_path(&mut self, path: &Path, paint: &Paint, stroke: &Stroke) -> Result<()> {
        todo!("PageOp::Stroke from to_krilla_path + to_stroke")
    }

    /// Deferred to the image knife.
    ///
    /// Non-panicking by design: the layout code that calls this must be able
    /// to run against the krilla backend today and get a typed refusal.
    fn draw_image(&mut self, image: &DecodedImage, _rect: Rect) -> Result<()> {
        // Err(Error::UnknownImage { id: image.id.as_u32() })
        todo!()
    }

    /// Deferred to the font knife. No `FontStore`, no skrifa, no panic.
    fn draw_glyph_run(&mut self, run: &GlyphRun) -> Result<()> {
        // Err(Error::UnknownFont { id: run.font.as_u32() })
        todo!()
    }

    /// Queues a link on the open page.
    ///
    /// The rect is converted here, so an invalid rect fails at this call even
    /// when no page is open. Validation does not depend on page state.
    fn annotate_link(&mut self, rect: Rect, target: LinkTarget) -> Result<()> {
        // let link = Link { rect: convert::to_krilla_rect(rect)?, uri: target.as_str().to_owned() };
        // if let Some(page) = self.open.as_mut() { page.links.push(link); }
        todo!()
    }
}
```

### `src/backend/convert.rs`

```rust
//! Identity mapping from crate geometry to krilla geometry.
//!
//! krilla 0.8.2's `Surface` is already top-left, y-down. Do not pre-flip here.
//! The numbers in the PDF differ from the numbers passed in because krilla
//! applies `page_root_transform` when it writes content streams and annotation
//! rectangles. That is krilla's business, not this module's.
//!
//! The crate's branded geometry makes most of these conversions total:
//! `Point` is `Finite` on both axes, `Size` is `Positive` on both, `Stroke`
//! width is `Positive`, and `Color` channels are `u8`, so the krilla
//! constructors that return `Option` cannot return `None` for values built
//! through the public API. Those arms still map to [`Error::InvalidGeometry`]
//! rather than panicking, and are marked unreachable in prose only. The one
//! genuinely reachable failure is a `Path` whose only verb is `MoveTo`.

use crate::canvas::DocumentInfo;
use crate::error::{Error, Result};
use crate::geom::{Paint, Path, Rect, Size, Stroke};

/// Identity. `Finite` on both axes means this cannot fail.
pub(super) fn to_krilla_point(point: crate::geom::Point) -> krilla::geom::Point {
    // krilla::geom::Point::from_xy(point.x().as_pt(), point.y().as_pt())
    todo!()
}

/// Identity. `Err` is unreachable for a `Rect` built through `geom`.
pub(super) fn to_krilla_rect(rect: Rect) -> Result<krilla::geom::Rect> {
    // krilla::geom::Rect::from_xywh(
    //     rect.origin().x().as_pt(),
    //     rect.origin().y().as_pt(),
    //     rect.size().width().as_pt(),
    //     rect.size().height().as_pt(),
    // ).ok_or(Error::InvalidGeometry)
    todo!()
}

/// Identity. `Err` is unreachable for a `Size` built through `geom`.
pub(super) fn to_krilla_size(size: Size) -> Result<krilla::geom::Size> {
    todo!("krilla::geom::Size::from_wh, both axes already Positive")
}

/// Page box. No crop box, no bleed box, no rotation in v1.
pub(super) fn to_page_settings(size: Size) -> Result<krilla::page::PageSettings> {
    // UNVERIFIED: whether PageSettings::from_wh returns PageSettings or Option.
    todo!("PageSettings::from_wh(width_pt, height_pt)")
}

/// The one conversion that can genuinely fail.
///
/// `Path` guarantees a non-empty verb list starting with `MoveTo`, but a path
/// that is only a `MoveTo` has no drawable segment and krilla's builder
/// rejects it. That is [`Error::InvalidGeometry`].
pub(super) fn to_krilla_path(path: &Path) -> Result<krilla::path::Path> {
    // let mut builder = krilla::path::PathBuilder::new();
    // for verb in path.verbs() {
    //     match verb {
    //         PathVerb::MoveTo(p) => builder.move_to(x, y),
    //         PathVerb::LineTo(p) => builder.line_to(x, y),
    //         PathVerb::Close => builder.close(),
    //     }
    // }
    // builder.finish().ok_or(Error::InvalidGeometry)
    todo!()
}

/// Solid fill, non-zero winding. Alpha 0 still draws, at opacity 0.
pub(super) fn to_fill(paint: &Paint) -> Result<krilla::paint::Fill> {
    // let (r, g, b, a) = paint.color().channels();
    // opacity = NormalizedF32::new(f32::from(a) / 255.0).ok_or(Error::InvalidGeometry)?
    // paint  = krilla::color::rgb::Color::new(r, g, b).into()
    // Fill { paint, opacity, rule: FillRule::NonZero }
    todo!()
}

/// Width only. Cap, join, miter limit and dash stay at krilla's defaults.
pub(super) fn to_stroke(paint: &Paint, stroke: &Stroke) -> Result<krilla::paint::Stroke> {
    // Stroke { paint, width: stroke.width().as_pt(), opacity, ..Stroke::default() }
    todo!()
}

/// Metadata mapping from `quire-design.md`.
///
/// Empty strings are treated as unset, matching krilla's own `is_empty`
/// guards. `creator` is the constant `"quire"`. No creation date, so bytes do
/// not vary with the clock.
pub(super) fn to_metadata(info: &DocumentInfo) -> krilla::metadata::Metadata {
    todo!("title, authors, description, keywords, language, creator")
}

/// Wraps a queued link into the annotation krilla adds after the surface closes.
pub(super) fn to_annotation(link: super::krilla_canvas::Link) -> krilla::annotation::Annotation {
    // UNVERIFIED: exact 0.8.2 path. Expected
    // LinkAnnotation::new(rect, Target::Action(LinkAction::new(uri).into())).into()
    todo!()
}
```

## Public surface delta

### `src/error.rs`

Four variants, and `Error` loses `Copy`. `Clone` and `PartialEq` stay.

```rust
#[derive(Debug, Clone, PartialEq)]   // was: Debug, Clone, Copy, PartialEq
#[non_exhaustive]
pub enum Error {
    // ... existing length variants and EmptyLink unchanged ...

    /// A shape could not be expressed in the PDF backend.
    ///
    /// Reachable today only for a path whose single verb is `MoveTo`.
    InvalidGeometry,

    /// A `GlyphRun` named a font that is not loaded.
    ///
    /// Every `FontId` is unknown until the font knife lands.
    UnknownFont { id: u32 },

    /// A `DecodedImage` named an image that is not loaded.
    ///
    /// Every `ImageId` is unknown until the image knife lands.
    UnknownImage { id: u32 },

    /// The PDF backend refused to serialize. Carries krilla's `Display` text.
    ///
    /// krilla's own error type never appears in this crate's public API.
    Backend(String),
}
```

Display strings, one per variant, asserted by tests:

- `InvalidGeometry` -> `"geometry cannot be drawn"`
- `UnknownFont { id }` -> `"font {id} is not loaded"`
- `UnknownImage { id }` -> `"image {id} is not loaded"`
- `Backend(message)` -> `"PDF backend failed: {message}"`

Dropping `Copy` compiles against the existing call sites. I checked the two
patterns `canvas.rs` uses (`matches!` on a moved-from-nothing place, then
`error.to_string()`) against a standalone non-`Copy` enum with `rustc` this
session, and both compile. Nothing in `geom.rs`, `canvas.rs` or `lib.rs` stores
an `Error` in a `Copy` type.

`InvalidPageGeometry` from the design draft is **not** added. The draft split it
from `InvalidGeometry` because page sizes came from raw `f32`. `Size` is
`Positive` on both axes now, so the page-size branch is unreachable and does not
deserve its own variant.

### `Cargo.toml`

```toml
[features]
default = []
backend-krilla = ["dep:krilla"]

[dependencies]
krilla = { version = "=0.8.2", optional = true, default-features = false, features = ["raster-images"] }
```

`default` stays `[]` this knife. The design doc flips it to
`["backend-krilla"]`, but with no public entry point reaching the backend yet,
defaulting it on buys compile time and nothing else. Flip it in the PR that adds
`Document::to_pdf`. Flagged in RATIONALE as a reversible call.

No new `dev-dependencies`. Backend tests are unit tests inside `src/backend/`
because `KrillaCanvas` is `pub(crate)`, which is also where AGENTS.md wants the
content-stream experiments to live.

### `src/lib.rs`

Unchanged. `backend` stays `pub(crate)` and feature-gated. No new public type,
no krilla type re-exported, no new `pub use`.

## Tests

All under `#[cfg(all(test, feature = "backend-krilla"))]` in the two backend
modules.

`convert.rs`:

1. `identity_keeps_the_numbers` — `to_krilla_point((3, 4))` has components 3 and
   4; `to_krilla_rect((10, 30, 80, 44))` keeps all four; `to_krilla_size(A4)`
   matches `A4.width().as_pt()`. The design draft's "non-positive side returns
   `None`" test is unwritable now, because `Size` cannot hold one.
2. `metadata_drops_empty_strings` — empty title, empty author, and a keyword
   list containing `""` produce a `Metadata` that leaves them unset.

`krilla_canvas.rs`:

3. `finish_starts_with_the_pdf_header` — bytes begin `%PDF`. Nothing else about
   the bytes is asserted, per AGENTS.md.
4. `lone_move_to_is_invalid_geometry` — `Path::begin(p).finish()` on
   `stroke_path` returns `Err(Error::InvalidGeometry)`, and the canvas still
   finishes afterwards.
5. `glyphs_and_images_are_refused_without_panicking` — `draw_glyph_run` returns
   `Err(Error::UnknownFont { id })`, `draw_image` returns
   `Err(Error::UnknownImage { id })`, and `finish` still yields a `%PDF`.
6. `stroked_path_appears_in_the_content_stream` — the `quire-design.md`
   experiment: a 200x200 page, `(10,20) -> (40,20) -> (40,50)`, stroke only,
   `experiment_settings()`. Asserts the operator numbers **observed during
   implementation**, with a comment stating they are post-`page_root_transform`
   and that a mismatch with `(10, 20)` is not a reason to add a flip to
   `to_krilla_point`.
7. `link_annotation_rect_matches_the_experiment` — public rect
   `(10, 30, 80, 44)`, assert the observed `/Rect`. Same comment.
8. `metadata_set_after_the_first_page_still_lands` — `set_document_info` called
   *after* `finish_page`; the title still appears in the uncompressed bytes.
   This is the test that proves the plan hoists the ordering rule.
9. `pages_agree_with_the_recording_canvas` — one command script, including ops
   outside any page and a trailing unclosed page, driven through both
   `RecordingCanvas` and `KrillaCanvas`. Assert both against the same **literal**
   expectation (two pages, sizes `A4` then `LETTER`), not just against each
   other, so the test still fails when both implementations drift the same way.
   This pins the interchangeability rule that lets `KrillaCanvas` drop ops
   silently instead of growing an error.

The lever, per build-the-lever:

10. `#[test] #[ignore] fn dump_experiment_pdf()` — prints the uncompressed bytes
    of the tests 6 and 7 document. Run once with `--ignored --nocapture` to read
    off the observed numbers, and rerun after any krilla version bump. This is
    the artifact a reviewer replays; the pinned numbers are not folklore.
