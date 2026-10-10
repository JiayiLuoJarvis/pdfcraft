//! Glyph mapping and advance widths via skrifa.
//!
//! Missing cmap entries become errors; v1 does not draw `.notdef` or fall back.

#![cfg_attr(not(test), allow(dead_code))]

use std::ops::Range;

use skrifa::instance::{LocationRef, Size};
use skrifa::{FontRef, MetadataProvider};

use crate::error::{Error, Result};
use crate::fonts::{FaceMetrics, FontId, FontStore};
use crate::geom::{Length, Positive};

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct MeasuredGlyph {
    pub(crate) scalar: char,
    pub(crate) glyph_id: u32,
    pub(crate) advance_font_units: f32,
    pub(crate) advance_norm: f32,
    pub(crate) byte_range: Range<usize>,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct MeasuredText {
    pub(crate) text: String,
    pub(crate) glyphs: Vec<MeasuredGlyph>,
    pub(crate) advance_norm: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct LineMetrics {
    pub(crate) ascent: f32,
    pub(crate) descent: f32,
    pub(crate) leading: f32,
    pub(crate) line_height: f32,
    pub(crate) baseline: f32,
}

pub(crate) fn measure_text(
    fonts: &mut FontStore,
    font: FontId,
    text: &str,
) -> Result<MeasuredText> {
    let (bytes, index, units_per_em) = {
        let face = fonts.face(font)?;
        (
            std::sync::Arc::clone(&face.source.bytes),
            face.source.index,
            face.metrics.units_per_em,
        )
    };
    let font_ref = FontRef::from_index(bytes.as_slice(), index).map_err(|error| Error::Font {
        index,
        message: error.to_string(),
    })?;
    let charmap = font_ref.charmap();
    let glyph_metrics = font_ref.glyph_metrics(Size::unscaled(), LocationRef::default());

    let mut glyphs = Vec::new();
    let mut total_norm = 0.0;
    for (start, scalar) in text.char_indices() {
        let Some(gid) = charmap.map(scalar) else {
            return Err(Error::MissingGlyph { font, scalar });
        };
        let glyph_id = gid.to_u32();
        let advance_font_units = {
            let face = fonts.face_mut(font)?;
            if let Some(&cached) = face.advances.get(&glyph_id) {
                cached
            } else {
                let width = glyph_metrics.advance_width(gid).ok_or(Error::Font {
                    index,
                    message: format!("no unscaled advance for glyph {glyph_id}"),
                })?;
                face.advances.insert(glyph_id, width);
                width
            }
        };
        let advance_norm = advance_font_units / units_per_em;
        total_norm += advance_norm;
        let end = start + scalar.len_utf8();
        glyphs.push(MeasuredGlyph {
            scalar,
            glyph_id,
            advance_font_units,
            advance_norm,
            byte_range: start..end,
        });
    }

    Ok(MeasuredText {
        text: text.to_owned(),
        glyphs,
        advance_norm: total_norm,
    })
}

pub(crate) fn line_metrics(
    face: &FaceMetrics,
    font_size: Length<Positive>,
    factor: f32,
) -> Result<LineMetrics> {
    if !factor.is_finite() || factor <= 0.0 {
        return Err(Error::InvalidGeometry);
    }
    let size = font_size.as_pt();
    let scale = size / face.units_per_em;
    let ascent = face.ascent * scale;
    let descent = face.descent * scale;
    let leading = face.leading * scale;
    let natural = ascent - descent + leading;
    Ok(LineMetrics {
        ascent,
        descent,
        leading,
        line_height: natural.max(size * factor),
        baseline: ascent,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fonts::{fixture_bytes, FontSource, FontStore};
    use crate::geom::Positive;

    fn loaded() -> (FontStore, FontId) {
        let mut fonts = FontStore::new();
        let id = fonts
            .register(FontSource {
                bytes: fixture_bytes(),
                index: 0,
            })
            .unwrap();
        (fonts, id)
    }

    #[test]
    fn unmapped_scalar_is_missing_glyph() {
        let (mut fonts, id) = loaded();
        let err = measure_text(&mut fonts, id, "\u{FFFF}").unwrap_err();
        assert!(matches!(
            err,
            Error::MissingGlyph {
                font,
                scalar: '\u{FFFF}'
            } if font == id
        ));
    }

    #[test]
    fn fixture_ascii_and_cjk_advances_are_pinned() {
        let (mut fonts, id) = loaded();
        let measured = measure_text(&mut fonts, id, "A第").unwrap();
        assert_eq!(measured.glyphs.len(), 2);
        assert_eq!(measured.glyphs[0].scalar, 'A');
        assert_eq!(measured.glyphs[1].scalar, '第');
        assert_eq!(
            measured.glyphs[0].advance_font_units.to_bits(),
            707.0f32.to_bits()
        );
        assert_eq!(
            measured.glyphs[1].advance_font_units.to_bits(),
            1000.0f32.to_bits()
        );
        assert_eq!(
            measured.glyphs[0].advance_norm.to_bits(),
            0.707f32.to_bits()
        );
        assert_eq!(measured.glyphs[1].advance_norm.to_bits(), 1.0f32.to_bits());
        assert_eq!(
            fonts.metrics(id).unwrap().units_per_em.to_bits(),
            1000.0f32.to_bits()
        );
    }

    #[test]
    fn line_metrics_keep_negative_descent() {
        let (fonts, id) = loaded();
        let face = fonts.metrics(id).unwrap();
        assert!(face.descent < 0.0);
        let line = line_metrics(face, Positive::pt(12.0).unwrap(), 1.2).unwrap();
        assert!(line.descent < 0.0);
        assert!(line.line_height > 0.0);
        assert_eq!(line.baseline.to_bits(), line.ascent.to_bits());
    }

    #[test]
    fn non_positive_factor_is_invalid() {
        let face = FaceMetrics {
            units_per_em: 1000.0,
            ascent: 800.0,
            descent: -200.0,
            leading: 0.0,
        };
        let size = Positive::pt(12.0).unwrap();
        assert!(matches!(
            line_metrics(&face, size, 0.0),
            Err(Error::InvalidGeometry)
        ));
        assert!(matches!(
            line_metrics(&face, size, f32::NAN),
            Err(Error::InvalidGeometry)
        ));
    }
}
