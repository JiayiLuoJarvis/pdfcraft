//! `Canvas` backed by a buffered page plan.
//!
//! This module does not import krilla. [`emit`](super::emit) is the only place
//! that constructs `Document`, `Page`, and `Surface`.

use crate::backend::emit;
use crate::canvas::{Canvas, DecodedImage, DocumentInfo, GlyphRun, LinkTarget};
use crate::error::{Error, Result};
use crate::geom::{Paint, Path, Rect, Size, Stroke};

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
    info: Option<DocumentInfo>,
    closed_pages: Vec<PagePlan>,
    open_page: Option<OpenPage>,
    failure: Option<Error>,
    serialization: Serialization,
}

impl KrillaCanvas {
    /// Production serialize settings (compressed, not pretty).
    pub(crate) fn new() -> Self {
        Self {
            info: None,
            closed_pages: Vec::new(),
            open_page: None,
            failure: None,
            serialization: Serialization::production(),
        }
    }

    /// Observable serialize settings for content-stream experiments.
    #[cfg(test)]
    pub(crate) fn new_observable_for_test() -> Self {
        Self {
            info: None,
            closed_pages: Vec::new(),
            open_page: None,
            failure: None,
            serialization: Serialization::observable(),
        }
    }

    /// Consumes the plan and writes PDF bytes.
    pub(crate) fn finish(self) -> Result<Vec<u8>> {
        if let Some(error) = self.failure {
            return Err(error);
        }
        if self.open_page.is_some() {
            return Err(Error::InvalidCanvasState {
                operation: "finish",
                state: "a page is still open",
            });
        }
        emit::render(self.info.as_ref(), &self.closed_pages, self.serialization)
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
            None => self.fail(Error::InvalidCanvasState {
                operation,
                state: "no page is open",
            }),
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
            return self.fail(Error::InvalidCanvasState {
                operation: "start_page",
                state: "a page is already open",
            });
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
            return self.fail(Error::InvalidCanvasState {
                operation: "finish_page",
                state: "no page is open",
            });
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

    fn draw_image(&mut self, _image: &DecodedImage, _rect: Rect) -> Result<()> {
        self.open_page_mut("draw_image")?;
        self.fail(Error::UnsupportedCanvasOperation {
            operation: "draw_image",
        })
    }

    fn draw_glyph_run(&mut self, _run: &GlyphRun) -> Result<()> {
        self.open_page_mut("draw_glyph_run")?;
        self.fail(Error::UnsupportedCanvasOperation {
            operation: "draw_glyph_run",
        })
    }

    fn annotate_link(&mut self, rect: Rect, target: LinkTarget) -> Result<()> {
        let page = self.open_page_mut("annotate_link")?;
        page.links.push(QueuedLink { rect, target });
        Ok(())
    }
}
