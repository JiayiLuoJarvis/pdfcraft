//! Glyph mapping and advance widths via skrifa.
//!
//! Missing cmap entries become errors; v1 does not draw `.notdef` or fall back.

use std::ops::Range;

use skrifa::instance::{LocationRef, Size};
use skrifa::MetadataProvider;

use crate::error::{Error, ErrorBag, ErrorKind, Fallible as Result, FontFault, GeometryKind};
use crate::fonts::{face_metrics, parse_face, FaceMetrics, FontFace, FontId, FontStore};
use crate::geom::{Finite, Length, Positive};

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct MeasuredGlyph {
    pub(crate) scalar: char,
    pub(crate) glyph_id: u32,
    pub(crate) advance_font_units: f32,
    pub(crate) advance_norm: f32,
    pub(crate) byte_range: Range<usize>,
    /// Missing and unknown-font scalars stay in the cluster tape at advance 0 and are not drawn.
    pub(crate) omit: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct MeasuredText {
    pub(crate) text: String,
    pub(crate) glyphs: Vec<MeasuredGlyph>,
    pub(crate) advance_norm: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct LineMetrics {
    pub(crate) ascent: Length<Finite>,
    pub(crate) descent: Length<Finite>,
    pub(crate) leading: Length<Finite>,
    pub(crate) line_height: Length<Positive>,
    pub(crate) baseline: Length<Finite>,
}

pub(crate) fn measure_text(
    fonts: &mut FontStore,
    errors: &mut ErrorBag,
    font: FontId,
    text: &str,
) -> Result<MeasuredText> {
    let face = match fonts.face(font) {
        Ok(face) => face,
        Err(error) => {
            if matches!(error.kind, ErrorKind::UnknownFont { .. }) {
                errors.record(FontFault::UnknownFont { id: font.as_u32() });
                return Ok(omitted_text(text));
            }
            return Err(error);
        }
    };
    let bytes = std::sync::Arc::clone(&face.source.bytes);
    let index = face.source.index;
    let units_per_em = face.metrics.units_per_em;
    let font_ref = parse_face(bytes.as_slice(), index)?;
    let charmap = font_ref.charmap();
    let glyph_metrics = font_ref.glyph_metrics(Size::unscaled(), LocationRef::default());

    let mut glyphs = Vec::new();
    let mut total_norm = 0.0;
    for (start, scalar) in text.char_indices() {
        let end = start + scalar.len_utf8();
        let Some(gid) = charmap.map(scalar) else {
            errors.record(FontFault::MissingGlyph {
                font: font.as_u32(),
                scalar,
            });
            glyphs.push(omitted_glyph(scalar, start..end));
            continue;
        };
        let glyph_id = gid.to_u32();
        let advance_font_units = {
            let face = fonts.face_mut(font)?;
            if let Some(&cached) = face.advances.get(&glyph_id) {
                cached
            } else {
                let width =
                    glyph_metrics
                        .advance_width(gid)
                        .ok_or(Error::from(ErrorKind::Font {
                            index,
                            message: format!("no unscaled advance for glyph {glyph_id}"),
                        }))?;
                face.advances.insert(glyph_id, width);
                width
            }
        };
        let advance_norm = width_norm(advance_font_units, units_per_em);
        total_norm += advance_norm;
        glyphs.push(MeasuredGlyph {
            scalar,
            glyph_id,
            advance_font_units,
            advance_norm,
            byte_range: start..end,
            omit: false,
        });
    }

    Ok(MeasuredText {
        text: text.to_owned(),
        glyphs,
        advance_norm: total_norm,
    })
}

fn omitted_glyph(scalar: char, byte_range: Range<usize>) -> MeasuredGlyph {
    MeasuredGlyph {
        scalar,
        glyph_id: 0,
        advance_font_units: 0.0,
        advance_norm: 0.0,
        byte_range,
        omit: true,
    }
}

fn omitted_text(text: &str) -> MeasuredText {
    let glyphs = text
        .char_indices()
        .map(|(start, scalar)| omitted_glyph(scalar, start..start + scalar.len_utf8()))
        .collect();
    MeasuredText {
        text: text.to_owned(),
        glyphs,
        advance_norm: 0.0,
    }
}

/// `advance / units_per_em`. No kerning. Body text and [`FontFace::text_width`] both use this.
pub(crate) fn width_norm(advance_font_units: f32, units_per_em: f32) -> f32 {
    advance_font_units / units_per_em
}

pub(crate) fn line_metrics(
    face: &FaceMetrics,
    font_size: Length<Positive>,
    factor: f32,
) -> Result<LineMetrics> {
    if !factor.is_finite() || factor <= 0.0 {
        return Err(GeometryKind::LineHeightFactor { factor }.into());
    }
    let size = font_size.as_pt();
    let scale = size / face.units_per_em;
    let ascent = face.ascent * scale;
    let descent = face.descent * scale;
    let leading = face.leading * scale;
    let natural = ascent - descent + leading;
    let line_height = natural.max(size * factor);
    Ok(LineMetrics {
        ascent: Length::from_trusted_pt(ascent),
        descent: Length::from_trusted_pt(descent),
        leading: Length::from_trusted_pt(leading),
        line_height: Length::from_trusted_pt(line_height),
        baseline: Length::from_trusted_pt(ascent),
    })
}

impl FontFace {
    /// Advance of `text` at `size`, in pt. An empty string is `0`.
    ///
    /// # Errors
    ///
    /// [`ErrorKind::MissingGlyph`] when a scalar is not in the cmap. The `font`
    /// field is `0` because this face has no document font id yet.
    /// [`ErrorKind::Font`] when the bytes cannot be parsed.
    pub fn text_width(&self, text: &str, size: Length<Positive>) -> Result<f32> {
        if text.is_empty() {
            return Ok(0.0);
        }
        let font_ref = parse_face(&self.bytes, self.index)?;
        let metrics = face_metrics(&font_ref, self.index)?;
        let charmap = font_ref.charmap();
        let glyph_metrics = font_ref.glyph_metrics(Size::unscaled(), LocationRef::default());
        let mut total = 0.0;
        for scalar in text.chars() {
            let Some(gid) = charmap.map(scalar) else {
                return Err(Error::from(ErrorKind::MissingGlyph { font: 0, scalar }));
            };
            let units = glyph_metrics.advance_width(gid).ok_or_else(|| {
                Error::from(ErrorKind::Font {
                    index: self.index,
                    message: format!("no unscaled advance for glyph {}", gid.to_u32()),
                })
            })?;
            total += width_norm(units, metrics.units_per_em);
        }
        Ok(total * size.as_pt())
    }

    /// Natural line height at a spacing factor of 1. Body text stays at 1.2.
    ///
    /// # Errors
    ///
    /// [`ErrorKind::Font`] when the bytes cannot be parsed or the em square is 0.
    pub fn line_height(&self, size: Length<Positive>) -> Result<f32> {
        let font_ref = parse_face(&self.bytes, self.index)?;
        let face = face_metrics(&font_ref, self.index)?;
        Ok(line_metrics(&face, size, 1.0)?.line_height.as_pt())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fonts::{fixture_bytes, FontSource, FontStore};
    use crate::geom::Positive;

    fn loaded() -> Option<(FontStore, FontId)> {
        let bytes = fixture_bytes()?;
        let mut fonts = FontStore::new();
        let id = fonts.register(FontSource { bytes, index: 0 }).unwrap();
        Some((fonts, id))
    }

    #[test]
    fn unmapped_scalar_is_missing_glyph() {
        let Some((mut fonts, id)) = loaded() else {
            return;
        };
        let mut errors = ErrorBag::default();
        let measured = measure_text(&mut fonts, &mut errors, id, "A\u{FFFF}B").unwrap();
        assert!(!measured.glyphs[0].omit && !measured.glyphs[2].omit);
        assert!(measured.glyphs[1].omit);
        assert!(measured.glyphs[1].advance_norm.abs() <= f32::EPSILON);
        let found = errors.finish(()).unwrap_err();
        assert_eq!(
            found.as_slice(),
            &[Error::from(ErrorKind::MissingGlyph {
                font: id.as_u32(),
                scalar: '\u{FFFF}',
            })]
        );
    }

    #[test]
    fn measured_advances_are_positive_and_normalized() {
        let Some((mut fonts, id)) = loaded() else {
            return;
        };
        let mut errors = ErrorBag::default();
        let measured = measure_text(&mut fonts, &mut errors, id, "A").unwrap();
        assert!(errors.is_empty());
        assert_eq!(measured.glyphs.len(), 1);
        assert_eq!(measured.glyphs[0].scalar, 'A');
        assert!(measured.glyphs[0].advance_font_units > 0.0);
        assert!(measured.glyphs[0].advance_norm > 0.0);
        let upem = fonts.metrics(id).unwrap().units_per_em;
        assert!(upem > 0.0);
        assert_eq!(
            measured.glyphs[0].advance_norm.to_bits(),
            (measured.glyphs[0].advance_font_units / upem).to_bits()
        );
    }

    #[test]
    fn line_metrics_keep_negative_descent() {
        let Some((fonts, id)) = loaded() else {
            return;
        };
        let face = fonts.metrics(id).unwrap();
        assert!(face.descent < 0.0);
        let line = line_metrics(face, Positive::pt(12.0).unwrap(), 1.2).unwrap();
        assert!(line.descent.as_pt() < 0.0);
        assert!(line.line_height.as_pt() > 0.0);
        assert_eq!(
            line.baseline.as_pt().to_bits(),
            line.ascent.as_pt().to_bits()
        );
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
        assert_eq!(
            line_metrics(&face, size, 0.0).unwrap_err(),
            GeometryKind::LineHeightFactor { factor: 0.0 }.into()
        );
        assert!(matches!(
            line_metrics(&face, size, f32::NAN).unwrap_err().kind,
            ErrorKind::InvalidGeometry(GeometryKind::LineHeightFactor { factor }) if factor.is_nan()
        ));
    }
}
