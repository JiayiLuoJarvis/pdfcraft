//! A parsed face, glyph advances, and text wrapping.
//!
//! Width and a later draw step both go through [`FontMetrics::glyph_run`].
//! Line height uses the hhea ascender and descender, plus a 2 pt gap.
//! The glyph cache lives on [`FontMetrics`], which each draw builds from [`Font`].

use std::collections::HashMap;
use std::fmt;
use std::sync::Arc;

use read_fonts::TableProvider;
use skrifa::instance::{LocationRef, Size};
use skrifa::{FontRef, MetadataProvider};

use crate::error::{Error, ErrorCode, ErrorSubject};
use crate::units::{pt_to_mm, LINE_GAP_PT};

#[cfg(test)]
mod fixed;

#[cfg(test)]
pub(crate) use fixed::FallbackMeasurer;

/// One TrueType or OpenType face, parsed once and shared.
///
/// The buffer is an `Arc` so cloning a [`Font`] does not copy the bytes.
/// Paths are not accepted. There is no [`PartialEq`]: two faces are not
/// compared by bytes.
#[derive(Clone)]
pub struct Font {
    data: Arc<Vec<u8>>,
    upem: f64,
    ascent: f64,
    /// Positive distance below the baseline. `FontMetrics` uses the same sign.
    descent: f64,
}

impl Font {
    /// Parse one face from owned bytes.
    ///
    /// Runs the same checks as opening a face for measurement: reject a `ttcf`
    /// collection, reject empty bytes, read `head` and `hhea`, require a finite
    /// units-per-em greater than zero, a positive ascent, and a negative descent.
    /// A missing glyph advance is reported later, from [`crate::Chart::to_pdf`].
    ///
    /// # Errors
    ///
    /// Returns [`ErrorCode::FontError`](crate::ErrorCode::FontError) when the
    /// bytes are not one usable face.
    pub fn new(data: impl Into<Vec<u8>>) -> Result<Self, Error> {
        Self::from_shared(Arc::new(data.into()))
    }

    /// Parse one face from a shared buffer, without copying its contents.
    ///
    /// # Errors
    ///
    /// Returns [`ErrorCode::FontError`](crate::ErrorCode::FontError) when the
    /// bytes are not one usable face. See [`Font::new`].
    pub fn from_shared(data: Arc<Vec<u8>>) -> Result<Self, Error> {
        let metrics = face_metrics(&data)?;
        Ok(Self {
            data,
            upem: metrics.upem,
            ascent: metrics.ascent,
            descent: metrics.descent,
        })
    }
}

impl fmt::Debug for Font {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Font")
            .field("bytes", &self.data.len())
            .field("units_per_em", &self.upem)
            .finish_non_exhaustive()
    }
}

/// Width, line height, and glyph coverage for wrapping.
pub(crate) trait Measurer {
    /// Width of `text` at `size_pt`, in mm.
    ///
    /// # Errors
    ///
    /// A real font returns [`ErrorCode::MissingGlyph`] when `text` needs a glyph it lacks.
    fn string_width_mm(&mut self, text: &str, size_pt: f64) -> Result<f64, Error>;

    /// Line slot height at `size_pt`, in mm.
    fn line_height_mm(&mut self, size_pt: f64) -> f64;

    /// Reject text that the font cannot draw.
    ///
    /// # Errors
    ///
    /// Returns [`ErrorCode::MissingGlyph`] when a character has no glyph.
    fn ensure_glyphs(&mut self, text: &str) -> Result<(), Error>;
}

/// One glyph produced by the same charmap and advance path drawing will use.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct GlyphUse {
    /// Glyph id in the face.
    pub glyph_id: u32,
    /// Advance in font units, before size scaling.
    pub advance_units: f32,
}

/// One face, opened from memory.
#[derive(Debug, Clone)]
pub(crate) struct FontMetrics {
    bytes: Arc<Vec<u8>>,
    upem: f64,
    ascent: f64,
    descent: f64,
    glyphs: HashMap<char, GlyphUse>,
}

impl FontMetrics {
    /// Measurement state for one draw. The glyph cache starts empty.
    #[must_use]
    pub(crate) fn from_font(font: &Font) -> Self {
        Self {
            bytes: Arc::clone(&font.data),
            upem: font.upem,
            ascent: font.ascent,
            descent: font.descent,
            glyphs: HashMap::new(),
        }
    }

    /// Charmap hits and unscaled advances for `text`.
    ///
    /// Uncached characters parse the face once. Later calls reuse those glyph ids.
    ///
    /// # Errors
    ///
    /// Returns [`ErrorCode::MissingGlyph`] or [`ErrorCode::FontError`].
    pub(crate) fn glyph_run(&mut self, text: &str) -> Result<Vec<GlyphUse>, Error> {
        if text.chars().any(|ch| !self.glyphs.contains_key(&ch)) {
            let face = parse_face(&self.bytes)?;
            let charmap = face.charmap();
            let glyph_metrics = face.glyph_metrics(Size::unscaled(), LocationRef::default());
            for ch in text.chars() {
                if self.glyphs.contains_key(&ch) {
                    continue;
                }
                let Some(gid) = charmap.map(ch) else {
                    return Err(missing_glyph(ch));
                };
                let advance_units = glyph_metrics.advance_width(gid).ok_or_else(|| {
                    Error::new(
                        ErrorCode::FontError,
                        format!("no advance for glyph {}", gid.to_u32()),
                    )
                })?;
                self.glyphs.insert(
                    ch,
                    GlyphUse {
                        glyph_id: gid.to_u32(),
                        advance_units,
                    },
                );
            }
        }
        Ok(text.chars().map(|ch| self.glyphs[&ch]).collect())
    }

    /// Sum of advances at `size_pt`, in mm.
    ///
    /// # Errors
    ///
    /// Returns [`ErrorCode::MissingGlyph`] when `text` is not covered.
    pub(crate) fn string_width_mm(&mut self, text: &str, size_pt: f64) -> Result<f64, Error> {
        let units: f64 = self
            .glyph_run(text)?
            .iter()
            .map(|glyph| f64::from(glyph.advance_units))
            .sum();
        Ok(pt_to_mm(units / self.upem * size_pt))
    }

    /// Distance from the baseline to the ascender, in mm.
    #[must_use]
    pub(crate) fn ascent_mm(&self, size_pt: f64) -> f64 {
        pt_to_mm(self.ascent / self.upem * size_pt)
    }

    /// Distance from the baseline down to the descender, in mm.
    ///
    /// The value is positive. It does not include the 2 pt line gap.
    #[must_use]
    pub(crate) fn descent_mm(&self, size_pt: f64) -> f64 {
        pt_to_mm(self.descent / self.upem * size_pt)
    }

    /// Face bytes passed to krilla. The same buffer skrifa measured.
    #[must_use]
    pub(crate) fn bytes(&self) -> &Arc<Vec<u8>> {
        &self.bytes
    }

    /// Units per em from `head`.
    #[must_use]
    pub(crate) fn units_per_em(&self) -> f64 {
        self.upem
    }

    /// Ascender plus descender, scaled, plus a 2 pt gap.
    #[must_use]
    pub(crate) fn line_height_mm(&self, size_pt: f64) -> f64 {
        pt_to_mm((self.ascent + self.descent) / self.upem * size_pt + LINE_GAP_PT)
    }
}

impl Measurer for FontMetrics {
    fn string_width_mm(&mut self, text: &str, size_pt: f64) -> Result<f64, Error> {
        Self::string_width_mm(self, text, size_pt)
    }

    fn line_height_mm(&mut self, size_pt: f64) -> f64 {
        Self::line_height_mm(self, size_pt)
    }

    fn ensure_glyphs(&mut self, text: &str) -> Result<(), Error> {
        self.glyph_run(text).map(|_| ())
    }
}

struct FaceMetrics {
    upem: f64,
    ascent: f64,
    descent: f64,
}

fn face_metrics(bytes: &[u8]) -> Result<FaceMetrics, Error> {
    if bytes.starts_with(b"ttcf") {
        return Err(rejected("TTC not supported"));
    }
    if bytes.is_empty() {
        return Err(rejected("font bytes are empty"));
    }
    let face = parse_face(bytes)?;
    let head = face
        .head()
        .map_err(|error| font_cause("failed to read head", error))?;
    let hhea = face
        .hhea()
        .map_err(|error| font_cause("failed to read hhea", error))?;
    let upem = f64::from(head.units_per_em());
    let ascent = f64::from(hhea.ascender().to_i16());
    let descent = f64::from(hhea.descender().to_i16());
    if !(upem.is_finite() && upem > 0.0) {
        return Err(rejected(format!(
            "head.unitsPerEm={upem} must be finite and > 0"
        )));
    }
    if !(ascent.is_finite() && ascent > 0.0 && descent.is_finite() && descent < 0.0) {
        return Err(rejected(format!(
            "hhea ascent={ascent} descent={descent} must be finite, ascent > 0 and descent < 0"
        )));
    }
    Ok(FaceMetrics {
        upem,
        ascent,
        descent: -descent,
    })
}

fn parse_face(bytes: &[u8]) -> Result<FontRef<'_>, Error> {
    FontRef::from_index(bytes, 0).map_err(|error| font_cause("failed to load font", error))
}

/// Our own face check. The detail is [`std::error::Error::source`], not the message.
fn rejected(detail: impl Into<String>) -> Error {
    Error::new(ErrorCode::FontError, "font bytes are not one usable face").with_source(
        FaceRejected {
            detail: detail.into(),
        },
    )
}

fn font_cause(message: &str, source: impl std::error::Error + Send + Sync + 'static) -> Error {
    Error::new(ErrorCode::FontError, message).with_source(source)
}

#[derive(Debug)]
struct FaceRejected {
    detail: String,
}

impl fmt::Display for FaceRejected {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.detail)
    }
}

impl std::error::Error for FaceRejected {}

fn missing_glyph(ch: char) -> Error {
    Error::new(ErrorCode::MissingGlyph, format!("missing glyph {ch}"))
        .with_subject(ErrorSubject::Character(ch))
}

/// Wrap `text`. ASCII runs stay together until a space. Other characters break one by one.
///
/// `allow_wrap == false` returns the whole string even when it is wider than `max_width_mm`.
///
/// # Errors
///
/// Returns the measurer's glyph or font error.
pub(crate) fn wrap_text(
    text: &str,
    measurer: &mut impl Measurer,
    size_pt: f64,
    max_width_mm: f64,
    allow_wrap: bool,
) -> Result<Vec<String>, Error> {
    measurer.ensure_glyphs(text)?;
    if !allow_wrap || measurer.string_width_mm(text, size_pt)? <= max_width_mm {
        return Ok(vec![text.to_owned()]);
    }
    let chars: Vec<char> = text.chars().collect();
    let mut out = Vec::new();
    let mut current = String::new();
    let mut index = 0;
    while index < chars.len() {
        let (mut candidate, mut take) = next_chunk(&chars, index);
        if measurer.string_width_mm(&candidate, size_pt)? > max_width_mm {
            candidate = chars[index].to_string();
            take = 1;
        }
        let combined = format!("{current}{candidate}");
        if measurer.string_width_mm(&combined, size_pt)? <= max_width_mm {
            current = combined;
            index += take;
        } else if !current.is_empty() {
            out.push(std::mem::take(&mut current));
        } else {
            out.push(candidate);
            index += take;
        }
    }
    if !current.is_empty() {
        out.push(current);
    }
    Ok(out)
}

fn next_chunk(chars: &[char], index: usize) -> (String, usize) {
    if chars[index].is_ascii() {
        let mut end = chars.len();
        for (offset, ch) in chars[index..].iter().enumerate() {
            if !ch.is_ascii() || *ch == ' ' {
                end = index + offset;
                break;
            }
        }
        if end < chars.len() && chars[end] == ' ' {
            end += 1;
        }
        let end = end.max(index + 1);
        (chars[index..end].iter().collect(), end - index)
    } else {
        (chars[index].to_string(), 1)
    }
}

#[cfg(test)]
#[path = "font_tests.rs"]
mod font_tests;
