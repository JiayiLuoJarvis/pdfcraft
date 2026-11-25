//! Shared helpers for orgchart integration tests.

use std::path::PathBuf;

use pdfcraft_orgchart::{Error, ErrorCode, Font};

const ORGCHART_FONT: &str = "PDFCRAFT_ORGCHART_FONT";
const CJK_FONT: &str = "PDFCRAFT_CJK_FONT";

#[expect(clippy::print_stderr, reason = "TST-6 skip reasons")]
#[expect(
    clippy::expect_used,
    reason = "integration test: a panic is the failure report"
)]
pub(crate) fn env_font() -> Option<Font> {
    let Some(path) = std::env::var_os(ORGCHART_FONT).or_else(|| std::env::var_os(CJK_FONT)) else {
        eprintln!("skipping: {ORGCHART_FONT} and {CJK_FONT} are unset");
        return None;
    };
    let path = PathBuf::from(path);
    let data = std::fs::read(&path).expect("read orgchart test font");
    Some(Font::new(data).expect("open orgchart test font"))
}

#[expect(clippy::print_stderr, reason = "TST-6 skip reasons")]
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

/// `pdftotext -layout` often drops ASCII spaces between CJK runs. Compare without them.
///
/// Shared by several integration crates; some binaries do not call it.
#[allow(dead_code, reason = "not every integration binary uses pdftotext checks")]
#[must_use]
pub(crate) fn pdftotext_has(body: &str, needle: &str) -> bool {
    let compact = |text: &str| text.chars().filter(|ch| !ch.is_whitespace()).collect::<String>();
    compact(body).contains(&compact(needle))
}
