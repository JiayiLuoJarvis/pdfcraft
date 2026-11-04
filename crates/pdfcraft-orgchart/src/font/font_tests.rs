//! Font bytes, wrap, and the HC `ExtraLight` face.

use std::sync::Arc;

use super::{wrap_text, FallbackMeasurer, FontBytes, FontMetrics, Measurer};
use crate::ErrorCode;

#[test]
fn new_wraps_owned_bytes() {
    let font = FontBytes::new(b"face".to_vec());
    assert_eq!(font.data.as_slice(), b"face");
}

#[test]
fn from_shared_keeps_the_arc() {
    let source = Arc::new(b"face".to_vec());
    let font = FontBytes::from_shared(Arc::clone(&source));
    assert!(Arc::ptr_eq(&source, &font.data));
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

fn hc_font() -> Option<FontBytes> {
    crate::test_font::env_font()
}

#[test]
fn open_keeps_the_shared_buffer() {
    let Some(font) = hc_font() else {
        return;
    };
    let metrics = FontMetrics::open(&font).expect("open");
    assert!(Arc::ptr_eq(&font.data, metrics.bytes()));
}

#[test]
fn hc_line_height_matches_hhea_plus_gap() {
    let Some(bytes) = hc_font() else {
        return;
    };
    let font = FontMetrics::open(&bytes).expect("open");
    let line_h = font.line_height_mm(10.0);
    assert!((4.0..=8.0).contains(&line_h));
    assert!(font.ascent_mm(10.0) < line_h);
}

#[test]
fn hebrew_is_missing_from_the_hc_face() {
    let Some(bytes) = hc_font() else {
        return;
    };
    let mut font = FontMetrics::open(&bytes).expect("open");
    let width = font.string_width_mm("אבג", 10.0).expect_err("width");
    assert_eq!(width.code, ErrorCode::MissingGlyph);
    let ensured = font.ensure_glyphs("אבג").expect_err("ensure");
    assert_eq!(ensured.code, ErrorCode::MissingGlyph);
    assert!(ensured.message.contains("missing glyph"));
}

#[test]
fn a_collection_and_garbage_are_font_error() {
    let ttc = FontMetrics::open(&FontBytes::new(b"ttcf".to_vec())).expect_err("ttc");
    assert_eq!(ttc.code, ErrorCode::FontError);
    assert!(ttc.message.contains("TTC"));
    let garbage = FontMetrics::open(&FontBytes::new(b"not a font".to_vec())).expect_err("bad");
    assert_eq!(garbage.code, ErrorCode::FontError);
    let empty = FontMetrics::open(&FontBytes::new(Vec::<u8>::new())).expect_err("empty");
    assert_eq!(empty.code, ErrorCode::FontError);
}

#[test]
fn hc_latin_glyph_run_is_stable_and_has_width() {
    let Some(bytes) = hc_font() else {
        return;
    };
    let mut font = FontMetrics::open(&bytes).expect("open");
    let first = font.glyph_run("A").expect("glyphs");
    let second = font.glyph_run("A").expect("glyphs");
    assert_eq!(first, second);
    assert_eq!(first.len(), 1);
    assert!(first[0].glyph_id > 0);
    let width = font.string_width_mm("A", 10.0).expect("width");
    assert!(width > 0.0);
}
