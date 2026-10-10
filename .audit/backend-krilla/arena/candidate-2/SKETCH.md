# Candidate 2 sketch

## Usage

`KrillaCanvas` stays crate-private. The later `Document::to_pdf` path sees the existing `Canvas` trait and one consuming finish method.

```rust
use crate::backend::krilla_canvas::KrillaCanvas;
use crate::canvas::Canvas;

let mut pdf = KrillaCanvas::new();
pdf.set_document_info(&info)?;
pdf.start_page(A4)?;
pdf.fill_path(&background, &white)?;
pdf.stroke_path(&rule, &black, &thin)?;
pdf.annotate_link(link_rect, LinkTarget::uri("https://example.test")?)?;
pdf.finish_page()?;

let bytes = pdf.finish()?;
assert!(bytes.starts_with(b"%PDF"));
```

Pages may have different sizes. The caller still performs one `start_page` and one `finish_page` per page.

```rust
let mut pdf = KrillaCanvas::new();

pdf.start_page(A4)?;
paint_a4(&mut pdf)?;
pdf.finish_page()?;

pdf.start_page(LETTER)?;
paint_letter(&mut pdf)?;
pdf.finish_page()?;

let bytes = pdf.finish()?;
```

This knife rejects font and image drawing. It never panics and never records a partial substitute.

```rust
pdf.start_page(A4)?;

assert!(matches!(
    pdf.draw_glyph_run(&run),
    Err(Error::UnsupportedCanvasOperation {
        operation: "draw_glyph_run"
    })
));

// The first error poisons this PDF build. Ignoring it cannot produce a PDF
// with silently missing text.
assert!(pdf.finish().is_err());
```

Backend tests can request observable content streams without exposing krilla settings to the crate.

```rust
let mut pdf = KrillaCanvas::new_observable_for_test();
pdf.start_page(Size::try_pt(200.0, 200.0)?)?;
pdf.stroke_path(&probe_path, &black, &thin)?;
pdf.annotate_link(
    Rect::try_pt(10.0, 30.0, 80.0, 44.0)?,
    LinkTarget::uri("https://example.test")?,
)?;
pdf.finish_page()?;

let bytes = pdf.finish()?;
let text = String::from_utf8_lossy(&bytes);
// Assert the literal operators and numbers observed from krilla 0.8.2.
```

## Module map

```text
src/backend/
  mod.rs
    Declares the private adapter modules. Exports no public krilla type.

  convert.rs
    Converts branded quire geometry, paths, colors, and strokes to krilla.
    Geometry conversion is an identity mapping in API coordinates.

  krilla_canvas.rs
    Owns the page plans, enforces the Canvas lifecycle, and renders all plans
    through one lexical krilla Document/Page/Surface borrow chain.
```

No new module represents a "load", "validate", or "render" phase. The adapter owns one body of knowledge and keeps the call chain within `krilla_canvas.rs`.

## Core data shape

The adapter records a typed page plan. It does not record `CanvasCommand`.

```rust
pub(crate) struct KrillaCanvas {
    info: Option<DocumentInfo>,
    closed_pages: Vec<PagePlan>,
    open_page: Option<OpenPage>,
    failure: Option<Error>,
    serialization: Serialization,
}

struct OpenPage {
    size: Size,
    marks: Vec<Mark>,
    links: Vec<Link>,
}

struct PagePlan {
    size: Size,
    marks: Box<[Mark]>,
    links: Box<[Link]>,
}

enum Mark {
    Fill {
        path: Path,
        paint: Paint,
    },
    Stroke {
        path: Path,
        paint: Paint,
        stroke: Stroke,
    },
}

struct Link {
    rect: Rect,
    target: LinkTarget,
}

#[derive(Clone, Copy)]
struct Serialization {
    pretty: bool,
    compress_content_streams: bool,
}
```

`closed_pages` can contain only finished pages. `open_page` is the sole mutable page. `PagePlan` separates surface marks from page annotations because krilla requires the surface borrow to end before `Page::add_annotation`.

The split between `marks` and `links` does not lose observable ordering. PDF link annotations do not paint into the content stream. Their order relative to fill and stroke calls has no visual meaning in the current `Canvas` contract.

`Serialization` has two constructors.

```rust
impl Serialization {
    const fn production() -> Self {
        Self {
            pretty: false,
            compress_content_streams: true,
        }
    }

    #[cfg(test)]
    const fn observable() -> Self {
        Self {
            pretty: true,
            compress_content_streams: false,
        }
    }
}
```

Both profiles set `enable_tagging` to `false` when they create krilla `SerializeSettings`. The field is not configurable in this knife.

## Signatures

```rust
impl KrillaCanvas {
    pub(crate) fn new() -> Self;

    #[cfg(test)]
    pub(crate) fn new_observable_for_test() -> Self;

    /// Fails if a page remains open or an earlier Canvas call failed.
    /// Creates the krilla Document only after the page plan is complete.
    pub(crate) fn finish(self) -> Result<Vec<u8>>;

    fn open_page_mut(&mut self, operation: &'static str) -> Result<&mut OpenPage>;
    fn fail<T>(&mut self, error: Error) -> Result<T>;
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

`set_document_info` uses last-write-wins semantics until `finish`. Buffering removes krilla's "metadata before the first page" timing constraint from the `Canvas` caller. The renderer always applies the final metadata before it starts the first krilla page.

`start_page` fails if `open_page` is occupied. `finish_page` moves `OpenPage` into `PagePlan`. Drawing and annotation methods fail unless a page is open.

Every first failure is stored in `failure`. Later calls return that failure. `finish` also returns it. A caller cannot ignore `draw_glyph_run` or `draw_image` and receive a PDF that omits content.

`draw_glyph_run` and `draw_image` first verify that a page is open. They then return `UnsupportedCanvasOperation`. This knife has no font or image registry, so either method would otherwise need a fake result or a krilla type outside the adapter. The font knife and image knife replace these errors with real `Mark` variants.

## Borrow-safe render

`finish` consumes the adapter and replays each closed page with lexical borrows.

```rust
fn render(plan: KrillaCanvas) -> Result<Vec<u8>> {
    // Create krilla::Document with plan.serialization.
    // Apply plan.info as Metadata before the first start_page_with call.

    for page_plan in plan.closed_pages {
        let settings = to_page_settings(page_plan.size)?;
        let mut page = document.start_page_with(settings);

        {
            // Call surface() exactly once for this page.
            let mut surface = page.surface();
            for mark in page_plan.marks {
                draw_mark(&mut surface, mark)?;
            }
        } // Surface drops here. Its mutable Page borrow ends here.

        for link in page_plan.links {
            page.add_annotation(to_annotation(link)?);
        }
        page.finish();
    }

    document
        .finish()
        .map_err(|error| Error::Backend(error.to_string()))
}
```

Neither `KrillaCanvas` nor `PagePlan` stores `krilla::Page<'_>` or `krilla::Surface<'_>`. The compiler sees each borrow end in the loop body. The design needs no self-reference, `unsafe`, `Rc`, or leaked allocation.

`draw_mark` owns krilla's pen policy.

```rust
fn draw_mark(surface: &mut krilla::Surface<'_>, mark: Mark) -> Result<()> {
    match mark {
        Mark::Fill { path, paint } => {
            surface.set_fill(Some(to_fill(paint)?));
            surface.set_stroke(None);
            surface.draw_path(&to_path(&path)?);
        }
        Mark::Stroke {
            path,
            paint,
            stroke,
        } => {
            surface.set_fill(None);
            surface.set_stroke(Some(to_stroke(paint, stroke)?));
            surface.draw_path(&to_path(&path)?);
        }
    }
    Ok(())
}
```

The adapter does not call `Surface::draw_text`. This knife does not call `draw_glyphs` either.

## Conversion boundary

`convert.rs` contains the only geometry and paint conversion functions.

```rust
pub(crate) fn to_point(point: Point) -> krilla::geom::Point;
pub(crate) fn to_size(size: Size) -> Option<krilla::geom::Size>;
pub(crate) fn to_rect(rect: Rect) -> Option<krilla::geom::Rect>;
pub(crate) fn to_path(path: &Path) -> Result<krilla::geom::Path>;
pub(crate) fn to_fill(paint: Paint) -> Result<krilla::paint::Fill>;
pub(crate) fn to_stroke(
    paint: Paint,
    stroke: Stroke,
) -> Result<krilla::paint::Stroke>;
```

The functions read geometry through `x()`, `y()`, `origin()`, `size()`, `width()`, `height()`, and `as_pt()`. They do not use the obsolete public `f32` fields from the design draft.

`to_point`, `to_size`, and `to_rect` copy the values unchanged. They do not flip y. A failed krilla constructor becomes `Error::Backend` because the branded source values are already finite and positive where required. Such a failure reports a backend contract mismatch, not invalid caller geometry.

`to_path` performs an exhaustive match on `PathVerb`. `to_fill` converts RGBA to krilla RGB paint plus normalized opacity. `to_stroke` starts from krilla's default stroke and replaces only paint, width, and opacity.

## Error changes

Only these variants land in this knife.

```rust
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum Error {
    // Existing geometry and link variants stay unchanged.

    InvalidCanvasState {
        operation: &'static str,
        state: &'static str,
    },
    UnsupportedCanvasOperation {
        operation: &'static str,
    },
    Backend(String),
}
```

`Error` stops deriving `Copy` because `Backend(String)` preserves krilla's diagnostic without exposing a krilla type. The crate is unpublished at version `0.0.0`. Keeping `Copy` would discard the only useful information at the external-library boundary.

This knife does not add `InvalidGeometry`, `InvalidPageGeometry`, font, or image decoding errors. Branded geometry already carries the local scalar invariants. Fonts and images do not exist in the adapter yet.

## Cargo and CI

`Cargo.toml` takes the planned exact pin.

```toml
[features]
default = ["backend-krilla"]
backend-krilla = ["dep:krilla"]

[dependencies]
krilla = { version = "=0.8.2", optional = true, default-features = false, features = ["raster-images"] }
```

The default enables the only production backend. `default-features = false` keeps krilla's `simple-text` and `rayon` features off. `raster-images` is enabled now because it is the fixed backend feature set, although `draw_image` remains unsupported in this knife.

CI uses two explicit configurations.

```text
base:
  cargo clippy --all-targets --no-default-features -- -D warnings
  cargo test --no-default-features
  cargo doc --no-deps --no-default-features

backend:
  cargo clippy --all-targets --features backend-krilla -- -D warnings
  cargo test --features backend-krilla
  cargo doc --no-deps --features backend-krilla
```

The existing format step remains single. Explicit feature arguments prevent a later default-feature edit from silently removing either configuration from CI.

## Tests

Backend unit tests stay under `src/backend/`. They do not add a PDF parser.

1. A one-page fill and stroke document starts with `%PDF`.
2. `to_point(Point::try_pt(3.0, 4.0)?)` preserves `3.0` and `4.0`.
3. The observable 200 pt page contains the literal path operators and coordinate numbers first observed from krilla 0.8.2.
4. The observable link test contains the literal `/Annot` rectangle first observed for public rect `(10, 30, 80, 44)`.
5. A fill followed by a stroke shows the expected fill-only and stroke-only operators. This catches leaked pen state.
6. Two page plans with different sizes both survive replay.
7. A nested `start_page`, a draw without an open page, and `finish` with an open page return `InvalidCanvasState`.
8. `draw_glyph_run` and `draw_image` return `UnsupportedCanvasOperation`, poison the build, and never panic.
9. A title set after page planning still appears because metadata is applied before krilla rendering.

The content-stream tests assert only the observed path and annotation fragments. They do not snapshot the complete PDF or infer API coordinates from PDF-space numbers.
