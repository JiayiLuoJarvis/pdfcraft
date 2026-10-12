//! Drawing surface and the ordered command log.
//!
//! Layout draws through [`Canvas`]. Tests read [`RecordingCanvas::commands`].
//! Coordinates match [`crate::geom`]: top-left origin, y downward, PDF points.

use std::any::Any;
use std::ops::Range;

use crate::error::{Error, ErrorKind};
use crate::fonts::FontId;
use crate::geom::{Color, Length, Paint, Path, Point, Positive, Rect, Size, Stroke};
use crate::limits::Limits;

/// Document metadata recorded by [`Canvas::set_document_info`].
///
/// Defined in this module so `canvas` does not depend on `spec`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[must_use]
pub struct DocumentInfo {
    /// Document title.
    pub title: Option<String>,
    /// Author name.
    pub author: Option<String>,
    /// Subject line.
    pub subject: Option<String>,
    /// Keyword list.
    pub keywords: Vec<String>,
    /// Language tag, stored as given.
    pub language: Option<String>,
}

/// A non-empty URI for a hyperlink annotation.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[must_use]
pub struct LinkTarget(String);

impl LinkTarget {
    /// Builds a URI target.
    ///
    /// An empty string is [`ErrorKind::EmptyLink`]. Spaces are kept.
    pub fn uri(uri: &str) -> std::result::Result<Self, Error> {
        if uri.is_empty() {
            return Err(Error::from(ErrorKind::EmptyLink));
        }
        Ok(Self(uri.to_owned()))
    }

    /// The URI.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Encoded format of image bytes passed to [`crate::spec::Resources::add_image`].
///
/// GIF and WebP draw their first frame as a still image.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ImageFormat {
    /// PNG bytes.
    Png,
    /// JPEG bytes.
    Jpeg,
    /// GIF bytes. The first frame is the still image.
    Gif,
    /// WebP bytes. The first frame is the still image.
    Webp,
}

/// Opaque image identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[must_use]
pub struct ImageId(pub(crate) u32);

impl ImageId {
    /// Wraps a raw identifier allocated for an image.
    pub const fn from_raw(raw: u32) -> Self {
        Self(raw)
    }

    /// The raw identifier.
    #[must_use]
    pub const fn as_u32(self) -> u32 {
        self.0
    }
}

/// Identifier and pixel size of an image the caller already decoded.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[must_use]
pub struct DecodedImage {
    /// Image identifier.
    pub id: ImageId,
    /// Width in pixels.
    pub pixel_width: u32,
    /// Height in pixels.
    pub pixel_height: u32,
}

/// Decodes one document's images and keeps whatever the PDF writer needs later.
pub(crate) trait ImageRegistry: Any + Send + Sync {
    /// Copies resource caps used by [`Self::register`].
    fn set_limits(&mut self, limits: &Limits);

    /// Checks limits, decodes, and returns the id plus pixel size layout will use.
    ///
    /// A failure leaves earlier registrations unchanged.
    fn register(
        &mut self,
        bytes: Vec<u8>,
        format: ImageFormat,
    ) -> std::result::Result<DecodedImage, Error>;
}

/// Registry for builds without a decoder. Every image fails.
#[cfg_attr(feature = "backend-krilla", allow(dead_code))]
pub(crate) struct RejectingImageRegistry;

impl ImageRegistry for RejectingImageRegistry {
    fn set_limits(&mut self, _limits: &Limits) {}

    fn register(
        &mut self,
        _bytes: Vec<u8>,
        _format: ImageFormat,
    ) -> std::result::Result<DecodedImage, Error> {
        Err(Error::from(ErrorKind::Image {
            message: "decoding needs feature `backend-krilla`".to_owned(),
        }))
    }
}

/// One glyph in a [`GlyphRun`].
#[derive(Debug, Clone, PartialEq)]
#[must_use]
pub struct GlyphPlacement {
    /// Glyph id from the font character map.
    pub glyph_id: u32,
    /// Advance in font units divided by units per em.
    ///
    /// This value is not multiplied by the font size. User-space width is
    /// `advance_norm * font_size`.
    pub advance_norm: f32,
    /// Byte range into [`GlyphRun::text`].
    pub byte_range: Range<usize>,
}

/// One horizontal glyph run.
///
/// `origin` is the baseline of the first glyph. y increases downward.
#[derive(Debug, Clone, PartialEq)]
#[must_use]
pub struct GlyphRun {
    /// Font used to draw the run.
    pub font: FontId,
    /// Font size in PDF points.
    pub font_size: Length<Positive>,
    /// Fill color.
    pub color: Color,
    /// Baseline of the first glyph.
    pub origin: Point,
    /// Glyphs in draw order.
    pub glyphs: Vec<GlyphPlacement>,
    /// UTF-8 source. Each [`GlyphPlacement::byte_range`] indexes this string.
    pub text: String,
}

/// One recorded [`Canvas`] call, in the order the caller made it.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
#[must_use]
pub enum CanvasCommand {
    /// [`Canvas::set_document_info`].
    SetDocumentInfo(DocumentInfo),
    /// [`Canvas::start_page`].
    StartPage(Size),
    /// [`Canvas::finish_page`].
    FinishPage,
    /// [`Canvas::fill_path`].
    FillPath {
        /// Filled path.
        path: Path,
        /// Fill paint.
        paint: Paint,
    },
    /// [`Canvas::stroke_path`].
    StrokePath {
        /// Stroked path.
        path: Path,
        /// Stroke paint.
        paint: Paint,
        /// Stroke width.
        stroke: Stroke,
    },
    /// [`Canvas::draw_image`].
    DrawImage {
        /// Image identity and pixel size.
        image: DecodedImage,
        /// Destination rectangle.
        rect: Rect,
    },
    /// [`Canvas::draw_glyph_run`].
    DrawGlyphRun {
        /// Glyph run.
        run: GlyphRun,
    },
    /// [`Canvas::annotate_link`].
    AnnotateLink {
        /// Annotation rectangle.
        rect: Rect,
        /// Non-empty URI.
        target: LinkTarget,
    },
}

/// Draws output that layout has already positioned.
pub trait Canvas {
    /// Stores document metadata.
    fn set_document_info(&mut self, info: &DocumentInfo) -> std::result::Result<(), Error>;

    /// Opens a page of `size`.
    fn start_page(&mut self, size: Size) -> std::result::Result<(), Error>;

    /// Closes the current page.
    fn finish_page(&mut self) -> std::result::Result<(), Error>;

    /// Fills `path` with `paint`.
    fn fill_path(&mut self, path: &Path, paint: &Paint) -> std::result::Result<(), Error>;

    /// Strokes `path` with `paint` and `stroke`.
    fn stroke_path(
        &mut self,
        path: &Path,
        paint: &Paint,
        stroke: &Stroke,
    ) -> std::result::Result<(), Error>;

    /// Draws `image` into `rect`.
    fn draw_image(&mut self, image: &DecodedImage, rect: Rect) -> std::result::Result<(), Error>;

    /// Draws a positioned glyph run.
    ///
    /// Advances stay in font units per em.
    fn draw_glyph_run(&mut self, run: &GlyphRun) -> std::result::Result<(), Error>;

    /// Records a hyperlink over `rect`.
    ///
    /// `target` is already a non-empty URI.
    fn annotate_link(&mut self, rect: Rect, target: LinkTarget) -> std::result::Result<(), Error>;
}

/// One page copied out of a [`RecordingCanvas`] log.
#[derive(Debug, Clone, PartialEq)]
#[must_use]
pub struct RecordedPage {
    size: Size,
    ops: Vec<CanvasCommand>,
}

impl RecordedPage {
    /// Page size from the matching `StartPage`.
    pub const fn size(&self) -> Size {
        self.size
    }

    /// Fill, stroke, image, glyph, and link commands, in call order.
    pub fn ops(&self) -> &[CanvasCommand] {
        &self.ops
    }
}

/// Append-only log of [`CanvasCommand`].
///
/// The log is the only stored record. [`Self::pages`] scans it.
#[derive(Debug)]
#[must_use]
pub struct RecordingCanvas {
    commands: Vec<CanvasCommand>,
}

impl RecordingCanvas {
    /// An empty command log.
    pub const fn new() -> Self {
        Self {
            commands: Vec::new(),
        }
    }

    /// Commands in call order.
    pub fn commands(&self) -> &[CanvasCommand] {
        &self.commands
    }

    /// Copies each closed page out of the command log.
    ///
    /// A page starts at [`CanvasCommand::StartPage`] and ends at the next
    /// [`CanvasCommand::FinishPage`]. The copy keeps fill, stroke, image,
    /// glyph, and link commands. An unfinished `StartPage` is left out.
    #[must_use]
    pub fn pages(&self) -> Vec<RecordedPage> {
        let mut pages = Vec::new();
        let mut index = 0;
        while index < self.commands.len() {
            let CanvasCommand::StartPage(size) = self.commands[index] else {
                index += 1;
                continue;
            };
            index += 1;
            let mut ops = Vec::new();
            let mut closed = false;
            while index < self.commands.len() {
                match &self.commands[index] {
                    CanvasCommand::FinishPage => {
                        index += 1;
                        closed = true;
                        break;
                    }
                    CanvasCommand::StartPage(_) => break,
                    CanvasCommand::SetDocumentInfo(_) => index += 1,
                    op @ (CanvasCommand::FillPath { .. }
                    | CanvasCommand::StrokePath { .. }
                    | CanvasCommand::DrawImage { .. }
                    | CanvasCommand::DrawGlyphRun { .. }
                    | CanvasCommand::AnnotateLink { .. }) => {
                        ops.push(op.clone());
                        index += 1;
                    }
                }
            }
            if closed {
                pages.push(RecordedPage { size, ops });
            }
        }
        pages
    }
}

impl Default for RecordingCanvas {
    fn default() -> Self {
        Self::new()
    }
}

impl Canvas for RecordingCanvas {
    fn set_document_info(&mut self, info: &DocumentInfo) -> std::result::Result<(), Error> {
        self.commands
            .push(CanvasCommand::SetDocumentInfo(info.clone()));
        Ok(())
    }

    fn start_page(&mut self, size: Size) -> std::result::Result<(), Error> {
        self.commands.push(CanvasCommand::StartPage(size));
        Ok(())
    }

    fn finish_page(&mut self) -> std::result::Result<(), Error> {
        self.commands.push(CanvasCommand::FinishPage);
        Ok(())
    }

    fn fill_path(&mut self, path: &Path, paint: &Paint) -> std::result::Result<(), Error> {
        self.commands.push(CanvasCommand::FillPath {
            path: path.clone(),
            paint: *paint,
        });
        Ok(())
    }

    fn stroke_path(
        &mut self,
        path: &Path,
        paint: &Paint,
        stroke: &Stroke,
    ) -> std::result::Result<(), Error> {
        self.commands.push(CanvasCommand::StrokePath {
            path: path.clone(),
            paint: *paint,
            stroke: *stroke,
        });
        Ok(())
    }

    fn draw_image(&mut self, image: &DecodedImage, rect: Rect) -> std::result::Result<(), Error> {
        self.commands.push(CanvasCommand::DrawImage {
            image: *image,
            rect,
        });
        Ok(())
    }

    fn draw_glyph_run(&mut self, run: &GlyphRun) -> std::result::Result<(), Error> {
        self.commands
            .push(CanvasCommand::DrawGlyphRun { run: run.clone() });
        Ok(())
    }

    fn annotate_link(&mut self, rect: Rect, target: LinkTarget) -> std::result::Result<(), Error> {
        self.commands
            .push(CanvasCommand::AnnotateLink { rect, target });
        Ok(())
    }
}

#[cfg_attr(not(feature = "backend-krilla"), allow(dead_code))]
pub(crate) fn replay(
    recording: &RecordingCanvas,
    canvas: &mut dyn Canvas,
) -> std::result::Result<(), Error> {
    for command in recording.commands() {
        match command {
            CanvasCommand::SetDocumentInfo(info) => canvas.set_document_info(info)?,
            CanvasCommand::StartPage(size) => canvas.start_page(*size)?,
            CanvasCommand::FinishPage => canvas.finish_page()?,
            CanvasCommand::FillPath { path, paint } => canvas.fill_path(path, paint)?,
            CanvasCommand::StrokePath {
                path,
                paint,
                stroke,
            } => canvas.stroke_path(path, paint, stroke)?,
            CanvasCommand::DrawImage { image, rect } => canvas.draw_image(image, *rect)?,
            CanvasCommand::DrawGlyphRun { run } => canvas.draw_glyph_run(run)?,
            CanvasCommand::AnnotateLink { rect, target } => {
                canvas.annotate_link(*rect, target.clone())?;
            }
        }
    }
    Ok(())
}

/// Absolute tolerance for coordinate asserts, in PDF points.
///
/// Also at the crate root under feature `test-utils`.
pub const PT_ABS_TOLERANCE: f32 = 0.01;

/// Reports whether `a` and `b` differ by at most [`PT_ABS_TOLERANCE`] points.
///
/// Also at the crate root under feature `test-utils`.
#[must_use]
pub fn pt_near(a: f32, b: f32) -> bool {
    (a - b).abs() <= PT_ABS_TOLERANCE
}

#[cfg(test)]
mod tests {
    use super::{
        pt_near, Canvas, CanvasCommand, DecodedImage, DocumentInfo, GlyphPlacement, GlyphRun,
        ImageFormat, ImageId, ImageRegistry, LinkTarget, RecordingCanvas, RejectingImageRegistry,
    };
    use crate::error::{Error, ErrorKind};
    use crate::fonts::FontId;
    use crate::geom::{Color, Paint, Path, PathVerb, Point, Positive, Rect, Stroke, A4};

    #[test]
    fn rejecting_registry_refuses_every_image() {
        let err = RejectingImageRegistry
            .register(vec![0; 8], ImageFormat::Png)
            .unwrap_err();
        assert_eq!(
            err,
            Error::from(ErrorKind::Image {
                message: "decoding needs feature `backend-krilla`".to_owned(),
            })
        );
    }

    #[test]
    fn records_draw_order_advances_and_one_page() -> std::result::Result<(), Error> {
        let info = DocumentInfo {
            title: Some("Letter".to_owned()),
            author: Some("Quire".to_owned()),
            subject: None,
            keywords: vec!["mail".to_owned()],
            language: Some("zh".to_owned()),
        };
        let from = Point::try_pt(10.0, 20.0)?;
        let to = Point::try_pt(110.0, 20.0)?;
        let path = Path::line(from, to);
        let paint = Paint::solid(Color::rgb(0, 0, 0));
        let stroke = Stroke::new(Positive::pt(0.75)?);
        let image = DecodedImage {
            id: ImageId::from_raw(7),
            pixel_width: 200,
            pixel_height: 100,
        };
        let image_rect = Rect::try_pt(10.0, 30.0, 120.0, 48.0)?;
        let origin = Point::try_pt(72.0, 100.0)?;
        let font_size = Positive::pt(12.0)?;
        let run = GlyphRun {
            font: FontId::from_raw(1),
            font_size,
            color: Color::rgb(0, 0, 0),
            origin,
            text: "AB".to_owned(),
            glyphs: vec![
                GlyphPlacement {
                    glyph_id: 1,
                    advance_norm: 0.5,
                    byte_range: 0..1,
                },
                GlyphPlacement {
                    glyph_id: 2,
                    advance_norm: 0.6,
                    byte_range: 1..2,
                },
            ],
        };
        let link_rect = Rect::try_pt(72.0, 90.0, 40.0, 14.0)?;
        let link = LinkTarget::uri("https://example.test/a")?;

        let mut rec = RecordingCanvas::new();
        rec.set_document_info(&info)?;
        rec.start_page(A4)?;
        rec.fill_path(&path, &paint)?;
        rec.stroke_path(&path, &paint, &stroke)?;
        rec.draw_image(&image, image_rect)?;
        rec.draw_glyph_run(&run)?;
        rec.annotate_link(link_rect, link)?;
        rec.finish_page()?;

        assert_eq!(
            rec.commands().iter().map(command_kind).collect::<Vec<_>>(),
            ["info", "start", "fill", "stroke", "image", "glyphs", "link", "finish"]
        );

        match &rec.commands()[0] {
            CanvasCommand::SetDocumentInfo(recorded) => assert_eq!(recorded, &info),
            other => panic!("expected document info, got {other:?}"),
        }

        let CanvasCommand::FillPath { path: filled, .. } = &rec.commands()[2] else {
            panic!("expected fill");
        };
        let PathVerb::MoveTo(start) = filled.verbs()[0] else {
            panic!("expected move");
        };
        assert!(pt_near(start.x().as_pt(), 10.0));
        assert!(pt_near(start.y().as_pt(), 20.0));
        assert!(!pt_near(start.x().as_pt(), 10.02));

        let CanvasCommand::DrawGlyphRun { run: recorded } = &rec.commands()[5] else {
            panic!("expected glyph run");
        };
        assert_eq!(recorded.font.as_u32(), 1);
        assert_eq!(recorded.font_size.as_pt().to_bits(), 12.0f32.to_bits());
        assert_eq!(recorded.glyphs[0].advance_norm.to_bits(), 0.5f32.to_bits());
        assert_eq!(recorded.glyphs[1].advance_norm.to_bits(), 0.6f32.to_bits());
        let expected_x = recorded.origin.x().as_pt()
            + recorded.glyphs[0].advance_norm * recorded.font_size.as_pt();
        assert!(pt_near(expected_x, 78.0));

        let pages = rec.pages();
        assert_eq!(pages.len(), 1);
        assert_eq!(pages[0].size(), A4);
        assert_eq!(pages[0].ops().len(), 5);
        assert!(matches!(pages[0].ops()[0], CanvasCommand::FillPath { .. }));
        assert!(matches!(
            pages[0].ops()[4],
            CanvasCommand::AnnotateLink { .. }
        ));
        Ok(())
    }

    #[test]
    fn empty_link_is_rejected_and_not_recorded() -> std::result::Result<(), Error> {
        let error = LinkTarget::uri("").expect_err("empty uri");
        assert!(matches!(error.kind, ErrorKind::EmptyLink));
        assert_eq!(error.to_string(), "link URI is empty");

        let spaced = LinkTarget::uri(" ")?;
        assert_eq!(spaced.as_str(), " ");

        let mut rec = RecordingCanvas::new();
        rec.start_page(A4)?;
        assert!(matches!(
            LinkTarget::uri("").map_err(|error| error.kind),
            Err(ErrorKind::EmptyLink)
        ));
        assert!(matches!(rec.commands(), [CanvasCommand::StartPage(_)]));
        Ok(())
    }

    #[test]
    fn replay_reissues_every_command_in_order() -> std::result::Result<(), Error> {
        let at = Point::try_pt(5.0, 6.0)?;
        let mut source = RecordingCanvas::new();
        source.set_document_info(&DocumentInfo {
            title: Some("T".to_owned()),
            ..DocumentInfo::default()
        })?;
        source.start_page(A4)?;
        source.fill_path(&Path::line(at, at), &Paint::solid(Color::rgb(1, 2, 3)))?;
        source.stroke_path(
            &Path::line(at, at),
            &Paint::solid(Color::rgb(0, 0, 0)),
            &Stroke::new(Positive::pt(1.0)?),
        )?;
        source.draw_image(
            &DecodedImage {
                id: ImageId::from_raw(2),
                pixel_width: 4,
                pixel_height: 3,
            },
            Rect::try_pt(1.0, 2.0, 3.0, 4.0)?,
        )?;
        source.draw_glyph_run(&GlyphRun {
            font: FontId::from_raw(0),
            font_size: Positive::pt(9.0)?,
            color: Color::rgb(0, 0, 0),
            origin: at,
            glyphs: Vec::new(),
            text: String::new(),
        })?;
        source.annotate_link(Rect::try_pt(0.0, 0.0, 1.0, 1.0)?, LinkTarget::uri("u")?)?;
        source.finish_page()?;

        let mut copy = RecordingCanvas::new();
        super::replay(&source, &mut copy)?;
        assert_eq!(copy.commands(), source.commands());
        assert_eq!(copy.commands().len(), 8);
        Ok(())
    }

    #[test]
    fn canvas_accepts_a_trait_object() -> std::result::Result<(), Error> {
        let mut rec = RecordingCanvas::new();
        let canvas: &mut dyn Canvas = &mut rec;
        canvas.finish_page()?;
        assert!(matches!(rec.commands(), [CanvasCommand::FinishPage]));
        Ok(())
    }

    fn command_kind(command: &CanvasCommand) -> &'static str {
        match command {
            CanvasCommand::SetDocumentInfo(_) => "info",
            CanvasCommand::StartPage(_) => "start",
            CanvasCommand::FinishPage => "finish",
            CanvasCommand::FillPath { .. } => "fill",
            CanvasCommand::StrokePath { .. } => "stroke",
            CanvasCommand::DrawImage { .. } => "image",
            CanvasCommand::DrawGlyphRun { .. } => "glyphs",
            CanvasCommand::AnnotateLink { .. } => "link",
        }
    }
}
