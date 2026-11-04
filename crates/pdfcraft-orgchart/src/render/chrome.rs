//! Page header and footer text.

use krilla::surface::Surface;
use krilla::text::Font;

use super::paint::{center_text, draw_text};
use super::TextLine;
use crate::captions::footer;
use crate::error::Error;
use crate::font::{FontMetrics, Measurer};
use crate::page::PageSpec;
use crate::units::{
    PageGeometry, FOOTER_TEXT_PT, MAIN_TITLE_PT, MAIN_TITLE_TOP_GAP_MM, PAGE_NUMBER_INSET_MM,
    SUB_TITLE_PT, TITLE_LINE_GAP_MM,
};

pub(super) fn draw_titles(
    surface: &mut Surface<'_>,
    page: &PageSpec,
    font: &mut FontMetrics,
    face: &Font,
) -> Result<(), Error> {
    let header = header_slots(page.geometry, &page.main_title, &page.sub_title, font)?;
    center_text(
        surface,
        &header.main_text,
        font,
        face,
        MAIN_TITLE_PT,
        &TextLine {
            left: header.left_mm,
            width: header.width_mm,
            y: header.main_top_mm,
        },
    )?;
    center_text(
        surface,
        &header.sub_text,
        font,
        face,
        SUB_TITLE_PT,
        &TextLine {
            left: header.left_mm,
            width: header.width_mm,
            y: header.sub_top_mm,
        },
    )?;
    Ok(())
}

struct HeaderSlots {
    left_mm: f64,
    width_mm: f64,
    main_top_mm: f64,
    sub_top_mm: f64,
    main_text: String,
    sub_text: String,
}

fn header_slots(
    geometry: PageGeometry,
    main_title: &str,
    sub_title: &str,
    measurer: &mut impl Measurer,
) -> Result<HeaderSlots, Error> {
    let width_mm = geometry.printable_width_mm();
    let main_top_mm = geometry.margin_mm + MAIN_TITLE_TOP_GAP_MM;
    let sub_top_mm = main_top_mm + measurer.line_height_mm(MAIN_TITLE_PT) + TITLE_LINE_GAP_MM;
    let main_text = fit_header_line(measurer, main_title, MAIN_TITLE_PT, width_mm)?;
    let sub_text = fit_header_line(measurer, sub_title, SUB_TITLE_PT, width_mm)?;
    Ok(HeaderSlots {
        left_mm: geometry.margin_mm,
        width_mm,
        main_top_mm,
        sub_top_mm,
        main_text,
        sub_text,
    })
}

fn fit_header_line(
    measurer: &mut impl Measurer,
    text: &str,
    size_pt: f64,
    max_width_mm: f64,
) -> Result<String, Error> {
    if measurer.string_width_mm(text, size_pt)? <= max_width_mm {
        return Ok(text.to_owned());
    }
    let ellipsis = "…";
    if measurer.string_width_mm(ellipsis, size_pt)? > max_width_mm {
        return Ok(String::new());
    }
    let chars: Vec<char> = text.chars().collect();
    let mut lo = 0_usize;
    let mut hi = chars.len();
    while lo < hi {
        let mid = lo + (hi - lo).div_ceil(2);
        let mut candidate: String = chars[..mid].iter().collect();
        candidate.push('…');
        if measurer.string_width_mm(&candidate, size_pt)? <= max_width_mm {
            lo = mid;
        } else {
            hi = mid - 1;
        }
    }
    let mut fitted: String = chars[..lo].iter().collect();
    fitted.push('…');
    Ok(fitted)
}

pub(super) fn draw_footer(
    surface: &mut Surface<'_>,
    page: &PageSpec,
    page_count: usize,
    index: usize,
    font: &mut FontMetrics,
    face: &Font,
) -> Result<(), Error> {
    let text = footer(index + 1, page_count);
    let width = font.string_width_mm(&text, FOOTER_TEXT_PT)?;
    let x = page.geometry.page_width_mm - PAGE_NUMBER_INSET_MM - width;
    let y = page.geometry.page_height_mm - PAGE_NUMBER_INSET_MM - font.ascent_mm(FOOTER_TEXT_PT);
    draw_text(surface, &text, font, face, FOOTER_TEXT_PT, x, y)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{fit_header_line, header_slots};
    use crate::error::Error;
    use crate::font::Measurer;
    use crate::render::near;
    use crate::units::PageGeometry;

    struct Widths {
        per_char_mm: f64,
        ellipsis_mm: f64,
    }

    impl Measurer for Widths {
        fn string_width_mm(&mut self, text: &str, _size_pt: f64) -> Result<f64, Error> {
            let mut width = 0.0;
            for ch in text.chars() {
                width += if ch == '…' {
                    self.ellipsis_mm
                } else {
                    self.per_char_mm
                };
            }
            Ok(width)
        }

        fn line_height_mm(&mut self, size_pt: f64) -> f64 {
            size_pt
        }

        fn ensure_glyphs(&mut self, _text: &str) -> Result<(), Error> {
            Ok(())
        }
    }

    #[test]
    fn main_title_slot_starts_at_the_page_margin() {
        let geometry = PageGeometry::default();
        let mut widths = Widths {
            per_char_mm: 1.0,
            ellipsis_mm: 1.0,
        };
        let slots = header_slots(geometry, "主体", "副标题", &mut widths).expect("slots");
        near(slots.main_top_mm, 8.0);
        near(slots.sub_top_mm, 8.0 + 16.0 + 1.5);
        near(slots.left_mm, 8.0);
        near(slots.width_mm, 210.0 - 16.0);
        near(
            slots.left_mm + slots.width_mm / 2.0,
            geometry.page_width_mm / 2.0,
        );
        assert_eq!(slots.main_text, "主体");
        assert_eq!(slots.sub_text, "副标题");
    }

    #[test]
    fn overlong_header_ends_with_an_ellipsis() {
        let geometry = PageGeometry::default();
        let mut widths = Widths {
            per_char_mm: 100.0,
            ellipsis_mm: 10.0,
        };
        let slots = header_slots(geometry, "甲乙丙", "短", &mut widths).expect("slots");
        assert_eq!(slots.main_text, "甲…");
        assert!(slots.main_text.ends_with('…'));
        assert_eq!(slots.sub_text, "短");
    }

    #[test]
    fn header_ellipsis_wider_than_the_page_is_blank() {
        let mut widths = Widths {
            per_char_mm: 100.0,
            ellipsis_mm: 1_000.0,
        };
        let fitted = fit_header_line(&mut widths, "甲乙", 16.0, 194.0).expect("fit");
        assert_eq!(fitted, "");
    }
}
