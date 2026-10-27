//! Caps on caller-supplied bytes, decoded images, and document size.
//!
//! Put a [`Limits`] value on [`DocumentSpec::limits`](crate::DocumentSpec::limits).
//! Defaults (32 MiB fonts/images, 40M pixels, 8192 edge, 10 000 pages, 100 000
//! blocks, 50 000 table cells, 1 000 000 paragraph bytes) suit batch generation.
//! Setters reject `0` and any value above four times that default. Overflow is
//! [`ErrorKind::LimitExceeded`] with a [`LimitKind`]. [`LimitKind`] is defined
//! in [`crate::error`] and re-exported here.

use crate::error::{Error, ErrorKind};

#[doc(inline)]
pub use crate::error::LimitKind;

const DEFAULT_FONT_BYTES: usize = 32 * 1024 * 1024;
const DEFAULT_IMAGE_BYTES: usize = 32 * 1024 * 1024;
const DEFAULT_IMAGE_PIXELS: u64 = 40_000_000;
const DEFAULT_IMAGE_EDGE: u32 = 8192;
const DEFAULT_PAGES: u32 = 10_000;
const DEFAULT_BLOCKS: u32 = 100_000;
const DEFAULT_TABLE_CELLS: u32 = 50_000;
const DEFAULT_PARAGRAPH_BYTES: usize = 1_000_000;

/// Resource and document-size caps.
///
/// Fields stay private so a value cannot skip the setter checks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[must_use]
pub struct Limits {
    font_bytes: usize,
    image_bytes: usize,
    image_pixels: u64,
    image_edge: u32,
    pages: u32,
    blocks: u32,
    table_cells: u32,
    paragraph_bytes: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            font_bytes: DEFAULT_FONT_BYTES,
            image_bytes: DEFAULT_IMAGE_BYTES,
            image_pixels: DEFAULT_IMAGE_PIXELS,
            image_edge: DEFAULT_IMAGE_EDGE,
            pages: DEFAULT_PAGES,
            blocks: DEFAULT_BLOCKS,
            table_cells: DEFAULT_TABLE_CELLS,
            paragraph_bytes: DEFAULT_PARAGRAPH_BYTES,
        }
    }
}

impl Limits {
    /// Most bytes one font file may contain. Default 32 MiB.
    #[must_use]
    pub const fn max_font_bytes(&self) -> usize {
        self.font_bytes
    }

    /// Most encoded bytes one image may contain. Default 32 MiB.
    #[must_use]
    pub const fn max_image_bytes(&self) -> usize {
        self.image_bytes
    }

    /// Most decoded pixels one image may contain. Default `40_000_000`.
    #[must_use]
    pub const fn max_image_pixels(&self) -> u64 {
        self.image_pixels
    }

    /// Most pixels on one decoded side. Default 8192.
    #[must_use]
    pub const fn max_image_edge(&self) -> u32 {
        self.image_edge
    }

    /// Most pages pagination may commit. Default `10_000`.
    #[must_use]
    pub const fn max_pages(&self) -> u32 {
        self.pages
    }

    /// Most body-flow blocks. Default `100_000`.
    #[must_use]
    pub const fn max_blocks(&self) -> u32 {
        self.blocks
    }

    /// Most table cells in the body flow. Default `50_000`.
    #[must_use]
    pub const fn max_table_cells(&self) -> u32 {
        self.table_cells
    }

    /// Most UTF-8 bytes of paragraph text in the body flow. Default `1_000_000`.
    #[must_use]
    pub const fn max_paragraph_bytes(&self) -> usize {
        self.paragraph_bytes
    }

    /// Sets the font-bytes cap.
    ///
    /// # Errors
    ///
    /// [`ErrorKind::LimitExceeded`] with [`LimitKind::FontBytes`] when `bytes` is 0
    /// (`max` is 0) or greater than four times the default.
    pub fn set_max_font_bytes(&mut self, bytes: usize) -> Result<(), Error> {
        self.font_bytes = admit_usize(bytes, DEFAULT_FONT_BYTES, LimitKind::FontBytes)?;
        Ok(())
    }

    /// Sets the per-image bytes cap.
    ///
    /// # Errors
    ///
    /// [`ErrorKind::LimitExceeded`] with [`LimitKind::ImageBytes`] when `bytes` is 0
    /// (`max` is 0) or greater than four times the default.
    pub fn set_max_image_bytes(&mut self, bytes: usize) -> Result<(), Error> {
        self.image_bytes = admit_usize(bytes, DEFAULT_IMAGE_BYTES, LimitKind::ImageBytes)?;
        Ok(())
    }

    /// Sets the per-image pixels cap.
    ///
    /// # Errors
    ///
    /// [`ErrorKind::LimitExceeded`] with [`LimitKind::ImagePixels`] when `pixels` is 0
    /// (`max` is 0) or greater than four times the default.
    pub fn set_max_image_pixels(&mut self, pixels: u64) -> Result<(), Error> {
        self.image_pixels = admit(pixels, DEFAULT_IMAGE_PIXELS, LimitKind::ImagePixels)?;
        Ok(())
    }

    /// Sets the per-image edge cap.
    ///
    /// # Errors
    ///
    /// [`ErrorKind::LimitExceeded`] with [`LimitKind::ImageEdge`] when `edge` is 0
    /// (`max` is 0) or greater than four times the default.
    pub fn set_max_image_edge(&mut self, edge: u32) -> Result<(), Error> {
        self.image_edge = narrow_u32(admit(
            u64::from(edge),
            u64::from(DEFAULT_IMAGE_EDGE),
            LimitKind::ImageEdge,
        )?);
        Ok(())
    }

    /// Sets the page-count cap.
    ///
    /// # Errors
    ///
    /// [`ErrorKind::LimitExceeded`] with [`LimitKind::Pages`] when `pages` is 0
    /// (`max` is 0) or greater than four times the default.
    pub fn set_max_pages(&mut self, pages: u32) -> Result<(), Error> {
        self.pages = narrow_u32(admit(
            u64::from(pages),
            u64::from(DEFAULT_PAGES),
            LimitKind::Pages,
        )?);
        Ok(())
    }

    /// Sets the body-block cap.
    ///
    /// # Errors
    ///
    /// [`ErrorKind::LimitExceeded`] with [`LimitKind::Blocks`] when `blocks` is 0
    /// (`max` is 0) or greater than four times the default.
    pub fn set_max_blocks(&mut self, blocks: u32) -> Result<(), Error> {
        self.blocks = narrow_u32(admit(
            u64::from(blocks),
            u64::from(DEFAULT_BLOCKS),
            LimitKind::Blocks,
        )?);
        Ok(())
    }

    /// Sets the table-cell cap.
    ///
    /// # Errors
    ///
    /// [`ErrorKind::LimitExceeded`] with [`LimitKind::TableCells`] when `cells` is 0
    /// (`max` is 0) or greater than four times the default.
    pub fn set_max_table_cells(&mut self, cells: u32) -> Result<(), Error> {
        self.table_cells = narrow_u32(admit(
            u64::from(cells),
            u64::from(DEFAULT_TABLE_CELLS),
            LimitKind::TableCells,
        )?);
        Ok(())
    }

    /// Sets the paragraph-bytes cap.
    ///
    /// # Errors
    ///
    /// [`ErrorKind::LimitExceeded`] with [`LimitKind::ParagraphBytes`] when `bytes` is 0
    /// (`max` is 0) or greater than four times the default.
    pub fn set_max_paragraph_bytes(&mut self, bytes: usize) -> Result<(), Error> {
        self.paragraph_bytes =
            admit_usize(bytes, DEFAULT_PARAGRAPH_BYTES, LimitKind::ParagraphBytes)?;
        Ok(())
    }
}

fn admit(value: u64, default: u64, kind: LimitKind) -> Result<u64, Error> {
    let ceiling = default.saturating_mul(4);
    if value == 0 || value > ceiling {
        let max = if value == 0 { 0 } else { ceiling };
        return Err(Error::from(ErrorKind::LimitExceeded { kind, value, max }));
    }
    Ok(value)
}

fn admit_usize(value: usize, default: usize, kind: LimitKind) -> Result<usize, Error> {
    let admitted = admit(value as u64, default as u64, kind)?;
    Ok(usize::try_from(admitted).expect("admitted byte cap fits usize"))
}

fn narrow_u32(value: u64) -> u32 {
    u32::try_from(value).expect("admitted count fits u32")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rejects(
        kind: LimitKind,
        default: u64,
        mut set: impl FnMut(&mut Limits, u64) -> Result<(), Error>,
        read: impl Fn(&Limits) -> u64,
    ) {
        let mut limits = Limits::default();
        assert_eq!(read(&limits), default);
        assert_eq!(
            set(&mut limits, 0).unwrap_err().kind,
            ErrorKind::LimitExceeded {
                kind,
                value: 0,
                max: 0,
            }
        );
        assert_eq!(read(&limits), default);
        let ceiling = default * 4;
        assert_eq!(
            set(&mut limits, ceiling + 1).unwrap_err().kind,
            ErrorKind::LimitExceeded {
                kind,
                value: ceiling + 1,
                max: ceiling,
            }
        );
        set(&mut limits, ceiling).unwrap();
        assert_eq!(read(&limits), ceiling);
    }

    #[test]
    fn setters_reject_zero_and_values_above_four_times_the_default() {
        let font = u64::try_from(DEFAULT_FONT_BYTES).unwrap();
        rejects(
            LimitKind::FontBytes,
            font,
            |limits, value| limits.set_max_font_bytes(usize::try_from(value).unwrap()),
            |limits| u64::try_from(limits.max_font_bytes()).unwrap(),
        );
        rejects(
            LimitKind::ImageBytes,
            font,
            |limits, value| limits.set_max_image_bytes(usize::try_from(value).unwrap()),
            |limits| u64::try_from(limits.max_image_bytes()).unwrap(),
        );
        rejects(
            LimitKind::ImagePixels,
            DEFAULT_IMAGE_PIXELS,
            Limits::set_max_image_pixels,
            Limits::max_image_pixels,
        );
        rejects(
            LimitKind::ImageEdge,
            u64::from(DEFAULT_IMAGE_EDGE),
            |limits, value| limits.set_max_image_edge(u32::try_from(value).unwrap()),
            |limits| u64::from(limits.max_image_edge()),
        );
        rejects(
            LimitKind::Pages,
            u64::from(DEFAULT_PAGES),
            |limits, value| limits.set_max_pages(u32::try_from(value).unwrap()),
            |limits| u64::from(limits.max_pages()),
        );
        rejects(
            LimitKind::Blocks,
            u64::from(DEFAULT_BLOCKS),
            |limits, value| limits.set_max_blocks(u32::try_from(value).unwrap()),
            |limits| u64::from(limits.max_blocks()),
        );
        rejects(
            LimitKind::TableCells,
            u64::from(DEFAULT_TABLE_CELLS),
            |limits, value| limits.set_max_table_cells(u32::try_from(value).unwrap()),
            |limits| u64::from(limits.max_table_cells()),
        );
        rejects(
            LimitKind::ParagraphBytes,
            u64::try_from(DEFAULT_PARAGRAPH_BYTES).unwrap(),
            |limits, value| limits.set_max_paragraph_bytes(usize::try_from(value).unwrap()),
            |limits| u64::try_from(limits.max_paragraph_bytes()).unwrap(),
        );
    }

    #[test]
    fn zero_and_over_ceiling_display() {
        let mut limits = Limits::default();
        let zero = limits.set_max_pages(0).unwrap_err();
        assert_eq!(zero.to_string(), "pages limit must be at least 1");
        let high = limits.set_max_pages(40_001).unwrap_err();
        assert_eq!(high.to_string(), "pages 40001 exceeds maximum 40000");
    }
}
