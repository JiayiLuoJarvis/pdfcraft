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

    /// Line slot height at `size_pt` with `gap_pt` added after the glyph, in mm.
    fn line_height_with_gap_mm(&mut self, size_pt: f64, gap_pt: f64) -> f64;

    /// Line slot height at `size_pt` with the default [`LINE_GAP_PT`] gap, in mm.
    fn line_height_mm(&mut self, size_pt: f64) -> f64 {
        self.line_height_with_gap_mm(size_pt, LINE_GAP_PT)
    }

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
        Ok(text
            .chars()
            .map(|ch| {
                #[expect(
                    clippy::expect_used,
                    reason = "glyph_run inserts every character before this collect"
                )]
                *self
                    .glyphs
                    .get(&ch)
                    .expect("glyph cache contains this character")
            })
            .collect())
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

    /// Ascender plus descender, scaled, plus `gap_pt`.
    #[must_use]
    pub(crate) fn line_height_with_gap_mm(&self, size_pt: f64, gap_pt: f64) -> f64 {
        pt_to_mm((self.ascent + self.descent) / self.upem * size_pt + gap_pt)
    }

    /// Ascender plus descender, scaled, plus a 2 pt gap.
    #[must_use]
    pub(crate) fn line_height_mm(&self, size_pt: f64) -> f64 {
        self.line_height_with_gap_mm(size_pt, LINE_GAP_PT)
    }
}

impl Measurer for FontMetrics {
    fn string_width_mm(&mut self, text: &str, size_pt: f64) -> Result<f64, Error> {
        Self::string_width_mm(self, text, size_pt)
    }

    fn line_height_with_gap_mm(&mut self, size_pt: f64, gap_pt: f64) -> f64 {
        Self::line_height_with_gap_mm(self, size_pt, gap_pt)
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
/// Output parts concatenate back to `text`.
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
    let tokens = tokenize(text, measurer, size_pt, max_width_mm)?;
    if tokens.is_empty() {
        return Ok(vec![text.to_owned()]);
    }
    let mut widths = Vec::with_capacity(tokens.len());
    for token in &tokens {
        widths.push(measurer.string_width_mm(token, size_pt)?);
    }
    let parts = balance_tokens(&tokens, &widths, max_width_mm);
    debug_assert_eq!(
        parts.concat(),
        text,
        "wrap slices must rejoin to the source line"
    );
    Ok(parts)
}

/// Forbid a wrap after an opener, before closer/punct, or beside `·`. Adaptive-layout spec §4.2.
const WRAP_FORBID_BREAK_PENALTY: f64 = 10_000.0;
/// Last wrapped line with at most two non-whitespace characters. Adaptive-layout spec §4.2.
const WRAP_ORPHAN_LINE_PENALTY: f64 = 2_000.0;
/// Split inside a static institution suffix. Adaptive-layout spec §4.2.
const WRAP_SUFFIX_SPLIT_PENALTY: f64 = 1_500.0;

const INSTITUTION_SUFFIXES: &[&str] = &[
    "股份有限公司",
    "有限责任公司",
    "有限公司",
    "合伙企业",
    "分公司",
    "股东",
];

#[derive(Clone, Copy, PartialEq, Eq)]
enum CharClass {
    OpenBracket,
    ClosePunct,
    MiddleDot,
    Whitespace,
    Other,
}

fn classify_scalar(ch: char) -> CharClass {
    match ch {
        '(' | '（' => CharClass::OpenBracket,
        ')' | '）' | '、' | '，' | '。' => CharClass::ClosePunct,
        '·' => CharClass::MiddleDot,
        ch if ch.is_whitespace() => CharClass::Whitespace,
        _ => CharClass::Other,
    }
}

fn boundary_penalty(prev: CharClass, next: CharClass) -> f64 {
    match (prev, next) {
        (CharClass::OpenBracket | CharClass::MiddleDot, _)
        | (_, CharClass::ClosePunct | CharClass::MiddleDot) => WRAP_FORBID_BREAK_PENALTY,
        (
            CharClass::ClosePunct | CharClass::Whitespace | CharClass::Other,
            CharClass::OpenBracket | CharClass::Whitespace | CharClass::Other,
        ) => 0.0,
    }
}

fn tokenize(
    text: &str,
    measurer: &mut impl Measurer,
    size_pt: f64,
    max_width_mm: f64,
) -> Result<Vec<String>, Error> {
    let chars: Vec<char> = text.chars().collect();
    let mut tokens = Vec::new();
    let mut index = 0;
    while index < chars.len() {
        let (chunk, take) = next_chunk(&chars, index);
        if chunk.is_ascii() && take > 1 && measurer.string_width_mm(&chunk, size_pt)? > max_width_mm
        {
            for ch in chunk.chars() {
                tokens.push(ch.to_string());
            }
        } else {
            tokens.push(chunk);
        }
        index += take;
    }
    Ok(tokens)
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

fn balance_tokens(tokens: &[String], widths: &[f64], max_width_mm: f64) -> Vec<String> {
    let count = tokens.len();
    let mut prefix = vec![0.0; count + 1];
    for (index, width) in widths.iter().enumerate() {
        prefix[index + 1] = prefix[index] + width;
    }
    let line_width = |start: usize, end: usize| prefix[end] - prefix[start];
    let fits = |start: usize, end: usize| {
        end > start && (end == start + 1 || line_width(start, end) <= max_width_mm)
    };
    let line_count = min_line_count(count, fits);
    if line_count <= 1 {
        return vec![tokens.concat()];
    }
    let split_cost = split_penalties(tokens);
    let target = prefix[count] / crate::units::n64(line_count);
    let pred = wrap_predecessors(
        count,
        line_count,
        target,
        &split_cost,
        tokens,
        fits,
        line_width,
    );
    reconstruct_lines(tokens, &pred, count, line_count)
}

fn min_line_count(count: usize, fits: impl Fn(usize, usize) -> bool) -> usize {
    let mut start = 0;
    let mut lines = 0;
    while start < count {
        let mut end = start + 1;
        while end <= count && fits(start, end) {
            end += 1;
        }
        start = (end - 1).max(start + 1);
        lines += 1;
    }
    lines
}

fn split_penalties(tokens: &[String]) -> Vec<f64> {
    let split_count = tokens.len().saturating_sub(1);
    let mut cost = vec![0.0; split_count];
    for (index, window) in tokens.windows(2).enumerate() {
        let prev = window[0].chars().next_back().map(classify_scalar);
        let next = window[1].chars().next().map(classify_scalar);
        if let (Some(prev), Some(next)) = (prev, next) {
            cost[index] = boundary_penalty(prev, next);
        }
    }
    let mut suffix_split = vec![false; split_count];
    for suffix in INSTITUTION_SUFFIXES {
        mark_suffix_splits(tokens, suffix, &mut suffix_split);
    }
    for (index, marked) in suffix_split.iter().enumerate() {
        if *marked {
            cost[index] += WRAP_SUFFIX_SPLIT_PENALTY;
        }
    }
    cost
}

fn mark_suffix_splits(tokens: &[String], suffix: &str, marked: &mut [bool]) {
    if suffix.is_empty() || tokens.is_empty() {
        return;
    }
    let mut concat = String::new();
    let mut ends = Vec::with_capacity(tokens.len());
    for token in tokens {
        concat.push_str(token);
        ends.push(concat.len());
    }
    let mut from = 0;
    while from < concat.len() {
        let Some(rel) = concat[from..].find(suffix) else {
            break;
        };
        let start = from + rel;
        let end = start + suffix.len();
        for (index, boundary) in ends.iter().take(marked.len()).enumerate() {
            if start < *boundary && *boundary < end {
                marked[index] = true;
            }
        }
        from = start + suffix.chars().next().map_or(1, char::len_utf8);
    }
}

const WRAP_INFEASIBLE: f64 = 1.0e30;

fn wrap_predecessors(
    count: usize,
    line_count: usize,
    target: f64,
    split_cost: &[f64],
    tokens: &[String],
    fits: impl Fn(usize, usize) -> bool,
    line_width: impl Fn(usize, usize) -> f64,
) -> Vec<Vec<usize>> {
    let mut cost = vec![vec![WRAP_INFEASIBLE; count + 1]; line_count + 1];
    let mut pred = vec![vec![0usize; count + 1]; line_count + 1];
    cost[0][0] = 0.0;
    for lines in 1..=line_count {
        for end in 1..=count {
            for start in 0..end {
                if !fits(start, end) {
                    continue;
                }
                let mut line_cost = (line_width(start, end) - target).powi(2);
                if start > 0 {
                    line_cost += split_cost[start - 1];
                }
                if lines == line_count && end == count {
                    line_cost += last_line_orphan_penalty(&tokens[start..end]);
                }
                let total = cost[lines - 1][start] + line_cost;
                if total + 1e-9 < cost[lines][end] {
                    cost[lines][end] = total;
                    pred[lines][end] = start;
                }
            }
        }
    }
    #[expect(
        clippy::expect_used,
        reason = "greedy min-line scan proved an N-line split exists"
    )]
    let _: () = (cost[line_count][count] < WRAP_INFEASIBLE)
        .then_some(())
        .expect("greedy min-line scan proved an N-line split exists");
    pred
}

fn last_line_orphan_penalty(tokens: &[String]) -> f64 {
    let letters = tokens
        .iter()
        .flat_map(|token| token.chars())
        .filter(|ch| !ch.is_whitespace())
        .count();
    if letters <= 2 {
        WRAP_ORPHAN_LINE_PENALTY
    } else {
        0.0
    }
}

fn reconstruct_lines(
    tokens: &[String],
    pred: &[Vec<usize>],
    count: usize,
    line_count: usize,
) -> Vec<String> {
    let mut ends = Vec::with_capacity(line_count);
    let mut end = count;
    for lines in (1..=line_count).rev() {
        ends.push(end);
        end = pred[lines][end];
    }
    ends.reverse();
    let mut start = 0;
    let mut parts = Vec::with_capacity(line_count);
    for stop in ends {
        parts.push(tokens[start..stop].concat());
        start = stop;
    }
    parts
}

#[cfg(test)]
#[path = "font_tests.rs"]
mod font_tests;
