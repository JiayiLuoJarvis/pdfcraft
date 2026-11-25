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

fn wrap(text: &str, max_width_mm: f64) -> Vec<String> {
    wrap_text(text, &mut FallbackMeasurer, 10.0, max_width_mm, true).expect("wrap")
}

#[test]
fn wrap_parts_concatenate_to_the_source() {
    let text = "甲乙丙丁戊己庚辛(股份有限公司)·股东";
    let parts = wrap(text, 8.0);
    assert_eq!(parts.concat(), text);
    assert!(parts.len() > 1);
}

#[test]
fn wrap_keeps_an_opener_with_the_following_token() {
    let parts = wrap("甲(乙丙丁戊)", 4.0);
    assert_eq!(parts.concat(), "甲(乙丙丁戊)");
    assert!(parts.iter().all(|part| !part.ends_with('(')), "{parts:?}");
    assert!(parts.iter().all(|part| !part.starts_with(')')), "{parts:?}");
}

#[test]
fn wrap_does_not_split_beside_a_middle_dot() {
    let text = "广州 · 深圳";
    let parts = wrap(text, 10.0);
    assert_eq!(parts.concat(), text);
    for part in &parts {
        assert!(!part.starts_with('·'), "{parts:?}");
        assert!(!part.ends_with('·'), "{parts:?}");
    }
}

#[test]
fn wrap_keeps_an_institution_suffix_on_one_line() {
    let parts = wrap("北京某某股份有限公司", 12.0);
    assert_eq!(parts.concat(), "北京某某股份有限公司");
    assert_eq!(parts, ["北京某某", "股份有限公司"]);
}

#[test]
fn wrap_avoids_a_one_character_last_line() {
    let parts = wrap("甲乙丙丁戊己庚", 6.0);
    assert_eq!(parts.concat(), "甲乙丙丁戊己庚");
    let last = parts.last().expect("line");
    let letters = last.chars().filter(|ch| !ch.is_whitespace()).count();
    assert!(letters > 2, "{parts:?}");
}

struct WideScalar;

impl Measurer for WideScalar {
    fn string_width_mm(&mut self, text: &str, _size_pt: f64) -> Result<f64, crate::Error> {
        Ok(if text == "宽" {
            20.0
        } else {
            crate::units::n64(text.chars().count()) * 2.0
        })
    }

    fn line_height_with_gap_mm(&mut self, _size_pt: f64, gap_pt: f64) -> f64 {
        5.0 - crate::units::pt_to_mm(crate::units::LINE_GAP_PT) + crate::units::pt_to_mm(gap_pt)
    }

    fn ensure_glyphs(&mut self, _text: &str) -> Result<(), crate::Error> {
        Ok(())
    }
}

#[test]
fn wrap_lets_a_single_token_exceed_the_line() {
    let parts = wrap_text("宽", &mut WideScalar, 10.0, 6.0, true).expect("wrap");
    assert_eq!(parts, ["宽"]);
}
