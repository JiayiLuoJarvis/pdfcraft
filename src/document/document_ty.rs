#![allow(clippy::wildcard_imports)]
use super::*;

/// Document settings and page-time closures. Body methods come from [`FlowWriter`].
pub struct Document<'a> {
    pub(crate) page_size: PageSize,
    pub(crate) doc: DocToken,
    pub(crate) fonts: FontStore,
    pub(crate) body: Sections<'a>,
    /// Document-level failures: section-closure `Err`, outside-section writes, and
    /// errors absorbed from sections as they are built.
    pub(crate) errors: ErrorBag,
    outside_reported: bool,
    pub(crate) images: Box<dyn ImageRegistry>,
}

impl<'a> Document<'a> {
    pub(crate) fn new(page_size: PageSize) -> Self {
        let doc = DocToken::next();
        Self {
            page_size,
            doc,
            fonts: FontStore::new(),
            body: Sections::Implicit(Section::new(default_style(), doc)),
            errors: ErrorBag::default(),
            outside_reported: false,
            images: Box::new(RejectingImageRegistry),
        }
    }

    /// Decodes `bytes` once and returns a handle for [`Flow::image`], cells, and [`Layer::image_at`].
    ///
    /// Fails now on oversized, unparsable, or empty images. Without `backend-krilla` every call fails.
    pub fn load_image(
        &mut self,
        bytes: Vec<u8>,
        format: ImageFormat,
    ) -> std::result::Result<Image, Error> {
        let decoded = self.images.register(bytes, format)?;
        Ok(Image {
            decoded,
            doc: self.doc,
        })
    }

    /// Registers a font from its file bytes. The first font is the one text uses.
    pub fn font(&mut self, bytes: Vec<u8>) -> std::result::Result<(), Error> {
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
    pub fn bold_font(&mut self, bytes: Vec<u8>) -> std::result::Result<(), Error> {
        if self.fonts.face(FontId::from_raw(0)).is_err() {
            return Err(Error::from(ErrorKind::Font {
                index: 0,
                message: "body font is not registered".to_owned(),
            }));
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
    /// Records [`ErrorKind::ContentOutsideSection`] when the implicit section
    /// already has body content, non-default margins, or a page hook, and when
    /// body content, margins, or hooks are written on the document after this.
    /// That error stays on the document. Later `section` calls still run.
    pub fn section(&mut self, f: impl FnOnce(&mut Section<'a>) -> Result<()>) {
        let lead_written = matches!(&self.body, Sections::Implicit(section) if section.written());
        let style = match &self.body {
            Sections::Implicit(section) => section.flow.style.clone(),
            Sections::Explicit { closed, .. } => closed.style.clone(),
        };
        let index = match &self.body {
            Sections::Implicit(_) => 0,
            Sections::Explicit { sections, .. } => sections.len(),
        };
        if lead_written {
            if let Sections::Implicit(section) = &mut self.body {
                let flow_errors = std::mem::take(&mut section.flow.errors);
                let mark = self.errors.mark();
                self.errors.absorb(flow_errors);
                self.errors.locate_since(mark, &Location::section(0));
            }
            self.fail_outside();
        }
        let mark = self.errors.mark();
        let mut section = Section::new(style, self.doc);
        let returned = f(&mut section);
        let flow_errors = std::mem::take(&mut section.flow.errors);
        self.errors.absorb(flow_errors);
        if let Err(errors) = returned {
            self.errors.absorb(errors.into());
        }
        self.errors.locate_since(mark, &Location::section(index));
        if lead_written {
            return;
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
    }

    pub(crate) fn fail_outside(&mut self) {
        if !self.outside_reported {
            self.outside_reported = true;
            self.errors
                .record(Error::from(ErrorKind::ContentOutsideSection));
        }
    }

    pub(crate) fn paint(self, canvas: &mut dyn Canvas) -> Result<Painted> {
        let Self {
            page_size,
            doc,
            mut fonts,
            body,
            mut errors,
            images,
            outside_reported: _,
        } = self;
        let sections = match body {
            Sections::Implicit(section) => vec![section],
            Sections::Explicit { sections, closed } => {
                errors.absorb(closed.errors);
                sections
            }
        };
        let mut configs = Vec::with_capacity(sections.len());
        let mut roots = Vec::with_capacity(sections.len());
        let mut decorations = Vec::with_capacity(sections.len());
        for (index, section) in sections.into_iter().enumerate() {
            let mark = errors.mark();
            let (config, root, deco, flow_errors) = section.lower(page_size, doc);
            errors.absorb(flow_errors);
            errors.locate_since(mark, &Location::section(index));
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
        let laid = {
            let mut ctx = LayoutCtx {
                fonts: &mut fonts,
                errors: &mut errors,
            };
            layout_pages(jobs, &mut ctx)
        };
        let laid = match laid {
            Ok(laid) => laid,
            Err(hard) => return Err(errors.fail(hard)),
        };
        let laid = errors.finish(laid)?;
        paint_laid(&laid, canvas).map_err(Errors::from)?;
        Ok(Painted { fonts, images })
    }

    /// Paints into a recording when `built` is `Ok`. A build `Err` is absorbed and
    /// layout does not run.
    pub(crate) fn render(self, built: Result<()>) -> Result<Rendered> {
        if let Err(returned) = built {
            let mut errors = self.collect_errors();
            errors.absorb(returned.into());
            return Err(errors.finish(()).expect_err("build Err was absorbed"));
        }
        let mut recording = RecordingCanvas::new();
        let painted = self.paint(&mut recording)?;
        Ok(Rendered {
            recording,
            fonts: painted.fonts,
            images: painted.images,
        })
    }

    fn collect_errors(self) -> ErrorBag {
        let Self {
            body, mut errors, ..
        } = self;
        match body {
            Sections::Implicit(section) => {
                let mark = errors.mark();
                errors.absorb(section.flow.errors);
                errors.locate_since(mark, &Location::section(0));
            }
            Sections::Explicit { sections, closed } => {
                errors.absorb(closed.errors);
                for (index, section) in sections.into_iter().enumerate() {
                    let mark = errors.mark();
                    errors.absorb(section.flow.errors);
                    errors.locate_since(mark, &Location::section(index));
                }
            }
        }
        errors
    }
}

pub(crate) struct Painted {
    pub(crate) fonts: FontStore,
    pub(crate) images: Box<dyn ImageRegistry>,
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
    #[cfg_attr(not(feature = "backend-krilla"), allow(dead_code))]
    pub(crate) images: Box<dyn ImageRegistry>,
}

impl Rendered {
    /// Every page's drawing commands, in paint order.
    #[must_use]
    pub fn pages(&self) -> Vec<RecordedPage> {
        self.recording.pages()
    }
}
