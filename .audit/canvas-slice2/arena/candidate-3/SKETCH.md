# Canvas slice 2 — page guard, nested transcript

Layout paints through a page guard. The guard's borrow is the open page. Sealing appends one `Record::Page` whose ops are exactly the calls made on that guard, in order. Paint methods return `()`. Empty URIs, non-finite advances, and glyph spans that cannot slice the run's text fail in their constructors. `DocumentInfo` and the transcript live in `canvas`. `Canvas` is a generic trait. The transcript is `Vec<Record>`. PDF bytes stay off the trait.

The usage below is the spec. Signatures match it.

## Usage

```rust
use quire_pdf::{
    pt_near, Canvas, Color, DecodedImage, DocumentInfo, FontId, GlyphRun, GlyphRunError, ImageId,
    LinkUri, PageOp, PagePaint, Paint, Path, Point, Positive, Record, RecordingCanvas, Rect, Size,
    Stroke, A4, LETTER,
};

fn draw_sample(canvas: &mut RecordingCanvas) -> quire_pdf::Result<()> {
    canvas.set_document_info(DocumentInfo::new().with_title("Letter"));

    let origin = Point::try_pt(72.0, 96.0)?;
    let run = sample_run(Positive::pt(12.0)?, origin).expect("spans sit on char boundaries");

    let mut page = canvas.start_page(A4);
    let rule = Path::line(origin, Point::try_pt(400.0, 96.0)?);
    page.stroke_path(
        &rule,
        Paint::solid(Color::rgb(0, 0, 0)),
        Stroke::new(Positive::pt(0.75)?),
    );
    let image = DecodedImage::new(ImageId::from_raw(7), 200, 100).expect("nonzero pixels");
    page.draw_image(image, Rect::try_pt(72.0, 140.0, 100.0, 50.0)?);
    page.draw_glyph_run(&run);

    let uri = LinkUri::new("https://example.com/a")?;
    page.annotate_link(Rect::try_pt(72.0, 86.0, 80.0, 14.0)?, &uri);
    page.finish();
    Ok(())
}

fn sample_run(
    size: quire_pdf::Length<Positive>,
    origin: Point,
) -> std::result::Result<GlyphRun, GlyphRunError> {
    let mut run = GlyphRun::build(FontId::from_raw(1), size, Color::rgb(0, 0, 0), origin, "Ab");
    run.place(11, 0.5, 0..1)?;
    run.place(12, 0.55, 1..2)?;
    run.finish()
}

/// Layout already holds trusted `Path` / `Stroke` values. Painting them cannot fail.
fn paint_rule<C: Canvas>(canvas: &mut C, size: Size, path: &Path, stroke: Stroke) {
    let mut page = canvas.start_page(size);
    page.stroke_path(path, Paint::solid(Color::rgb(0, 0, 0)), stroke);
    page.finish();
}

/// One checked URI, two line boxes. `finish` ends the page borrow, so the next
/// page starts in the same scope.
fn link_lines<C: Canvas>(canvas: &mut C, uri: &str) -> quire_pdf::Result<()> {
    let uri = LinkUri::new(uri)?;
    let mut page = canvas.start_page(A4);
    page.annotate_link(Rect::try_pt(72.0, 100.0, 120.0, 14.0)?, &uri);
    page.annotate_link(Rect::try_pt(72.0, 116.0, 40.0, 14.0)?, &uri);
    page.finish();
    let mut page = canvas.start_page(LETTER);
    page.finish();
    Ok(())
}

fn assert_sample_log(canvas: &RecordingCanvas) {
    let records = canvas.records();
    let Record::Info(info) = &records[0] else {
        panic!("info");
    };
    assert_eq!(info.title(), Some("Letter"));

    let Record::Page(page) = &records[1] else {
        panic!("page");
    };
    assert_eq!(page.size(), A4);
    assert!(matches!(page.ops()[0], PageOp::StrokePath { .. }));
    assert!(matches!(page.ops()[1], PageOp::Image { .. }));
    match &page.ops()[2] {
        PageOp::GlyphRun(run) => {
            let placed = &run.placements()[0];
            let user_advance = placed.advance().get() * run.font_size().as_pt();
            assert!(pt_near(run.origin().x().as_pt(), 72.0));
            assert!(pt_near(user_advance, 6.0));
            assert_eq!(placed.advance().get().to_bits(), 0.5f32.to_bits());
        }
        _ => panic!("glyphs"),
    }
}
```

`LinkUri::new("")` is `Err(Error::EmptyLink)` and never reaches `annotate_link`. A `PageOp` is not something a caller constructs and submits.

## Module map

```text
src/error.rs     Error::EmptyLink only
src/fonts.rs     FontId newtype; no measurement, no FontSource
src/canvas.rs    writer traits, value types, RecordingCanvas, unit tests
src/lib.rs       re-exports below
src/document.rs  untouched (no DSL, no DocumentInfo)
src/geom.rs      untouched
src/backend/     untouched
```

One module owns the transcript. No `canvas/command.rs`.

Later adapter, not this slice: `KrillaCanvas` implements `Canvas`. Its page guard draws on the one `surface()` borrow, queues links privately, and on `finish` / `Drop` drops the surface and then writes annotations. `KrillaCanvas::finish(self) -> Result<Vec<u8>, Error>` stays off the trait. Mid-page backend failures latch on that guard and surface from the adapter `finish`, not from paint methods.

Dependency this slice locks in:

```text
document → layout → canvas → fonts
```

`fonts` does not refer to `canvas`. `document` does not define types `canvas` names.

## Public surface

Writer (layout calls these):

- `Canvas`, `PagePaint`
- `RecordingCanvas`
- `DocumentInfo`, `LinkUri`, `DecodedImage`, `ImageId`, `FontId`
- `GlyphRun`, `GlyphRunBuilder`, `GlyphRunError`, `GlyphPlacement`, `AdvanceNorm`

Oracle (tests read these; callers do not build them):

- `Record`, `RecordedPage`, `PageOp`, `RecordingPage`
- `pt_near`, `PT_ABS_TOLERANCE`

Copy values cross the trait by value (`Paint`, `Stroke`, `Rect`, `DecodedImage`). Non-`Copy` borrows cross by shared reference (`Path`, `GlyphRun`, `LinkUri`); `RecordingPage` clones them into the op it owns. `DocumentInfo` moves into the log.

## `src/error.rs`

```rust
/// Failures from constructing lengths, geometry, and link URIs.

pub enum Error {
    NonFiniteLength { value: f32 },
    NegativeLength { value: f32 },
    NonPositiveLength { value: f32 },
    /// `LinkUri::new` rejected an empty string.
    EmptyLink,
}

// Display: "link URI is empty"
```

No `InvalidGeometry`, `Backend`, `UnknownFont`, or glyph-span variants.

## `src/fonts.rs`

```rust
/// Handle stored on a glyph run.
///
/// This slice does not look up a font. Any `u32` is representable, including 0.
/// A later font table may mint ids from 1; that rule belongs to the table.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[must_use]
pub struct FontId(u32);

impl FontId {
    pub const fn from_raw(raw: u32) -> Self {
        Self(raw)
    }

    pub const fn raw(self) -> u32 {
        self.0
    }
}
```

## `src/canvas.rs`

Coordinates are geom's: top-left origin, y down, PDF points. This module does not flip axes.

```rust
use std::num::NonZeroU32;
use std::ops::Range;

use crate::error::{Error, Result};
use crate::fonts::FontId;
use crate::geom::{Color, Length, Paint, Path, Point, Positive, Rect, Size, Stroke};

/// Absolute comparison for recorded point values. Inclusive bound.
pub const PT_ABS_TOLERANCE: f32 = 0.01;

pub fn pt_near(actual: f32, expected: f32) -> bool {
    // TODO: (actual - expected).abs() <= PT_ABS_TOLERANCE
    unimplemented!()
}

pub trait Canvas {
    type Page<'a>: PagePaint
    where
        Self: 'a;

    /// Appends one [`Record::Info`]. Legal only while no page guard is alive.
    fn set_document_info(&mut self, info: DocumentInfo);

    /// Opens a page. The returned guard exclusively borrows `self` until
    /// [`PagePaint::finish`] or drop.
    ///
    /// ```compile_fail
    /// use quire_pdf::{Canvas, DocumentInfo, RecordingCanvas, A4};
    /// let mut canvas = RecordingCanvas::new();
    /// let page = canvas.start_page(A4);
    /// canvas.set_document_info(DocumentInfo::new());
    /// drop(page);
    /// ```
    fn start_page(&mut self, size: Size) -> Self::Page<'_>;
}

pub trait PagePaint {
    fn fill_path(&mut self, path: &Path, paint: Paint);
    fn stroke_path(&mut self, path: &Path, paint: Paint, stroke: Stroke);
    fn draw_image(&mut self, image: DecodedImage, rect: Rect);
    fn draw_glyph_run(&mut self, run: &GlyphRun);
    fn annotate_link(&mut self, rect: Rect, uri: &LinkUri);
    /// Seals the page and ends the borrow of the canvas.
    /// Drop seals too. A second seal is a no-op.
    fn finish(self);
}

/// PDF info dictionary fields this crate records.
///
/// Empty strings are unset. Keywords keep order and drop empty entries.
/// Whitespace-only strings stay. No scheme, language-tag, or length check.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
#[must_use]
pub struct DocumentInfo { /* title, author, subject, keywords, language */ }

impl DocumentInfo {
    pub fn new() -> Self { unimplemented!() }

    /// Empty `title` stores `None` and clears a previous title.
    pub fn with_title(self, title: impl Into<String>) -> Self { unimplemented!() }
    pub fn with_author(self, author: impl Into<String>) -> Self { unimplemented!() }
    pub fn with_subject(self, subject: impl Into<String>) -> Self { unimplemented!() }
    pub fn with_language(self, language: impl Into<String>) -> Self { unimplemented!() }

    /// Empty `keyword` does not append.
    pub fn keyword(self, keyword: impl Into<String>) -> Self { unimplemented!() }

    pub fn title(&self) -> Option<&str> { unimplemented!() }
    pub fn author(&self) -> Option<&str> { unimplemented!() }
    pub fn subject(&self) -> Option<&str> { unimplemented!() }
    pub fn language(&self) -> Option<&str> { unimplemented!() }
    pub fn keywords(&self) -> &[String] { unimplemented!() }
}

/// Non-empty URI bytes. The canvas does not parse or fetch them.
#[derive(Debug, Clone, PartialEq, Eq)]
#[must_use]
pub struct LinkUri(String);

impl LinkUri {
    /// # Errors
    /// [`Error::EmptyLink`] when `uri` is empty.
    pub fn new(uri: impl Into<String>) -> Result<Self> {
        // TODO: reject is_empty(); do not trim.
        unimplemented!()
    }

    pub fn as_str(&self) -> &str { unimplemented!() }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[must_use]
pub struct ImageId(u32);

impl ImageId {
    pub const fn from_raw(raw: u32) -> Self { Self(raw) }
    pub const fn raw(self) -> u32 { self.0 }
}

/// Pixel size of an image the caller already decoded. No bytes, no name map.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[must_use]
pub struct DecodedImage { /* id, NonZeroU32 width, NonZeroU32 height */ }

impl DecodedImage {
    /// `None` when either pixel edge is 0.
    pub fn new(id: ImageId, pixel_width: u32, pixel_height: u32) -> Option<Self> {
        unimplemented!()
    }

    pub fn id(self) -> ImageId { unimplemented!() }
    pub fn pixel_width(self) -> u32 { unimplemented!() }
    pub fn pixel_height(self) -> u32 { unimplemented!() }
}

/// Finite font-unit advance divided by units per em.
///
/// Zero and negative values are representable. The canvas does not multiply
/// by font size. Glyph runs expose this factor and [`Length<Positive>`]
/// separately.
#[derive(Debug, Clone, Copy)]
#[must_use]
pub struct AdvanceNorm(f32);

impl AdvanceNorm {
    pub const fn get(self) -> f32 { self.0 }

    fn new(value: f32) -> Option<Self> {
        // TODO: Some iff value.is_finite(). Private: only GlyphRunBuilder calls it.
        unimplemented!()
    }
}

impl PartialEq for AdvanceNorm {
    fn eq(&self, other: &Self) -> bool {
        // TODO: bit equality. NaN never enters, same reason as `Length`.
        let _ = other;
        unimplemented!()
    }
}

impl Eq for AdvanceNorm {}

#[derive(Debug, Clone, PartialEq, Eq)]
#[must_use]
pub struct GlyphPlacement { /* glyph_id, advance, byte_range */ }

impl GlyphPlacement {
    pub fn glyph_id(&self) -> u32 { unimplemented!() }
    pub fn advance(&self) -> AdvanceNorm { unimplemented!() }
    pub fn byte_range(&self) -> Range<usize> { unimplemented!() }
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[must_use]
pub struct GlyphRun { /* font, font_size, color, origin, text, placements */ }

impl GlyphRun {
    pub fn build(
        font: FontId,
        font_size: Length<Positive>,
        color: Color,
        origin: Point,
        text: impl Into<String>,
    ) -> GlyphRunBuilder {
        unimplemented!()
    }

    pub fn font(&self) -> FontId { unimplemented!() }
    pub fn font_size(&self) -> Length<Positive> { unimplemented!() }
    pub fn color(&self) -> Color { unimplemented!() }
    pub fn origin(&self) -> Point { unimplemented!() }
    pub fn text(&self) -> &str { unimplemented!() }
    pub fn placements(&self) -> &[GlyphPlacement] { unimplemented!() }
}

#[derive(Debug)]
#[must_use]
pub struct GlyphRunBuilder { /* text: String, placements: Vec<GlyphPlacement>, ... */ }

impl GlyphRunBuilder {
    /// Appends one glyph, or leaves the builder unchanged on `Err`.
    ///
    /// `bytes` is a byte range of `text`. It must satisfy `start <= end`,
    /// `end <= text.len()`, and both ends on char boundaries, so a later
    /// adapter can slice `text` without panicking.
    ///
    /// # Errors
    /// [`GlyphRunError::NonFiniteAdvance`], [`GlyphRunError::ByteRange`].
    pub fn place(
        &mut self,
        glyph_id: u32,
        advance_norm: f32,
        bytes: Range<usize>,
    ) -> std::result::Result<(), GlyphRunError> {
        unimplemented!()
    }

    /// # Errors
    /// [`GlyphRunError::Empty`] when no `place` succeeded.
    pub fn finish(self) -> std::result::Result<GlyphRun, GlyphRunError> {
        unimplemented!()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum GlyphRunError {
    Empty,
    NonFiniteAdvance,
    ByteRange {
        index: usize,
        start: usize,
        end: usize,
        text_len: usize,
    },
}

// Display + std::error::Error. Not crate::Error. No From<GlyphRunError> for Error.

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Record {
    Info(DocumentInfo),
    Page(RecordedPage),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordedPage { /* size: Size, ops: Vec<PageOp> */ }

impl RecordedPage {
    pub fn size(&self) -> Size { unimplemented!() }
    pub fn ops(&self) -> &[PageOp] { unimplemented!() }
}

/// One call on a page guard, in call order.
///
/// Links stay where the caller put them. A later adapter drains
/// [`PageOp::Link`] after the content stream; this log does not reorder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PageOp {
    FillPath { path: Path, paint: Paint },
    StrokePath { path: Path, paint: Paint, stroke: Stroke },
    Image { image: DecodedImage, rect: Rect },
    GlyphRun(GlyphRun),
    Link { rect: Rect, uri: LinkUri },
}

#[derive(Debug)]
#[must_use]
pub struct RecordingCanvas {
    records: Vec<Record>,
}

impl RecordingCanvas {
    pub fn new() -> Self {
        Self { records: Vec::new() }
    }

    /// Sealed records only. Unobservable while a [`RecordingPage`] is alive,
    /// because that guard holds the only `&mut self`.
    pub fn records(&self) -> &[Record] {
        &self.records
    }
}

impl Default for RecordingCanvas {
    fn default() -> Self {
        Self::new()
    }
}

impl Canvas for RecordingCanvas {
    type Page<'a> = RecordingPage<'a>;

    fn set_document_info(&mut self, info: DocumentInfo) {
        self.records.push(Record::Info(info));
    }

    fn start_page(&mut self, size: Size) -> RecordingPage<'_> {
        RecordingPage {
            canvas: self,
            size,
            ops: Vec::new(),
            sealed: false,
        }
    }
}

#[derive(Debug)]
#[must_use = "dropping seals the page into the log"]
pub struct RecordingPage<'a> {
    canvas: &'a mut RecordingCanvas,
    size: Size,
    ops: Vec<PageOp>,
    sealed: bool,
}

impl RecordingPage<'_> {
    fn seal(&mut self) {
        // TODO: if sealed { return }; set sealed; mem::take ops;
        // push Record::Page(RecordedPage { size, ops }) onto canvas.records.
        // A second call, including Drop after finish, does not push again.
        unimplemented!()
    }
}

impl PagePaint for RecordingPage<'_> {
    fn fill_path(&mut self, path: &Path, paint: Paint) {
        // TODO: push PageOp::FillPath { path: path.clone(), paint }.
        let _ = (path, paint);
        unimplemented!()
    }

    fn stroke_path(&mut self, path: &Path, paint: Paint, stroke: Stroke) {
        let _ = (path, paint, stroke);
        unimplemented!()
    }

    fn draw_image(&mut self, image: DecodedImage, rect: Rect) {
        let _ = (image, rect);
        unimplemented!()
    }

    fn draw_glyph_run(&mut self, run: &GlyphRun) {
        let _ = run;
        unimplemented!()
    }

    fn annotate_link(&mut self, rect: Rect, uri: &LinkUri) {
        // TODO: push PageOp::Link { rect, uri: uri.clone() }. No emptiness check.
        let _ = (rect, uri);
        unimplemented!()
    }

    fn finish(mut self) {
        self.seal();
    }
}

impl Drop for RecordingPage<'_> {
    fn drop(&mut self) {
        self.seal();
    }
}
```

`AdvanceNorm` implements `PartialEq` and `Eq` by bits, the same way `Length` does, because a derived `Eq` cannot wrap `f32`. Containers derive `Eq` from that impl. `GlyphRunBuilder` returns `std::result::Result<_, GlyphRunError>`, not `crate::Result`, so span failures stay off `Error`.

Dominant access is append on the guard, then index the sealed `records` slice. No id map and no second index. Image-id lookup belongs to a later backend table, not this log.

## `src/lib.rs`

```rust
pub use canvas::{
    pt_near, AdvanceNorm, Canvas, DecodedImage, DocumentInfo, GlyphPlacement, GlyphRun,
    GlyphRunBuilder, GlyphRunError, ImageId, LinkUri, PageOp, PagePaint, Record, RecordedPage,
    RecordingCanvas, RecordingPage, PT_ABS_TOLERANCE,
};
pub use fonts::FontId;
```

Existing geom and error re-exports stay. `document` stays a placeholder module with no re-exports.

## Tests (`src/canvas.rs`)

Tolerances use `pt_near`. Exact `assert_eq!` is for branded values built in the test (`A4`, `Size`) and for bit-identical advances.

1. `transcript_order_is_info_then_page_ops` — `set_document_info`, then one page: fill, stroke, image, glyph run, link. `records()` is `[Info, Page]`. Ops match that order. Page size is `A4`. Link URI equals the input. Rect origin x is `pt_near` 72.
2. `stored_advance_is_not_premultiplied` — placement advance bits are `0.5`; `advance.get() * font_size.as_pt()` is `pt_near` 6.0; origin x is `pt_near` 72.
3. `tolerance_bounds` — `pt_near(0.0, PT_ABS_TOLERANCE)` holds; `pt_near(0.0, PT_ABS_TOLERANCE * 2.0)` does not; constant bits equal `0.01f32`.
4. `empty_uri_is_empty_link` — `LinkUri::new("")` and `LinkUri::new(String::new())` are `Err(Error::EmptyLink)`. `LinkUri::new(" ")` is ok. No page is opened.
5. `document_info_blanks` — `with_title("A").with_title("")` yields `title() == None`. `keyword("a").keyword("").keyword(" ")` yields `["a", " "]`.
6. `zero_pixel_edge_is_none` — `DecodedImage::new(id, 0, 10)` is `None`.
7. `glyph_span_rules` — `finish` on a builder with no successful `place` is `Empty`. `place(..., NAN, 0..1)` is `NonFiniteAdvance` and does not stick. After that error, `place(..., 0.5, 0..1)` then `finish` yields one glyph. Text `"你"` rejects `0..1` and accepts `0..3`.
8. `two_pages_and_info_between` — records are `Info("a")`, page `A4`, `Info("b")`, page `LETTER`.
9. `drop_seals_once` — a guard that only `fill_path`s, dropped at block end, appends one page. `finish` then end-of-scope still appends one page. A start/finish with no draws appends a page whose `ops()` is empty.

The compile-fail doctest on `start_page` is the check that info and drawing cannot interleave.

## Not in this slice

Flow-inline canvas element, font measurement, `FontSource`, image bytes, named image map, clip/transform stack, `dyn Canvas`, `KrillaCanvas`, any `Error` variant besides `EmptyLink`.
