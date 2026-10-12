#![allow(clippy::wildcard_imports)]
use super::*;

/// Font and image registration retained so existing intra-doc links resolve.
///
/// Callers lay documents out from [`crate::DocumentSpec`] through [`crate::layout_spec`].
pub struct Document {
    doc: DocToken,
    fonts: FontStore,
    images: Box<dyn ImageRegistry>,
}

impl Document {
    /// Starts an empty registration table for `page_size`.
    ///
    /// Layout uses [`crate::layout_spec`], which carries its own paper and resources.
    #[must_use]
    pub fn new(_page_size: PageSize) -> Self {
        Self {
            doc: DocToken::next(),
            fonts: FontStore::new(),
            images: Box::new(RejectingImageRegistry),
        }
    }

    /// Installs caps for later [`Self::load_image`] calls.
    ///
    /// Fonts and images already registered stay registered. Lowering a cap does not recheck them.
    pub fn set_limits(&mut self, limits: Limits) {
        self.fonts.set_max_bytes(limits.max_font_bytes());
        self.images.set_limits(&limits);
    }

    /// Decodes `bytes` once and returns a handle.
    ///
    /// Fails now on empty bytes, a byte cap, a pixel or edge cap, or a decode error.
    /// PNG pixel data is checked here. A valid header with a damaged IDAT fails this call.
    /// JPEG, GIF, and WebP can still report a pixel error when the PDF is written. That error
    /// is [`ErrorKind::Backend`] and has no page, because krilla raises it when the document
    /// finishes.
    /// The byte cap runs before decode. Pixel and edge caps run after a decoder reports
    /// a size, so a hostile image can still allocate during decode.
    /// Without `backend-krilla` every call fails.
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
