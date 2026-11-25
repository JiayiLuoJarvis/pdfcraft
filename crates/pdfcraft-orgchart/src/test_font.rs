//! Load the orgchart test face from the environment.

use std::path::PathBuf;

use crate::{Error, ErrorCode, Font};

const ORGCHART_FONT: &str = "PDFCRAFT_ORGCHART_FONT";
const CJK_FONT: &str = "PDFCRAFT_CJK_FONT";

/// `PDFCRAFT_ORGCHART_FONT`, or `PDFCRAFT_CJK_FONT` if the former is unset.
///
/// Prints a skip reason and returns `None` when both variables are unset.
pub(crate) fn env_font() -> Option<Font> {
    let Some(path) = std::env::var_os(ORGCHART_FONT).or_else(|| std::env::var_os(CJK_FONT)) else {
        eprintln!("skipping: {ORGCHART_FONT} and {CJK_FONT} are unset");
        return None;
    };
    let path = PathBuf::from(path);
    let data = std::fs::read(&path)
        .unwrap_or_else(|error| panic!("read orgchart test font {}: {error}", path.display()));
    Some(
        Font::new(data)
            .unwrap_or_else(|error| panic!("open orgchart test font {}: {error}", path.display())),
    )
}

/// True when `error` is a missing glyph on the fixture face, after printing why.
pub(crate) fn skip_missing_glyph(error: &Error) -> bool {
    if error.code() == ErrorCode::MissingGlyph {
        eprintln!(
            "skipping: test font is missing a glyph needed by the fixture ({})",
            error.message()
        );
        true
    } else {
        false
    }
}
