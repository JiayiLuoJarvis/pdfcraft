# Candidate 2: Canvas as an owned command sink

## Caller usage first

The public drawing boundary has one operation: submit one owned command. Layout does not coordinate a family of forwarding methods, and a backend has one exhaustive match that defines the whole protocol.

```rust
use quire_pdf::{
    Canvas, CanvasCommand, Color, Paint, Path, Point, RecordingCanvas, A4,
};

fn draw_rule(canvas: &mut dyn Canvas) -> quire_pdf::Result<()> {
    let from = Point::try_pt(36.0, 72.0)?;
    let to = Point::try_pt(559.0, 72.0)?;

    canvas.execute(CanvasCommand::StartPage(A4))?;
    canvas.execute(CanvasCommand::StrokePath {
        path: Path::line(from, to),
        paint: Paint::solid(Color::rgb(32, 32, 32)),
        stroke: quire_pdf::Stroke::new(quire_pdf::Positive::pt(0.75)?),
    })?;
    canvas.execute(CanvasCommand::FinishPage)
}
```

The paginator emits metadata, page boundaries, glyphs, and annotations through the same protocol. Resource-producing code inside the crate supplies opaque font and image handles; application code cannot forge them.

```rust
fn paint_page(
    canvas: &mut dyn Canvas,
    info: quire_pdf::DocumentInfo,
    run: quire_pdf::GlyphRun,
    link_rect: quire_pdf::Rect,
) -> quire_pdf::Result<()> {
    canvas.execute(CanvasCommand::SetDocumentInfo(info))?;
    canvas.execute(CanvasCommand::StartPage(A4))?;
    canvas.execute(CanvasCommand::DrawGlyphRun(run))?;
    canvas.execute(CanvasCommand::AnnotateLink {
        rect: link_rect,
        uri: "https://example.com/invoice/42".into(),
    })?;
    canvas.execute(CanvasCommand::FinishPage)
}
```

Tests inspect the exact accepted stream. They match variants for order and use an absolute `0.01 pt` comparison for coordinates rather than comparing encoded PDF bytes.

```rust
let mut canvas = RecordingCanvas::new();
paint_page(&mut canvas, info.clone(), run, link_rect)?;

assert!(matches!(
    canvas.commands(),
    [
        CanvasCommand::SetDocumentInfo(actual),
        CanvasCommand::StartPage(_),
        CanvasCommand::DrawGlyphRun(_),
        CanvasCommand::AnnotateLink { .. },
        CanvasCommand::FinishPage,
    ] if actual == &info
));

let CanvasCommand::AnnotateLink { rect, .. } = &canvas.commands()[3] else {
    unreachable!()
};
assert!((rect.origin().x().as_pt() - 36.0).abs() <= 0.01);
```

A backend implements one exhaustive interpreter. The trait has no document-finalization method; the private krilla adapter retains its own consuming `finish`.

```rust
impl Canvas for MyCanvas {
    fn execute(&mut self, command: CanvasCommand) -> quire_pdf::Result<()> {
        match command {
            CanvasCommand::SetDocumentInfo(info) => self.set_info(info),
            CanvasCommand::StartPage(size) => self.start_page(size),
            CanvasCommand::FinishPage => self.finish_page(),
            CanvasCommand::FillPath { path, paint } => self.fill(path, paint),
            CanvasCommand::StrokePath { path, paint, stroke } => {
                self.stroke(path, paint, stroke)
            }
            CanvasCommand::DrawImage { image, rect } => self.image(image, rect),
            CanvasCommand::DrawGlyphRun(run) => self.glyphs(run),
            CanvasCommand::AnnotateLink { rect, uri } => self.link(rect, uri),
        }
    }
}
```

## Module map

```text
src/
  canvas.rs    Canvas, CanvasCommand, RecordingCanvas,
               GlyphRun, GlyphPlacement, ImageId, DecodedImage
  document.rs  DocumentInfo only; no document builder or flow node
  fonts.rs     FontId only; no parser, store, or measurement yet
  error.rs     add EmptyLink only
  geom.rs      unchanged; all page geometry and paint are reused
  lib.rs       re-export the public canvas payload and handle types
```

`canvas.rs` owns the drawing protocol. `document.rs` owns metadata because it is document data rather than a rendering representation. `fonts.rs` owns the opaque font handle because the later font table is its sole allocator. No layout or backend type crosses this boundary.

## Type sketch

```rust
// src/document.rs

/// PDF metadata copied into a canvas before the first page.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DocumentInfo {
    pub title: Option<String>,
    pub author: Option<String>,
    pub subject: Option<String>,
    pub keywords: Vec<String>,
    pub language: Option<String>,
}
```

```rust
// src/fonts.rs

use std::num::NonZeroU32;

/// Opaque identity allocated by this document's font table.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FontId(NonZeroU32);

impl FontId {
    pub(crate) const fn from_raw(raw: NonZeroU32) -> Self {
        Self(raw)
    }

    /// Stable numeric identity for diagnostics and canvas implementations.
    pub const fn get(self) -> u32 {
        self.0.get()
    }
}
```

```rust
// src/canvas.rs

use std::{num::NonZeroU32, ops::Range};

use crate::{
    document::DocumentInfo,
    error::{Error, Result},
    fonts::FontId,
    geom::{Color, Length, Paint, Path, Point, Positive, Rect, Size, Stroke},
};

/// One complete operation in the page-painting protocol.
///
/// Commands are owned so implementations can consume, queue, or record them
/// without cloning caller-owned strings, paths, glyph arrays, or images.
#[derive(Debug, Clone, PartialEq)]
pub enum CanvasCommand {
    SetDocumentInfo(DocumentInfo),
    StartPage(Size),
    FinishPage,
    FillPath {
        path: Path,
        paint: Paint,
    },
    StrokePath {
        path: Path,
        paint: Paint,
        stroke: Stroke,
    },
    DrawImage {
        image: DecodedImage,
        rect: Rect,
    },
    DrawGlyphRun(GlyphRun),
    AnnotateLink {
        rect: Rect,
        uri: Box<str>,
    },
}

/// Backend-neutral sink for already-positioned drawing operations.
///
/// Producers issue `SetDocumentInfo`, then balanced `StartPage` /
/// `FinishPage` regions. Drawing and annotation commands occur only inside a
/// page. Implementations may reject backend failures; empty link URIs are
/// rejected by every implementation before accepting that command.
pub trait Canvas {
    fn execute(&mut self, command: CanvasCommand) -> Result<()>;
}

/// Opaque identity allocated by the crate's image decoder/registry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ImageId(NonZeroU32);

impl ImageId {
    pub(crate) const fn from_raw(raw: NonZeroU32) -> Self {
        Self(raw)
    }

    pub const fn get(self) -> u32 {
        self.0.get()
    }
}

/// A decoded raster resource known to the active canvas.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DecodedImage {
    id: ImageId,
    pixel_width: NonZeroU32,
    pixel_height: NonZeroU32,
}

impl DecodedImage {
    pub(crate) const fn new(
        id: ImageId,
        pixel_width: NonZeroU32,
        pixel_height: NonZeroU32,
    ) -> Self {
        Self { id, pixel_width, pixel_height }
    }

    pub const fn id(self) -> ImageId { self.id }
    pub const fn pixel_width(self) -> u32 { self.pixel_width.get() }
    pub const fn pixel_height(self) -> u32 { self.pixel_height.get() }
}

/// One positioned glyph. `advance_norm` is `font_units / units_per_em`.
#[derive(Debug, Clone, PartialEq)]
pub struct GlyphPlacement {
    glyph_id: u32,
    advance_norm: f32,
    byte_range: Range<usize>,
}

impl GlyphPlacement {
    /// Called only by the measurement pipeline after its boundary validation.
    pub(crate) fn new(
        glyph_id: u32,
        advance_norm: f32,
        byte_range: Range<usize>,
    ) -> Self {
        Self { glyph_id, advance_norm, byte_range }
    }

    pub const fn glyph_id(&self) -> u32 { self.glyph_id }
    pub const fn advance_norm(&self) -> f32 { self.advance_norm }
    pub fn byte_range(&self) -> Range<usize> { self.byte_range.clone() }
}

/// An immutable, already-measured run positioned at a baseline origin.
#[derive(Debug, Clone, PartialEq)]
pub struct GlyphRun {
    font: FontId,
    font_size: Length<Positive>,
    color: Color,
    origin: Point,
    glyphs: Box<[GlyphPlacement]>,
    text: Box<str>,
}

impl GlyphRun {
    /// Called by the measurement pipeline after glyph/range validation.
    pub(crate) fn new(
        font: FontId,
        font_size: Length<Positive>,
        color: Color,
        origin: Point,
        glyphs: Box<[GlyphPlacement]>,
        text: Box<str>,
    ) -> Self {
        Self { font, font_size, color, origin, glyphs, text }
    }

    pub const fn font(&self) -> FontId { self.font }
    pub const fn font_size(&self) -> Length<Positive> { self.font_size }
    pub const fn color(&self) -> Color { self.color }
    pub const fn origin(&self) -> Point { self.origin }
    pub fn glyphs(&self) -> &[GlyphPlacement] { &self.glyphs }
    pub fn text(&self) -> &str { &self.text }
}

/// In-memory canvas used by layout tests.
#[derive(Debug, Default)]
pub struct RecordingCanvas {
    commands: Vec<CanvasCommand>,
}

impl RecordingCanvas {
    pub const fn new() -> Self {
        Self { commands: Vec::new() }
    }

    pub fn commands(&self) -> &[CanvasCommand] {
        &self.commands
    }

    pub fn into_commands(self) -> Vec<CanvasCommand> {
        self.commands
    }
}

impl Canvas for RecordingCanvas {
    fn execute(&mut self, command: CanvasCommand) -> Result<()> {
        if matches!(
            &command,
            CanvasCommand::AnnotateLink { uri, .. } if uri.is_empty()
        ) {
            return Err(Error::EmptyLink);
        }

        self.commands.push(command);
        Ok(())
    }
}
```

```rust
// src/error.rs addition

#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum Error {
    // existing length errors ...
    EmptyLink,
}

// Display arm:
Error::EmptyLink => f.write_str("link URI is empty"),
```

```rust
// src/lib.rs additions

pub use canvas::{
    Canvas, CanvasCommand, DecodedImage, GlyphPlacement, GlyphRun, ImageId,
    RecordingCanvas,
};
pub use document::DocumentInfo;
pub use fonts::FontId;
```

## Dominant access paths

1. **Layout to output:** layout creates one positioned payload, moves it into `CanvasCommand`, and calls `Canvas::execute`; there is no intermediate adapter or mirrored command representation.
2. **Test assertion:** `RecordingCanvas::execute` validates the only recording-time failure (`EmptyLink`) and pushes the same command; tests borrow the slice and match commands in order.
3. **Backend rendering:** the private adapter exhaustively matches `CanvasCommand`; geometry is already branded and glyph/image resources are resolved by opaque IDs in adapter-private maps.
4. **Glyph fidelity:** measurement constructs `GlyphRun` once; its normalized advances, byte ranges, baseline origin, and positive point size travel unchanged to the backend.

## Test sketch

```rust
#[test]
fn records_metadata_page_paint_link_and_finish_in_order() {
    // Build crate-private FontId/GlyphRun fixtures.
    // Execute five commands.
    // Match the exact five-variant slice.
    // Compare page size, baseline x/y, and link rect coordinates with:
    //     assert!((actual - expected).abs() <= 0.01);
}

#[test]
fn rejected_empty_link_is_not_recorded() {
    // Execute StartPage, then an AnnotateLink with "".
    // Assert Error::EmptyLink and that only StartPage is present.
}

#[test]
fn command_stream_preserves_two_glyph_advances_without_rescaling() {
    // Record a run with deliberately distinct normalized advances.
    // Assert the stored f32 bit patterns and baseline coordinates.
}
```
