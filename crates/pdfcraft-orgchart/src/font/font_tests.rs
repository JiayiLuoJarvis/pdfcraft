//! Font bytes, wrap, and the HC `ExtraLight` face.

use std::error::Error as StdError;
use std::sync::Arc;

use super::{wrap_text, FallbackMeasurer, Font, FontMetrics, Measurer};
use crate::{ErrorCode, ErrorSubject};

fn source_text(error: &crate::Error) -> String {
    StdError::source(error).expect("source").to_string()
}

fn font_failure_keeps_the_cause_on_source(error: &crate::Error, detail: &str) {
    assert_eq!(error.code(), ErrorCode::FontError);
    assert_eq!(error.subject(), None);
    let source = source_text(error);
    assert!(source.contains(detail), "{source}");
    assert!(
        !error.message().contains(&source),
        "message={} source={source}",
        error.message()
    );
}

#[test]
fn new_rejects_bytes_that_are_not_one_face() {
    let ttc = Font::new(b"ttcf".to_vec()).expect_err("ttc");
    font_failure_keeps_the_cause_on_source(&ttc, "TTC");
    let garbage = Font::new(b"not a font".to_vec()).expect_err("bad");
    assert_eq!(garbage.code(), ErrorCode::FontError);
    assert_eq!(garbage.subject(), None);
    let garbage_source = source_text(&garbage);
    assert!(!garbage_source.is_empty(), "{garbage_source}");
    assert!(
        !garbage.message().contains(&garbage_source),
        "message={} source={garbage_source}",
        garbage.message()
    );
    let empty = Font::new(Vec::<u8>::new()).expect_err("empty");
    font_failure_keeps_the_cause_on_source(&empty, "empty");
}

#[test]
fn cjk_wraps_char_by_char() {
    let mut measurer = FallbackMeasurer;
    let parts = wrap_text("甲乙丙丁戊己", &mut measurer, 10.0, 6.0, true).expect("wrap");
    assert_eq!(parts, ["甲乙丙", "丁戊己"]);
}

#[test]
fn ascii_keeps_words_intact() {
    let mut measurer = FallbackMeasurer;
    let parts = wrap_text("ab cd ef", &mut measurer, 10.0, 6.0, true).expect("wrap");
    assert_eq!(parts, ["ab ", "cd ", "ef"]);
}

#[test]
fn overlong_ascii_token_is_cut_by_code_point() {
    let mut measurer = FallbackMeasurer;
    let text = "abcdefgh";
    let parts = wrap_text(text, &mut measurer, 10.0, 6.0, true).expect("wrap");
    assert_eq!(parts.concat(), text);
    assert!(parts.len() > 1);
    for part in &parts {
        let width = measurer.string_width_mm(part, 10.0).expect("width");
        assert!(width <= 6.0);
    }
}

#[test]
fn ratio_text_stays_one_line_when_wrap_is_off() {
    let mut measurer = FallbackMeasurer;
    let long_ratio = "12.50%".repeat(10);
    let width = measurer.string_width_mm(&long_ratio, 9.0).expect("width");
    assert!(width > 38.0);
    let parts = wrap_text(&long_ratio, &mut measurer, 9.0, 38.0, false).expect("wrap");
    assert_eq!(parts, [long_ratio]);
}

fn hc_font() -> Option<Font> {
    crate::test_font::env_font()
}

#[test]
fn from_shared_keeps_the_arc_and_debug_omits_bytes() {
    let Some(font) = hc_font() else {
        return;
    };
    let source = Arc::clone(&font.data);
    let again = Font::from_shared(Arc::clone(&source)).expect("font");
    assert!(
        Arc::ptr_eq(&source, &again.data),
        "from_shared keeps the buffer"
    );
    let rendered = format!("{again:?}");
    assert!(rendered.contains("units_per_em"), "{rendered}");
    assert!(
        rendered.len() < source.len(),
        "debug dumped face bytes: {rendered}"
    );
    let metrics = FontMetrics::from_font(&again);
    assert!(Arc::ptr_eq(&again.data, metrics.bytes()));
}

#[test]
fn hc_line_height_matches_hhea_plus_gap() {
    let Some(bytes) = hc_font() else {
        return;
    };
    let font = FontMetrics::from_font(&bytes);
    let line_h = font.line_height_mm(10.0);
    assert!((4.0..=8.0).contains(&line_h));
    assert!(font.ascent_mm(10.0) < line_h);
}

#[test]
fn hebrew_is_missing_from_the_hc_face() {
    let Some(bytes) = hc_font() else {
        return;
    };
    let mut font = FontMetrics::from_font(&bytes);
    let width = font.string_width_mm("אבג", 10.0).expect_err("width");
    assert_eq!(width.code(), ErrorCode::MissingGlyph);
    let ensured = font.ensure_glyphs("אבג").expect_err("ensure");
    assert_eq!(ensured.code(), ErrorCode::MissingGlyph);
    assert!(ensured.message().contains("missing glyph"));
    assert_eq!(ensured.subject(), Some(&ErrorSubject::Character('א')));
    assert_eq!(width.subject(), Some(&ErrorSubject::Character('א')));
}

#[test]
fn hc_latin_glyph_run_is_stable_and_has_width() {
    let Some(bytes) = hc_font() else {
        return;
    };
    let mut font = FontMetrics::from_font(&bytes);
    let first = font.glyph_run("A").expect("glyphs");
    let second = font.glyph_run("A").expect("glyphs");
    assert_eq!(first, second);
    assert_eq!(first.len(), 1);
    assert!(first[0].glyph_id > 0);
    let width = font.string_width_mm("A", 10.0).expect("width");
    assert!(width > 0.0);
}
