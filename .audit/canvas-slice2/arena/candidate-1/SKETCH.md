# Candidate 1 — Canvas slice 2 type sketch

## Module map

```text
src/
  document.rs     DocumentInfo (data only; no builders)
  fonts.rs        FontId (newtype)
  canvas.rs       Canvas trait, payloads, CanvasCommand, RecordingCanvas, test helpers
  error.rs        + EmptyLink { field: &'static str }  (only canvas validation in this slice)
  lib.rs          re-export canvas::* , DocumentInfo, FontId
```

Everything canvas-related stays in one module until a second file owns a distinct invariant. No `canvas/recording.rs` split in slice 2.

---

## Usage (spec — written before types below)

### README excerpt

Layout and integration tests draw through `Canvas`. Production uses the krilla adapter (later PR); tests use `RecordingCanvas` and inspect an ordered command log.

```rust
use quire_pdf::canvas::{Canvas, CanvasCommand, LinkTarget, RecordingCanvas};
use quire_pdf::document::DocumentInfo;
use quire_pdf::geom::{A4, Color, Paint, Path, Point, Rect, Size, Stroke};
use quire_pdf::Error;

let mut rec = RecordingCanvas::new();
rec.set_document_info(&DocumentInfo::default())?;
rec.start_page(A4)?;
rec.fill_path(
    &Path::line(Point::try_pt(10.0, 20.0)?, Point::try_pt(110.0, 20.0)?),
    &Paint::solid(Color::rgb(0, 0, 0)),
)?;
rec.finish_page()?;

assert_eq!(rec.commands().len(), 4);
```

Compare coordinates with `0.01` pt absolute tolerance via `canvas::test_util::pt_near`.

### Call site A — paginate loop (layout, future)

```rust
fn write_page_body(
    canvas: &mut dyn Canvas,
    size: Size,
    fragments: &[PreparedFragment],
) -> quire_pdf::Result<()> {
    canvas.start_page(size)?;
    for frag in fragments {
        match frag {
            PreparedFragment::Glyphs(run) => canvas.draw_glyph_run(run)?,
            PreparedFragment::Image { decoded, rect } => canvas.draw_image(decoded, *rect)?,
            PreparedFragment::Link { rect, uri } => {
                let target = LinkTarget::uri(uri)?;
                canvas.annotate_link(*rect, target)?;
            }
        }
    }
    canvas.finish_page()
}
```

### Call site B — layout test asserting glyph placement

```rust
#[test]
fn second_glyph_advances_along_baseline() -> quire_pdf::Result<()> {
    use quire_pdf::canvas::{CanvasCommand, GlyphRun, GlyphPlacement, RecordingCanvas};
    use quire_pdf::fonts::FontId;
    use quire_pdf::geom::{Color, Point};

    let font = FontId::for_test(1);
    let run = GlyphRun {
        font,
        font_size_pt: Positive::pt(12.0)?,
        color: Color::rgb(0, 0, 0),
        origin: Point::try_pt(72.0, 100.0)?,
        text: "AB".into(),
        glyphs: vec![
            GlyphPlacement { glyph_id: 1, advance_norm: 0.5, byte_range: 0..1 },
            GlyphPlacement { glyph_id: 2, advance_norm: 0.6, byte_range: 1..2 },
        ],
    };

    let mut rec = RecordingCanvas::new();
    rec.start_page(A4)?;
    rec.draw_glyph_run(&run)?;
    rec.finish_page()?;

    let CanvasCommand::DrawGlyphRun { run: recorded } = &rec.commands()[2] else {
        panic!("expected glyph command");
    };
    assert!(pt_near(
        recorded.origin.x().as_pt(),
        72.0,
        PT_TOLERANCE,
    ));
    // Second glyph x = origin.x + advance_norm * font_size (adapter must not rescale).
    let expected_x = 72.0 + 0.5 * recorded.font_size_pt.as_pt();
    // … assert via command payload, not PDF bytes
    Ok(())
}
```

---

## Public types (`canvas.rs`)

```rust
use crate::document::DocumentInfo;
use crate::error::{Error, Result};
use crate::fonts::FontId;
use crate::geom::{Color, Paint, Path, Point, Positive, Rect, Size, Stroke};

/// Ordered log entry. One variant per `Canvas` call (plus lifecycle).
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum CanvasCommand {
    SetDocumentInfo(DocumentInfo),
    StartPage(Size),
    FinishPage,
    FillPath { path: Path, paint: Paint },
    StrokePath { path: Path, paint: Paint, stroke: Stroke },
    DrawImage { image: DecodedImage, rect: Rect },
    DrawGlyphRun { run: GlyphRun },
    AnnotateLink { rect: Rect, target: LinkTarget },
}

/// External hyperlink target. v1: URI only (提案 §8).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct LinkTarget(String);

impl LinkTarget {
    /// Rejects empty strings at the boundary (`Error::EmptyLink`).
    pub fn uri(s: &str) -> Result<Self> {
        if s.is_empty() {
            return Err(Error::EmptyLink { field: "uri" });
        }
        Ok(Self(s.to_owned()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ImageId(pub(crate) u32);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecodedImage {
    pub id: ImageId,
    pub pixel_width: u32,
    pub pixel_height: u32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct GlyphPlacement {
    pub glyph_id: u32,
    /// Font units advance / units_per_em. Adapter draws without rescaling.
    pub advance_norm: f32,
    pub byte_range: std::ops::Range<usize>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct GlyphRun {
    pub font: FontId,
    pub font_size_pt: Length<Positive>, // use crate::geom::Length<Positive> in real code
    pub color: Color,
    /// Baseline origin of the first glyph; y increases downward.
    pub origin: Point,
    pub glyphs: Vec<GlyphPlacement>,
    /// UTF-8 source; byte_range indices are into this string (ToUnicode).
    pub text: String,
}

pub trait Canvas {
    fn set_document_info(&mut self, info: &DocumentInfo) -> Result<()>;
    fn start_page(&mut self, size: Size) -> Result<()>;
    fn finish_page(&mut self) -> Result<()>;
    fn fill_path(&mut self, path: &Path, paint: &Paint) -> Result<()>;
    fn stroke_path(&mut self, path: &Path, paint: &Paint, stroke: &Stroke) -> Result<()>;
    fn draw_image(&mut self, image: &DecodedImage, rect: Rect) -> Result<()>;
    fn draw_glyph_run(&mut self, run: &GlyphRun) -> Result<()>;
    fn annotate_link(&mut self, rect: Rect, target: LinkTarget) -> Result<()>;
}

pub struct RecordingCanvas {
    log: Vec<CanvasCommand>,
    /// Runtime guard: `InsidePage` while a page is open.
    phase: RecordingPhase,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RecordingPhase {
    NoPage,
    InsidePage,
}

impl RecordingCanvas {
    pub fn new() -> Self {
        Self {
            log: Vec::new(),
            phase: RecordingPhase::NoPage,
        }
    }

    /// Immutable view of commands in invocation order.
    pub fn commands(&self) -> &[CanvasCommand] {
        &self.log
    }

    fn push(&mut self, cmd: CanvasCommand) -> Result<()> {
        self.log.push(cmd);
        Ok(())
    }

    fn ensure_no_page(&self) -> Result<()> {
        if self.phase == RecordingPhase::InsidePage {
            return Err(Error::CanvasPageOpen); // see error note below
        }
        Ok(())
    }

    fn ensure_inside_page(&self) -> Result<()> {
        if self.phase == RecordingPhase::NoPage {
            return Err(Error::CanvasNoPageOpen);
        }
        Ok(())
    }
}

impl Canvas for RecordingCanvas {
    fn set_document_info(&mut self, info: &DocumentInfo) -> Result<()> {
        self.ensure_no_page()?; // document info only before first page in tests
        self.push(CanvasCommand::SetDocumentInfo(info.clone()))
    }

    fn start_page(&mut self, size: Size) -> Result<()> {
        self.ensure_no_page()?;
        self.phase = RecordingPhase::InsidePage;
        self.push(CanvasCommand::StartPage(size))
    }

    fn finish_page(&mut self) -> Result<()> {
        self.ensure_inside_page()?;
        self.phase = RecordingPhase::NoPage;
        self.push(CanvasCommand::FinishPage)
    }

    fn fill_path(&mut self, path: &Path, paint: &Paint) -> Result<()> {
        self.ensure_inside_page()?;
        self.push(CanvasCommand::FillPath {
            path: path.clone(),
            paint: *paint,
        })
    }

    fn stroke_path(&mut self, path: &Path, paint: &Paint, stroke: &Stroke) -> Result<()> {
        self.ensure_inside_page()?;
        self.push(CanvasCommand::StrokePath {
            path: path.clone(),
            paint: *paint,
            stroke: *stroke,
        })
    }

    fn draw_image(&mut self, image: &DecodedImage, rect: Rect) -> Result<()> {
        self.ensure_inside_page()?;
        self.push(CanvasCommand::DrawImage {
            image: image.clone(),
            rect,
        })
    }

    fn draw_glyph_run(&mut self, run: &GlyphRun) -> Result<()> {
        self.ensure_inside_page()?;
        self.push(CanvasCommand::DrawGlyphRun { run: run.clone() })
    }

    fn annotate_link(&mut self, rect: Rect, target: LinkTarget) -> Result<()> {
        self.ensure_inside_page()?;
        self.push(CanvasCommand::AnnotateLink { rect, target })
    }
}
```

### Error additions (`error.rs`)

Slice 2 adds only what recording and link validation need:

```rust
#[non_exhaustive]
pub enum Error {
    // … existing length errors …
    /// Hyperlink URI was empty after trim is NOT applied — empty str only.
    EmptyLink { field: &'static str },
    /// `finish_page` without `start_page`, or draw while no page open.
    CanvasNoPageOpen,
    /// `start_page` while a page is already open, or `set_document_info` mid-page.
    CanvasPageOpen,
}
```

**Note:** If the orchestrator prefers a single new variant, drop `CanvasNoPageOpen` / `CanvasPageOpen` and use `debug_assert` + test-only panics in `RecordingCanvas` only, keeping public `Error` to `EmptyLink` alone. This candidate keeps phase errors in `Error` so misuse returns `Result` in tests without panics.

---

## `document.rs`

```rust
#[derive(Debug, Clone, PartialEq, Default)]
pub struct DocumentInfo {
    pub title: Option<String>,
    pub author: Option<String>,
    pub subject: Option<String>,
    pub keywords: Option<String>,
    pub creator: Option<String>,
    pub producer: Option<String>,
}
```

No builder DSL. Paginate fills fields when metadata exists.

---

## `fonts.rs`

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FontId(pub(crate) u32);

impl FontId {
    #[doc(hidden)]
    pub fn for_test(id: u32) -> Self {
        Self(id)
    }
}
```

---

## Test helpers (`canvas::test_util`)

```rust
pub const PT_TOLERANCE: f32 = 0.01;

#[must_use]
pub fn pt_near(actual: f32, expected: f32, tol: f32) -> bool {
    (actual - expected).abs() <= tol
}

pub fn assert_commands(
    log: &[CanvasCommand],
    expected: &[CanvasCommand],
) {
    assert_eq!(log, expected);
}
```

Unit tests in `canvas.rs` `mod tests`:

- Empty URI never reaches the log (`LinkTarget::uri("")` fails).
- Page lifecycle: draw without `start_page` → `CanvasNoPageOpen`.
- Double `start_page` → `CanvasPageOpen`.
- Command order: info → start → fill → finish.
- Coordinate equality on `FillPath` points via `pt_near`.

---

## Deliberately not in slice 2

- `finish(self) -> Vec<u8>` (krilla adapter only).
- Flow-inline canvas element (提案 §2.14).
- `InvalidGeometry` / `Backend` error variants (backend PR).
- Krilla types anywhere in this module.

---

## Backend adapter note (non-code)

`KrillaCanvas` implements the same `Canvas` trait. `annotate_link` receives `LinkTarget` already validated; adapter pushes `(Rect, String)` into `pending_links` and flushes on `finish_page`. Phase errors are recording-specific; krilla adapter tracks its own page surface state.
