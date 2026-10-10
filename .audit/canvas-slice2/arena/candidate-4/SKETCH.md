# Candidate 4 — Canvas slice 2 type sketch

## Module map

```text
src/
  document.rs     DocumentInfo (data bag only; no document DSL)
  fonts.rs        FontId (opaque handle; test constructor)
  error.rs        + EmptyLink (the only new error variant required)
  canvas.rs       Canvas, PageCanvas, RecordingCanvas, RecordingPageCanvas,
                  RecordedPage, PaintOp, CanvasCommand, GlyphRun,
                  GlyphPlacement, ImageId, DecodedImage, test utilities
  lib.rs          re-exports for Canvas, PageCanvas, RecordingCanvas,
                  DocumentInfo, FontId, etc.
```

---

## Usage (spec — written before types below)

### Quickstart / README

Layout and tests draw through the `Canvas` hierarchy. Production will use the sealed `krilla` backend; tests use `RecordingCanvas`.

Document metadata is set on `Canvas`. Page drawing is strictly scoped within `canvas.page(size, |page| { ... })`, which guarantees balanced surface lifecycle and automatic link flushing.

```rust
use quire_pdf::canvas::{Canvas, RecordingCanvas};
use quire_pdf::document::DocumentInfo;
use quire_pdf::geom::{A4, Color, Paint, Path, Point, Stroke};
use quire_pdf::Positive;

let mut canvas = RecordingCanvas::new();
canvas.set_document_info(&DocumentInfo {
    title: Some("Invoice #1001".into()),
    ..Default::default()
})?;

canvas.page(A4, |page| {
    let from = Point::try_pt(50.0, 50.0)?;
    let to = Point::try_pt(500.0, 50.0)?;
    page.stroke_path(
        &Path::line(from, to),
        &Paint::solid(Color::rgb(0, 0, 0)),
        &Stroke::new(Positive::pt(1.0)?),
    )?;
    page.annotate_link(
        quire_pdf::Rect::try_pt(50.0, 50.0, 450.0, 10.0)?,
        "https://example.com/invoice/1001",
    )?;
    Ok(())
})?;

// Test assertions can inspect by structured page...
assert_eq!(canvas.pages().len(), 1);
assert_eq!(canvas.pages()[0].size(), A4);
assert_eq!(canvas.pages()[0].ops().len(), 2);

// ...or by full chronological command stream:
assert_eq!(canvas.commands().len(), 4);
```

### Call site 1 — Paginator loop (`layout::paginate`)

The paginator owns the document flow. It sets metadata once, loops through planned pages, and passes `&mut dyn PageCanvas` into page fragment renderers.

```rust
use quire_pdf::canvas::{Canvas, PageCanvas};
use quire_pdf::geom::Size;
use quire_pdf::Result;

pub struct LayoutPage {
    pub size: Size,
    pub fragments: Vec<Fragment>,
}

pub enum Fragment {
    Glyphs(quire_pdf::canvas::GlyphRun),
    Image {
        decoded: quire_pdf::canvas::DecodedImage,
        rect: quire_pdf::Rect,
    },
    Link {
        rect: quire_pdf::Rect,
        uri: String,
    },
}

pub fn render_document(
    canvas: &mut dyn Canvas,
    info: &quire_pdf::DocumentInfo,
    pages: &[LayoutPage],
) -> Result<()> {
    canvas.set_document_info(info)?;
    for p in pages {
        canvas.with_page(p.size, &mut |surface| {
            for frag in &p.fragments {
                match frag {
                    Fragment::Glyphs(run) => surface.draw_glyph_run(run)?,
                    Fragment::Image { decoded, rect } => surface.draw_image(decoded, *rect)?,
                    Fragment::Link { rect, uri } => surface.annotate_link(*rect, uri)?,
                }
            }
            Ok(())
        })?;
    }
    Ok(())
}
```

### Call site 2 — Sub-layout component isolation (Paragraph layout test)

Leaf layout code (paragraphs, table cells, headers, footers) accepts `&mut dyn PageCanvas`. It has **no access** to `start_page`, `finish_page`, or document metadata, preventing accidental page lifecycle bugs. Unit tests can test leaf rendering on a standalone `RecordingPageCanvas` without instantiating an entire document.

```rust
use quire_pdf::canvas::{GlyphPlacement, GlyphRun, PageCanvas, PaintOp, RecordingPageCanvas};
use quire_pdf::fonts::FontId;
use quire_pdf::geom::{A4, Color, Point};
use quire_pdf::Positive;

fn render_heading(surface: &mut dyn PageCanvas, font: FontId) -> quire_pdf::Result<()> {
    let run = GlyphRun {
        font,
        font_size: Positive::pt(18.0)?,
        color: Color::rgb(17, 24, 39),
        origin: Point::try_pt(72.0, 100.0)?,
        glyphs: vec![GlyphPlacement {
            glyph_id: 10,
            advance_norm: 0.55,
            byte_range: 0..4,
        }],
        text: "Quire".into(),
    };
    surface.draw_glyph_run(&run)
}

#[test]
fn test_heading_baseline_isolated() -> quire_pdf::Result<()> {
    let mut page = RecordingPageCanvas::new(A4);
    render_heading(&mut page, FontId::for_test(1))?;

    assert_eq!(page.ops().len(), 1);
    let PaintOp::DrawGlyphRun(run) = &page.ops()[0] else {
        panic!("expected glyph run");
    };
    assert!((run.origin.y().as_pt() - 100.0).abs() <= 0.01);
    assert!((run.origin.x().as_pt() - 72.0).abs() <= 0.01);
    Ok(())
}
```

### Call site 3 — Multi-page assertions and 0.01 pt tolerance in tests

```rust
use quire_pdf::canvas::{Canvas, PaintOp, RecordingCanvas};
use quire_pdf::canvas::test_util::assert_point_near;
use quire_pdf::geom::{Point, A4, LETTER};

let mut canvas = RecordingCanvas::new();
canvas.page(A4, |p| {
    p.annotate_link(quire_pdf::Rect::try_pt(10.0, 20.0, 100.0, 15.0)?, "https://example.com")
})?;
canvas.page(LETTER, |_| Ok(()))?;

// Assert across pages without index collisions
assert_eq!(canvas.pages().len(), 2);
assert_eq!(canvas.pages()[0].size(), A4);
assert_eq!(canvas.pages()[1].size(), LETTER);

let PaintOp::AnnotateLink { rect, uri } = &canvas.pages()[0].ops()[0] else {
    panic!("expected link");
};
assert_eq!(uri, "https://example.com");
assert_point_near(rect.origin(), Point::try_pt(10.0, 20.0)?, 0.01);
```

---

## Types and Signatures

### `src/error.rs`

Minimal extension: exactly one new error variant as prescribed by `how.md`.

```rust
// In src/error.rs:

#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum Error {
    /// `value` is NaN or infinite.
    NonFiniteLength { value: f32 },
    /// `value` is finite and below zero.
    NegativeLength { value: f32 },
    /// `value` is finite and not above zero.
    NonPositiveLength { value: f32 },
    /// A link annotation URI was empty.
    EmptyLink,
}

// Display format:
// Self::EmptyLink => write!(f, "link annotation URI cannot be empty")
```

### `src/document.rs`

Metadata storage bag for document creation. No document builder or DSL code here.

```rust
//! Document-level metadata and configuration.

/// Metadata stored in the PDF document dictionary.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DocumentInfo {
    /// Document title.
    pub title: Option<String>,
    /// Document author.
    pub author: Option<String>,
    /// Document subject or description.
    pub subject: Option<String>,
    /// Keywords for indexing.
    pub keywords: Option<String>,
    /// Application that generated the document.
    pub creator: Option<String>,
    /// PDF producer library name.
    pub producer: Option<String>,
}
```

### `src/fonts.rs`

Opaque newtype representing a registered font handle.

```rust
//! Font identifiers and metrics placeholder.

/// An opaque handle for a font registered with the layout engine.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FontId(pub(crate) u32);

impl FontId {
    /// Constructs a font ID from a raw numeric identifier.
    #[must_use]
    pub const fn from_raw(id: u32) -> Self {
        Self(id)
    }

    /// Constructs a font ID for testing purposes.
    #[must_use]
    pub const fn for_test(id: u32) -> Self {
        Self::from_raw(id)
    }

    /// Returns the underlying raw ID.
    #[must_use]
    pub const fn as_u32(self) -> u32 {
        self.0
    }
}
```

### `src/canvas.rs`

Core canvas drawing abstractions, payload types, and recorders.

```rust
//! Drawing surface abstractions and recording implementations.
//!
//! Layout talks only to [`Canvas`] (for document lifecycle) and [`PageCanvas`]
//! (for 2D page drawing).
//! `krilla` types must never appear in this module.

use std::ops::Range;

use crate::document::DocumentInfo;
use crate::error::{Error, Result};
use crate::fonts::FontId;
use crate::geom::{Color, Length, Paint, Path, Point, Positive, Rect, Size, Stroke};

// ---------------------------------------------------------------------------
// Identifiers and Payloads
// ---------------------------------------------------------------------------

/// An opaque identifier for a raster image registered with the engine.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ImageId(pub(crate) u32);

impl ImageId {
    /// Constructs an image ID from a raw numeric identifier.
    #[must_use]
    pub const fn from_raw(id: u32) -> Self {
        Self(id)
    }

    /// Constructs an image ID for testing purposes.
    #[must_use]
    pub const fn for_test(id: u32) -> Self {
        Self::from_raw(id)
    }

    /// Returns the underlying raw ID.
    #[must_use]
    pub const fn as_u32(self) -> u32 {
        self.0
    }
}

/// A decoded raster image ready for rendering.
///
/// In the recording canvas, this carries dimensions and an opaque handle.
/// The sealed backend translates this into an embedded PDF image XObject.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecodedImage {
    /// Opaque handle.
    pub id: ImageId,
    /// Width in pixels.
    pub pixel_width: u32,
    /// Height in pixels.
    pub pixel_height: u32,
}

/// Placement and metric data for a single positioned glyph.
#[derive(Debug, Clone, PartialEq)]
pub struct GlyphPlacement {
    /// Font glyph index.
    pub glyph_id: u32,
    /// Normalized horizontal advance: `font_units / units_per_em`.
    ///
    /// Dimensionless. The backend multiplies this by font size to advance x.
    pub advance_norm: f32,
    /// Byte range within the source text slice for ToUnicode mapping.
    pub byte_range: Range<usize>,
}

/// A run of positioned glyphs sharing a single font, size, and color.
#[derive(Debug, Clone, PartialEq)]
pub struct GlyphRun {
    /// The font used to render this run.
    pub font: FontId,
    /// Font size in PDF points.
    pub font_size: Length<Positive>,
    /// Glyph fill color.
    pub color: Color,
    /// Baseline origin for the first glyph in page coordinates (y downward).
    pub origin: Point,
    /// Ordered glyph placements.
    pub glyphs: Vec<GlyphPlacement>,
    /// Full UTF-8 text for the run, used by PDF viewers for selection/copying.
    pub text: String,
}

// ---------------------------------------------------------------------------
// Drawing Operations & Event Log
// ---------------------------------------------------------------------------

/// An individual paint or annotation operation occurring on a page.
#[derive(Debug, Clone, PartialEq)]
pub enum PaintOp {
    /// Fills a closed or open path.
    FillPath {
        path: Path,
        paint: Paint,
    },
    /// Strokes a path with the specified stroke width.
    StrokePath {
        path: Path,
        paint: Paint,
        stroke: Stroke,
    },
    /// Draws a raster image scaled into the target rectangle.
    DrawImage {
        image: DecodedImage,
        rect: Rect,
    },
    /// Renders a positioned run of glyphs.
    DrawGlyphRun(GlyphRun),
    /// Registers a clickable link annotation covering `rect`.
    AnnotateLink {
        rect: Rect,
        uri: String,
    },
}

/// A linear command recorded by [`RecordingCanvas`] across the entire document.
///
/// This enum captures the exact chronological order of all lifecycle and drawing calls.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum CanvasCommand {
    /// Document metadata was set.
    SetDocumentInfo(DocumentInfo),
    /// A page of the given size was opened.
    StartPage(Size),
    /// A path was filled.
    FillPath {
        path: Path,
        paint: Paint,
    },
    /// A path was stroked.
    StrokePath {
        path: Path,
        paint: Paint,
        stroke: Stroke,
    },
    /// An image was drawn.
    DrawImage {
        image: DecodedImage,
        rect: Rect,
    },
    /// A glyph run was rendered.
    DrawGlyphRun(GlyphRun),
    /// A link annotation was added.
    AnnotateLink {
        rect: Rect,
        uri: String,
    },
    /// The current page was committed.
    FinishPage,
}

impl From<PaintOp> for CanvasCommand {
    fn from(op: PaintOp) -> Self {
        match op {
            PaintOp::FillPath { path, paint } => Self::FillPath { path, paint },
            PaintOp::StrokePath { path, paint, stroke } => Self::StrokePath { path, paint, stroke },
            PaintOp::DrawImage { image, rect } => Self::DrawImage { image, rect },
            PaintOp::DrawGlyphRun(run) => Self::DrawGlyphRun(run),
            PaintOp::AnnotateLink { rect, uri } => Self::AnnotateLink { rect, uri },
        }
    }
}

// ---------------------------------------------------------------------------
// Traits
// ---------------------------------------------------------------------------

/// 2D vector drawing operations inside an active page.
///
/// Implementations may buffer operations or paint directly to a backend surface.
/// This trait is strictly scoped to a single page; it cannot create or finish pages.
pub trait PageCanvas {
    /// Fills `path` with `paint`.
    fn fill_path(&mut self, path: &Path, paint: &Paint) -> Result<()>;

    /// Strokes `path` with `paint` and `stroke` width.
    fn stroke_path(&mut self, path: &Path, paint: &Paint, stroke: &Stroke) -> Result<()>;

    /// Draws `image` scaled to fill `rect`.
    fn draw_image(&mut self, image: &DecodedImage, rect: Rect) -> Result<()>;

    /// Draws a run of positioned glyphs.
    fn draw_glyph_run(&mut self, run: &GlyphRun) -> Result<()>;

    /// Registers a clickable link annotation covering `rect`.
    ///
    /// Rejects empty or whitespace-only URIs with [`Error::EmptyLink`].
    fn annotate_link(&mut self, rect: Rect, uri: &str) -> Result<()>;
}

/// Document-level canvas managing document metadata and scoped page rendering.
///
/// This trait is 100% object-safe. Callers can pass `&mut dyn Canvas`.
pub trait Canvas {
    /// Sets document metadata dictionary.
    fn set_document_info(&mut self, info: &DocumentInfo) -> Result<()>;

    /// Executes `render` inside an open page of `size`.
    ///
    /// Implementations guarantee that the page surface is initialized before `render`
    /// runs, and that surface tear-down and link annotation flushing occur when
    /// `render` completes.
    fn with_page(
        &mut self,
        size: Size,
        render: &mut dyn FnMut(&mut dyn PageCanvas) -> Result<()>,
    ) -> Result<()>;

    /// Convenience wrapper for closures when `Self: Sized`.
    fn page<F>(&mut self, size: Size, mut f: F) -> Result<()>
    where
        Self: Sized,
        F: FnMut(&mut dyn PageCanvas) -> Result<()>,
    {
        self.with_page(size, &mut f)
    }
}

// ---------------------------------------------------------------------------
// Recording Implementations
// ---------------------------------------------------------------------------

/// An isolated page recorder used for unit testing leaf layout components.
#[derive(Debug, Clone, PartialEq)]
pub struct RecordingPageCanvas {
    size: Size,
    ops: Vec<PaintOp>,
}

impl RecordingPageCanvas {
    /// Creates a new page canvas with the given page `size`.
    #[must_use]
    pub fn new(size: Size) -> Self {
        Self {
            size,
            ops: Vec::new(),
        }
    }

    /// The page dimensions.
    #[must_use]
    pub const fn size(&self) -> Size {
        self.size
    }

    /// The recorded paint operations in invocation order.
    #[must_use]
    pub fn ops(&self) -> &[PaintOp] {
        &self.ops
    }
}

impl PageCanvas for RecordingPageCanvas {
    fn fill_path(&mut self, path: &Path, paint: &Paint) -> Result<()> {
        self.ops.push(PaintOp::FillPath {
            path: path.clone(),
            paint: *paint,
        });
        Ok(())
    }

    fn stroke_path(&mut self, path: &Path, paint: &Paint, stroke: &Stroke) -> Result<()> {
        self.ops.push(PaintOp::StrokePath {
            path: path.clone(),
            paint: *paint,
            stroke: *stroke,
        });
        Ok(())
    }

    fn draw_image(&mut self, image: &DecodedImage, rect: Rect) -> Result<()> {
        self.ops.push(PaintOp::DrawImage {
            image: image.clone(),
            rect,
        });
        Ok(())
    }

    fn draw_glyph_run(&mut self, run: &GlyphRun) -> Result<()> {
        self.ops.push(PaintOp::DrawGlyphRun(run.clone()));
        Ok(())
    }

    fn annotate_link(&mut self, rect: Rect, uri: &str) -> Result<()> {
        if uri.trim().is_empty() {
            return Err(Error::EmptyLink);
        }
        self.ops.push(PaintOp::AnnotateLink {
            rect,
            uri: uri.to_string(),
        });
        Ok(())
    }
}

/// A completed page record inside [`RecordingCanvas`].
#[derive(Debug, Clone, PartialEq)]
pub struct RecordedPage {
    size: Size,
    ops: Vec<PaintOp>,
}

impl RecordedPage {
    /// The page dimensions.
    #[must_use]
    pub const fn size(&self) -> Size {
        self.size
    }

    /// The paint operations recorded on this page.
    #[must_use]
    pub fn ops(&self) -> &[PaintOp] {
        &self.ops
    }
}

/// Test double that records all canvas operations for assertion in tests.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct RecordingCanvas {
    info: Option<DocumentInfo>,
    pages: Vec<RecordedPage>,
    commands: Vec<CanvasCommand>,
}

impl RecordingCanvas {
    /// Creates an empty recording canvas.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// The recorded document metadata, if set.
    #[must_use]
    pub fn document_info(&self) -> Option<&DocumentInfo> {
        self.info.as_ref()
    }

    /// The committed pages in order.
    #[must_use]
    pub fn pages(&self) -> &[RecordedPage] {
        &self.pages
    }

    /// Flat chronological log of all commands executed on this canvas.
    #[must_use]
    pub fn commands(&self) -> &[CanvasCommand] {
        &self.commands
    }
}

impl Canvas for RecordingCanvas {
    fn set_document_info(&mut self, info: &DocumentInfo) -> Result<()> {
        self.info = Some(info.clone());
        self.commands
            .push(CanvasCommand::SetDocumentInfo(info.clone()));
        Ok(())
    }

    fn with_page(
        &mut self,
        size: Size,
        render: &mut dyn FnMut(&mut dyn PageCanvas) -> Result<()>,
    ) -> Result<()> {
        self.commands.push(CanvasCommand::StartPage(size));

        let mut page_canvas = RecordingPageCanvas::new(size);
        let result = render(&mut page_canvas);

        if let Err(err) = result {
            // Early return does not commit the page to self.pages,
            // preserving clean transactional failure semantics.
            return Err(err);
        }

        // Commit page and flatten its ops into the global chronological command stream.
        for op in &page_canvas.ops {
            self.commands.push(op.clone().into());
        }
        self.commands.push(CanvasCommand::FinishPage);

        self.pages.push(RecordedPage {
            size,
            ops: page_canvas.ops,
        });

        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Test Utilities (Tolerance Matching)
// ---------------------------------------------------------------------------

/// Coordinate comparison utilities for layout tests.
pub mod test_util {
    use super::*;

    /// Returns true if two floating point numbers are within `eps` absolute distance.
    #[must_use]
    pub fn pt_near(a: f32, b: f32, eps: f32) -> bool {
        (a - b).abs() <= eps
    }

    /// Returns true if two points are within `eps` absolute distance on both axes.
    #[must_use]
    pub fn point_near(a: Point, b: Point, eps: f32) -> bool {
        pt_near(a.x().as_pt(), b.x().as_pt(), eps) && pt_near(a.y().as_pt(), b.y().as_pt(), eps)
    }

    /// Returns true if two rectangles match in origin and size within `eps`.
    #[must_use]
    pub fn rect_near(a: Rect, b: Rect, eps: f32) -> bool {
        point_near(a.origin(), b.origin(), eps)
            && pt_near(a.size().width().as_pt(), b.size().width().as_pt(), eps)
            && pt_near(a.size().height().as_pt(), b.size().height().as_pt(), eps)
    }

    /// Asserts that two points are within `eps` absolute tolerance.
    #[track_caller]
    pub fn assert_point_near(actual: Point, expected: Point, eps: f32) {
        assert!(
            point_near(actual, expected, eps),
            "point mismatch: actual ({}, {}), expected ({}, {}), tolerance {}",
            actual.x().as_pt(),
            actual.y().as_pt(),
            expected.x().as_pt(),
            expected.y().as_pt(),
            eps
        );
    }
}
```
