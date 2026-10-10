#![allow(clippy::wildcard_imports)]
use super::*;

/// Lays out a document whose default paper is `page_size`.
///
/// `build` runs once. Builder mistakes are deferred: the first one is
/// returned from here, before any page is drawn. A missing glyph anywhere,
/// including headers and layers, also fails the whole document.
///
/// Text uses the first font passed to [`Document::font`] at 12 pt with line
/// height 1.2, black, start-aligned. Margins default to zero.
pub fn document<'a>(
    page_size: PageSize,
    build: impl FnOnce(&mut Document<'a>) -> Result<()>,
) -> Result<Rendered> {
    let mut doc = Document::new(page_size);
    build(&mut doc)?;
    let mut recording = RecordingCanvas::new();
    let painted = doc.paint(&mut recording)?;
    Ok(Rendered {
        recording,
        fonts: painted.fonts,
        #[cfg(feature = "backend-krilla")]
        images: painted.images,
    })
}

/// Document settings and page-time closures. Body methods come from [`Flow`].
pub struct Document<'a> {
    pub(crate) page_size: PageSize,
    pub(crate) doc: DocToken,
    pub(crate) fonts: FontStore,
    pub(crate) body: Sections<'a>,
    /// First [`Error::ContentOutsideSection`], or the error from a section closure.
    pub(crate) error: Option<Error>,
    #[cfg(feature = "backend-krilla")]
    pub(crate) images: ImageTable,
}

impl<'a> Document<'a> {
    pub(crate) fn new(page_size: PageSize) -> Self {
        let doc = DocToken::next();
        Self {
            page_size,
            doc,
            fonts: FontStore::new(),
            body: Sections::Implicit(Section::new(default_style(), doc)),
            error: None,
            #[cfg(feature = "backend-krilla")]
            images: ImageTable::default(),
        }
    }

    /// Decodes `bytes` once and returns a handle for [`Flow::image`], cells, and [`Layer::image_at`].
    ///
    /// Fails now on oversized, unparsable, or empty images. Without `backend-krilla` every call fails.
    pub fn load_image(&mut self, bytes: Vec<u8>, format: ImageFormat) -> Result<Image> {
        #[cfg(feature = "backend-krilla")]
        {
            let decoded = self.images.register(bytes, format)?;
            Ok(Image {
                decoded,
                doc: self.doc,
            })
        }
        #[cfg(not(feature = "backend-krilla"))]
        {
            let _ = (self.doc, bytes, format);
            Err(Error::Image {
                message: "decoding needs feature `backend-krilla`".to_owned(),
            })
        }
    }

    /// Registers a font from its file bytes. The first font is the one text uses.
    pub fn font(&mut self, bytes: Vec<u8>) -> Result<()> {
        self.fonts
            .register(FontSource {
                bytes: Arc::new(bytes),
                index: 0,
            })
            .map(drop)
    }

    /// Registers the face used by [`Paragraph::bold`].
    ///
    /// A body font must be registered first. A later call replaces the bold binding.
    pub fn bold_font(&mut self, bytes: Vec<u8>) -> Result<()> {
        if self.fonts.face(FontId::from_raw(0)).is_err() {
            return Err(Error::Font {
                index: 0,
                message: "body font is not registered".to_owned(),
            });
        }
        let id = self.fonts.register(FontSource {
            bytes: Arc::new(bytes),
            index: 0,
        })?;
        self.fonts.set_bold(id);
        Ok(())
    }

    /// Sets the four page margins. Top and bottom are also the header and footer band heights.
    pub fn margins(&mut self, margins: Margins) {
        if let Sections::Implicit(section) = &mut self.body {
            section.margins(margins);
        } else {
            self.fail_outside();
        }
    }

    /// Runs `f` on every page to fill the header band. A later call replaces an earlier one.
    pub fn header(&mut self, f: impl Fn(&mut Band) + 'a) {
        if let Sections::Implicit(section) = &mut self.body {
            section.header(f);
        } else {
            self.fail_outside();
        }
    }

    /// Runs `f` on every page to fill the footer band. A later call replaces an earlier one.
    pub fn footer(&mut self, f: impl Fn(&mut Band) + 'a) {
        if let Sections::Implicit(section) = &mut self.body {
            section.footer(f);
        } else {
            self.fail_outside();
        }
    }

    /// Runs `f` on every page to draw under everything else. A later call replaces an earlier one.
    pub fn behind(&mut self, f: impl Fn(&mut Layer) + 'a) {
        if let Sections::Implicit(section) = &mut self.body {
            section.behind(f);
        } else {
            self.fail_outside();
        }
    }

    /// Runs `f` on every page to draw over everything else. A later call replaces an earlier one.
    pub fn front(&mut self, f: impl Fn(&mut Layer) + 'a) {
        if let Sections::Implicit(section) = &mut self.body {
            section.front(f);
        } else {
            self.fail_outside();
        }
    }

    /// Appends a section that starts on a new page and keeps its own margins and hooks.
    ///
    /// Registering a font or an image does not count as writing the document.
    /// Fails with [`Error::ContentOutsideSection`] when the implicit section
    /// already has body content, non-default margins, or a page hook, and when
    /// body content, margins, or hooks are written on the document after this.
    pub fn section(&mut self, f: impl FnOnce(&mut Section<'a>) -> Result<()>) -> Result<()> {
        if let Some(error) = self.error.clone() {
            return Err(error);
        }
        let lead_written = matches!(&self.body, Sections::Implicit(section) if section.written());
        if lead_written {
            let error = Error::ContentOutsideSection;
            self.error = Some(error.clone());
            return Err(error);
        }
        let style = match &self.body {
            Sections::Implicit(section) => section.flow.style.clone(),
            Sections::Explicit { closed, .. } => closed.style.clone(),
        };
        let mut section = Section::new(style, self.doc);
        if let Err(error) = f(&mut section) {
            self.error = Some(error.clone());
            return Err(error);
        }
        if matches!(self.body, Sections::Implicit(_)) {
            let style = section.flow.style.clone();
            let doc = self.doc;
            self.body = Sections::Explicit {
                sections: vec![section],
                closed: Flow::rejecting(style, doc),
            };
        } else if let Sections::Explicit { sections, .. } = &mut self.body {
            sections.push(section);
        }
        Ok(())
    }

    pub(crate) fn fail_outside(&mut self) {
        if self.error.is_none() {
            self.error = Some(Error::ContentOutsideSection);
        }
    }

    pub(crate) fn paint(self, canvas: &mut dyn Canvas) -> Result<Painted> {
        let Self {
            page_size,
            doc,
            mut fonts,
            body,
            error,
            #[cfg(feature = "backend-krilla")]
            images,
        } = self;
        if let Some(error) = error {
            return Err(error);
        }
        let sections = match body {
            Sections::Implicit(section) => vec![section],
            Sections::Explicit { sections, closed } => {
                closed.into_nodes()?;
                sections
            }
        };
        let mut configs = Vec::with_capacity(sections.len());
        let mut roots = Vec::with_capacity(sections.len());
        let mut decorations = Vec::with_capacity(sections.len());
        for section in sections {
            let (config, root, deco) = section.lower(page_size, doc)?;
            configs.push(config);
            roots.push(root);
            decorations.push(deco);
        }
        let jobs = configs
            .into_iter()
            .zip(roots)
            .zip(&decorations)
            .map(|((config, root), decorations)| SectionJob {
                config,
                root,
                decorations,
            })
            .collect();
        let mut ctx = LayoutCtx { fonts: &mut fonts };
        paginate_sections(jobs, &mut ctx, canvas)?;
        Ok(Painted {
            fonts,
            #[cfg(feature = "backend-krilla")]
            images,
        })
    }
}

pub(crate) struct Painted {
    pub(crate) fonts: FontStore,
    #[cfg(feature = "backend-krilla")]
    pub(crate) images: ImageTable,
}
impl Deref for Document<'_> {
    type Target = Flow;

    fn deref(&self) -> &Flow {
        match &self.body {
            Sections::Implicit(section) => &section.flow,
            Sections::Explicit { closed, .. } => closed,
        }
    }
}

impl DerefMut for Document<'_> {
    fn deref_mut(&mut self) -> &mut Flow {
        match &mut self.body {
            Sections::Implicit(section) => &mut section.flow,
            Sections::Explicit { closed, .. } => closed,
        }
    }
}

pub(crate) fn default_style() -> Style {
    Style {
        font: FontId::from_raw(0),
        font_size: Positive::pt(12.0).expect("12 pt is positive"),
        line_height_factor: 1.2,
        color: Color::rgb(0, 0, 0),
        paragraph_spacing: NonNegative::ZERO,
        first_line_indent: NonNegative::ZERO,
        align: TextAlign::Start,
    }
}
/// A laid-out document.
#[must_use]
pub struct Rendered {
    pub(crate) recording: RecordingCanvas,
    #[cfg_attr(not(feature = "backend-krilla"), allow(dead_code))]
    pub(crate) fonts: FontStore,
    #[cfg(feature = "backend-krilla")]
    pub(crate) images: ImageTable,
}

impl Rendered {
    /// Every page's drawing commands, in paint order.
    #[must_use]
    pub fn pages(&self) -> Vec<RecordedPage> {
        self.recording.pages()
    }

    /// Writes the pages as PDF bytes.
    #[cfg(feature = "backend-krilla")]
    pub fn to_pdf(self) -> Result<Vec<u8>> {
        let mut pdf =
            crate::backend::KrillaCanvas::new_with_resources(Arc::new(self.fonts), self.images);
        crate::canvas::replay(&self.recording, &mut pdf)?;
        pdf.finish()
    }
}
