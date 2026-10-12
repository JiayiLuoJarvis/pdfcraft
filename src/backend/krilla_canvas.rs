//! `Canvas` backed by a buffered page plan.
//!
//! This module does not import krilla. [`emit`](super::emit) is the only place
//! that constructs `Document`, `Page`, and `Surface`.

use std::sync::Arc;

use crate::backend::emit;
use crate::backend::images::{ImageTable, Raster};
use crate::canvas::{Canvas, DecodedImage, DocumentInfo, GlyphPlacement, GlyphRun, LinkTarget};
use crate::error::{Error, ErrorKind, Fallible as Result};
use crate::fonts::FontStore;
use crate::geom::{Color, Length, Paint, Path, Point, Positive, Rect, Size, Stroke};

/// Serialize profile. Converted to krilla settings only inside `emit`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Serialization {
    pub(super) pretty: bool,
    pub(super) compress_content_streams: bool,
}

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

/// One closed page ready for emission.
#[derive(Debug, Clone)]
pub(super) struct PagePlan {
    pub(super) size: Size,
    pub(super) marks: Box<[Mark]>,
    pub(super) links: Box<[QueuedLink]>,
}

/// A page still accepting canvas calls.
#[derive(Debug, Clone)]
struct OpenPage {
    size: Size,
    marks: Vec<Mark>,
    links: Vec<QueuedLink>,
}

/// Painted content that goes through `Surface`.
#[derive(Debug, Clone)]
pub(super) enum Mark {
    /// Filled path.
    Fill {
        /// Path to fill.
        path: Path,
        /// Fill paint.
        paint: Paint,
    },
    /// Stroked path.
    Stroke {
        /// Path to stroke.
        path: Path,
        /// Stroke paint.
        paint: Paint,
        /// Stroke width.
        stroke: Stroke,
    },
    Glyphs {
        font_bytes: Arc<Vec<u8>>,
        font_index: u32,
        skrifa_units_per_em: f32,
        font_size: Length<Positive>,
        color: Color,
        origin: Point,
        glyphs: Box<[GlyphPlacement]>,
        text: Box<str>,
    },
    /// Decoded image painted into `rect` (page coordinates, top-left, y down).
    Image { raster: Raster, rect: Rect },
}

/// Link recorded while a surface would still be borrowed.
#[derive(Debug, Clone)]
pub(super) struct QueuedLink {
    pub(super) rect: Rect,
    pub(super) target: LinkTarget,
}

/// Private krilla-backed canvas. Holds no live krilla document.
#[derive(Debug)]
pub(crate) struct KrillaCanvas {
    font_store: Arc<FontStore>,
    images: ImageTable,
    info: Option<DocumentInfo>,
    closed_pages: Vec<PagePlan>,
    open_page: Option<OpenPage>,
    failure: Option<Error>,
    serialization: Serialization,
}

impl KrillaCanvas {
    fn with_resources(
        font_store: Arc<FontStore>,
        images: ImageTable,
        serialization: Serialization,
    ) -> Self {
        Self {
            font_store,
            images,
            info: None,
            closed_pages: Vec::new(),
            open_page: None,
            failure: None,
            serialization,
        }
    }

    /// Production serialize settings (compressed, not pretty).
    pub(crate) fn new() -> Self {
        Self::with_resources(
            Arc::new(FontStore::new()),
            ImageTable::default(),
            Serialization::production(),
        )
    }

    pub(crate) fn new_with_fonts(fonts: Arc<FontStore>) -> Self {
        Self::new_with_resources(fonts, ImageTable::default())
    }

    pub(crate) fn new_with_resources(fonts: Arc<FontStore>, images: ImageTable) -> Self {
        Self::with_resources(fonts, images, Serialization::production())
    }

    /// Observable serialize settings for content-stream experiments.
    #[cfg(test)]
    pub(crate) fn new_observable_for_test() -> Self {
        Self::with_resources(
            Arc::new(FontStore::new()),
            ImageTable::default(),
            Serialization::observable(),
        )
    }

    #[cfg(test)]
    pub(crate) fn new_observable_with_fonts(fonts: Arc<FontStore>) -> Self {
        Self::with_resources(fonts, ImageTable::default(), Serialization::observable())
    }

    #[cfg(test)]
    pub(crate) fn new_observable_with_resources(fonts: Arc<FontStore>, images: ImageTable) -> Self {
        Self::with_resources(fonts, images, Serialization::observable())
    }

    /// Consumes the plan and writes PDF bytes.
    pub(crate) fn finish(self) -> Result<Vec<u8>> {
        let Self {
            font_store,
            images,
            info,
            closed_pages,
            open_page,
            failure,
            serialization,
        } = self;
        // Marks already own font bytes and rasters. Emission does not look up either table.
        drop(font_store);
        drop(images);
        if let Some(error) = failure {
            return Err(error);
        }
        if open_page.is_some() {
            return Err(Error::from(ErrorKind::InvalidCanvasState {
                operation: "finish",
                state: "a page is still open",
            }));
        }
        emit::render(info.as_ref(), &closed_pages, serialization)
    }

    fn fail<T>(&mut self, error: Error) -> Result<T> {
        if self.failure.is_none() {
            self.failure = Some(error.clone());
        }
        Err(self.failure.clone().unwrap_or(error))
    }

    fn ensure_ok(&mut self) -> Result<()> {
        if let Some(error) = &self.failure {
            return Err(error.clone());
        }
        Ok(())
    }

    fn open_page_mut(&mut self, operation: &'static str) -> Result<&mut OpenPage> {
        self.ensure_ok()?;
        match self.open_page {
            Some(ref mut page) => Ok(page),
            None => self.fail(Error::from(ErrorKind::InvalidCanvasState {
                operation,
                state: "no page is open",
            })),
        }
    }
}

impl Canvas for KrillaCanvas {
    fn set_document_info(&mut self, info: &DocumentInfo) -> Result<()> {
        self.ensure_ok()?;
        self.info = Some(info.clone());
        Ok(())
    }

    fn start_page(&mut self, size: Size) -> Result<()> {
        self.ensure_ok()?;
        if self.open_page.is_some() {
            return self.fail(Error::from(ErrorKind::InvalidCanvasState {
                operation: "start_page",
                state: "a page is already open",
            }));
        }
        self.open_page = Some(OpenPage {
            size,
            marks: Vec::new(),
            links: Vec::new(),
        });
        Ok(())
    }

    fn finish_page(&mut self) -> Result<()> {
        self.ensure_ok()?;
        let Some(open) = self.open_page.take() else {
            return self.fail(Error::from(ErrorKind::InvalidCanvasState {
                operation: "finish_page",
                state: "no page is open",
            }));
        };
        self.closed_pages.push(PagePlan {
            size: open.size,
            marks: open.marks.into_boxed_slice(),
            links: open.links.into_boxed_slice(),
        });
        Ok(())
    }

    fn fill_path(&mut self, path: &Path, paint: &Paint) -> Result<()> {
        let page = self.open_page_mut("fill_path")?;
        page.marks.push(Mark::Fill {
            path: path.clone(),
            paint: *paint,
        });
        Ok(())
    }

    fn stroke_path(&mut self, path: &Path, paint: &Paint, stroke: &Stroke) -> Result<()> {
        let page = self.open_page_mut("stroke_path")?;
        page.marks.push(Mark::Stroke {
            path: path.clone(),
            paint: *paint,
            stroke: *stroke,
        });
        Ok(())
    }

    fn draw_image(&mut self, image: &DecodedImage, rect: Rect) -> Result<()> {
        self.open_page_mut("draw_image")?;
        let raster = match self.images.get(image.id) {
            Some(raster) => raster.clone(),
            None => {
                return self.fail(Error::from(ErrorKind::UnknownImage {
                    id: image.id.as_u32(),
                }));
            }
        };
        let page = self.open_page_mut("draw_image")?;
        page.marks.push(Mark::Image { raster, rect });
        Ok(())
    }

    fn draw_glyph_run(&mut self, run: &GlyphRun) -> Result<()> {
        self.open_page_mut("draw_glyph_run")?;
        let resolved = match self.font_store.resolve(run.font) {
            Ok(resolved) => resolved,
            Err(error) => return self.fail(error),
        };
        let page = self.open_page_mut("draw_glyph_run")?;
        page.marks.push(Mark::Glyphs {
            font_bytes: resolved.bytes,
            font_index: resolved.index,
            skrifa_units_per_em: resolved.skrifa_units_per_em,
            font_size: run.font_size,
            color: run.color,
            origin: run.origin,
            glyphs: run.glyphs.clone().into_boxed_slice(),
            text: run.text.clone().into_boxed_str(),
        });
        Ok(())
    }

    fn annotate_link(&mut self, rect: Rect, target: LinkTarget) -> Result<()> {
        let page = self.open_page_mut("annotate_link")?;
        page.links.push(QueuedLink { rect, target });
        Ok(())
    }
}
