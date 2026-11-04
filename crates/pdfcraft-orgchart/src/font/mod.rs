//! Font bytes, glyph advances, and text wrapping.
//!
//! Width and a later draw step both go through [`FontMetrics::glyph_run`].
//! Line height uses the hhea ascender and descender, plus a 2 pt gap.

use std::collections::HashMap;
use std::sync::Arc;

use read_fonts::TableProvider;
use skrifa::instance::{LocationRef, Size};
use skrifa::{FontRef, MetadataProvider};

use crate::error::{Error, ErrorCode};
use crate::units::{pt_to_mm, LINE_GAP_PT};

#[cfg(test)]
mod fixed;

#[cfg(test)]
pub(crate) use fixed::FallbackMeasurer;

/// Font file bytes. Paths are not accepted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FontBytes {
    /// Shared TrueType or OpenType bytes for one face.
    pub data: Arc<Vec<u8>>,
}

impl FontBytes {
    /// Wrap owned bytes without parsing them.
    #[must_use]
    pub fn new(data: impl Into<Vec<u8>>) -> Self {
        Self {
            data: Arc::new(data.into()),
        }
    }

    /// Reuse an existing shared font buffer without copying its contents.
    #[must_use]
    pub fn from_shared(data: Arc<Vec<u8>>) -> Self {
        Self { data }
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
    /// Open one TrueType or OpenType face.
    ///
    /// A `ttcf` collection is [`ErrorCode::FontError`]. So is a face whose hhea
    /// ascent is not positive or whose hhea descent is not negative.
    ///
    /// # Errors
    ///
    /// Returns [`ErrorCode::FontError`] when the bytes are not one usable face.
    pub(crate) fn open(font: &FontBytes) -> Result<Self, Error> {
        if font.data.starts_with(b"ttcf") {
            return Err(Error::new(ErrorCode::FontError, "TTC not supported"));
        }
        if font.data.is_empty() {
            return Err(Error::new(ErrorCode::FontError, "font bytes are empty"));
        }
        let face = parse_face(&font.data)?;
        let head = face.head().map_err(|error| {
            Error::new(
                ErrorCode::FontError,
                format!("failed to read head: {error}"),
            )
        })?;
        let hhea = face.hhea().map_err(|error| {
            Error::new(
                ErrorCode::FontError,
                format!("failed to read hhea: {error}"),
            )
        })?;
        let upem = f64::from(head.units_per_em());
        let ascent = f64::from(hhea.ascender().to_i16());
        let descent = f64::from(hhea.descender().to_i16());
        if !(upem.is_finite() && upem > 0.0) {
            return Err(Error::new(
                ErrorCode::FontError,
                format!("head.unitsPerEm={upem} must be finite and > 0"),
            ));
        }
        if !(ascent.is_finite() && ascent > 0.0 && descent.is_finite() && descent < 0.0) {
            return Err(Error::new(
                ErrorCode::FontError,
                format!(
                    "hhea ascent={ascent} descent={descent} must be finite, ascent > 0 and descent < 0"
                ),
            ));
        }
        let bytes = Arc::clone(&font.data);
        Ok(Self {
            bytes,
            upem,
            ascent,
            descent: -descent,
            glyphs: HashMap::new(),
        })
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
                    return Err(missing_glyph(text));
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

fn parse_face(bytes: &[u8]) -> Result<FontRef<'_>, Error> {
    FontRef::from_index(bytes, 0).map_err(|error| {
        Error::new(
            ErrorCode::FontError,
            format!("failed to load font: {error}"),
        )
    })
}

fn missing_glyph(text: &str) -> Error {
    let prefix: String = text.chars().take(20).collect();
    Error::new(ErrorCode::MissingGlyph, format!("missing glyph: {prefix}"))
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
