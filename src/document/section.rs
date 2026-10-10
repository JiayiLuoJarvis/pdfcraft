#![allow(clippy::wildcard_imports)]
use super::*;

pub(crate) type BandHook<'a> = Box<dyn Fn(&mut Band) + 'a>;
pub(crate) type LayerHook<'a> = Box<dyn Fn(&mut Layer) + 'a>;

/// Implicit lead section, or the explicit sections that replaced it.
pub(crate) enum Sections<'a> {
    Implicit(Section<'a>),
    Explicit {
        sections: Vec<Section<'a>>,
        /// Body pushes after the first `section` are rejected.
        closed: Flow,
    },
}

/// One run of pages with its own paper, margins, and page decorations.
pub struct Section<'a> {
    pub(crate) page_size: Option<PageSize>,
    pub(crate) margins: Margins,
    pub(crate) flow: Flow,
    pub(crate) header: Option<BandHook<'a>>,
    pub(crate) footer: Option<BandHook<'a>>,
    pub(crate) behind: Option<LayerHook<'a>>,
    pub(crate) front: Option<LayerHook<'a>>,
}

impl<'a> Section<'a> {
    pub(crate) fn new(style: Style, doc: DocToken) -> Self {
        Self {
            page_size: None,
            margins: Margins::default(),
            flow: Flow::new(style, doc),
            header: None,
            footer: None,
            behind: None,
            front: None,
        }
    }

    pub(crate) fn written(&self) -> bool {
        self.flow.written()
            || self.margins != Margins::default()
            || self.header.is_some()
            || self.footer.is_some()
            || self.behind.is_some()
            || self.front.is_some()
    }

    /// Sets this section's paper. Otherwise the document default is used.
    pub fn page_size(&mut self, page_size: PageSize) {
        self.page_size = Some(page_size);
    }

    /// Sets the four margins. Top and bottom are also the header and footer band heights.
    pub fn margins(&mut self, margins: Margins) {
        self.margins = margins;
    }

    /// Runs `f` on every page of this section to fill the header band.
    ///
    /// A later call replaces an earlier one.
    pub fn header(&mut self, f: impl Fn(&mut Band) + 'a) {
        self.header = Some(Box::new(f));
    }

    /// Runs `f` on every page of this section to fill the footer band.
    ///
    /// A later call replaces an earlier one.
    pub fn footer(&mut self, f: impl Fn(&mut Band) + 'a) {
        self.footer = Some(Box::new(f));
    }

    /// Runs `f` on every page of this section to draw under everything else.
    ///
    /// A later call replaces an earlier one.
    pub fn behind(&mut self, f: impl Fn(&mut Layer) + 'a) {
        self.behind = Some(Box::new(f));
    }

    /// Runs `f` on every page of this section to draw over everything else.
    ///
    /// A later call replaces an earlier one.
    pub fn front(&mut self, f: impl Fn(&mut Layer) + 'a) {
        self.front = Some(Box::new(f));
    }

    pub(crate) fn lower(
        self,
        default_page: PageSize,
        doc: DocToken,
    ) -> Result<(PaginateConfig, FlowNode, Decorations<'a>)> {
        let Self {
            page_size,
            margins,
            flow,
            header,
            footer,
            behind,
            front,
        } = self;
        let style = flow.style.clone();
        let config = PaginateConfig {
            page_size: page_size.unwrap_or(default_page).resolve(),
            margins,
        };
        let root = FlowNode::Stack(flow.into_nodes()?);
        let decorations = Decorations {
            header: header.map(|hook| band_content(hook, style.clone())),
            footer: footer.map(|hook| band_content(hook, style.clone())),
            behind: behind.map(|hook| layer_content(hook, style.clone(), doc)),
            front: front.map(|hook| layer_content(hook, style, doc)),
        };
        Ok((config, root, decorations))
    }
}
