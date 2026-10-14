//! Load the orgchart test face from the environment.

use std::path::PathBuf;

use crate::FontBytes;

/// `PDFCRAFT_ORGCHART_FONT`, or `PDFCRAFT_CJK_FONT` if the former is unset.
pub(crate) fn env_font() -> Option<FontBytes> {
    let path = std::env::var_os("PDFCRAFT_ORGCHART_FONT")
        .or_else(|| std::env::var_os("PDFCRAFT_CJK_FONT"))?;
    let path = PathBuf::from(path);
    let data = std::fs::read(&path)
        .unwrap_or_else(|error| panic!("read orgchart test font {}: {error}", path.display()));
    Some(FontBytes::new(data))
}
