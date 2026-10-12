#![allow(clippy::wildcard_imports)]
use super::*;

/// Page facts a page-time closure can read.
///
/// Section counters are filled after pagination. With one section they match
/// `number` and `count`, because that section's span covers the document.
#[derive(Debug, Clone, Copy, PartialEq)]
#[non_exhaustive]
pub struct Page {
    /// One-based page number in the document.
    pub number: u32,
    /// Pages in the document, blank ones included.
    pub count: u32,
    /// One-based page number in the section.
    pub section_number: u32,
    /// Pages in the section.
    pub section_count: u32,
    /// Paper size.
    pub size: Size,
    /// The four margins of this page.
    pub margins: Margins,
}

impl Page {
    /// Whether this is the last page of the whole document.
    ///
    /// This is `number == count`. It is not the last page of the section. Compare
    /// [`Self::section_number`] with [`Self::section_count`] for that.
    #[must_use]
    pub fn is_last(&self) -> bool {
        self.number == self.count
    }
}

impl From<&PageInfo> for Page {
    fn from(info: &PageInfo) -> Self {
        Self {
            number: info.number.get(),
            count: info.count.get(),
            section_number: info.section_number.get(),
            section_count: info.section_count.get(),
            size: info.size,
            margins: info.margins,
        }
    }
}
