use crate::error::{font_unusable, Error, ErrorKind, FontReason, Result};
use crate::fields::{VAlign, LINE_HEIGHT_FACTOR, PAD_BOTTOM, PAD_TOP, PAD_X};
use crate::font::{Face, GlyphFault};
use crate::inspect::PdfRect;

const MIN_MP: i64 = 7_000;
const STEP_MP: i64 = 500;
pub(crate) const MAX_START_MP: i64 = 256_000;

#[derive(Clone, Copy, Debug)]
pub(crate) struct GlyphUse {
    pub(crate) id: u16,
    pub(crate) scalar: char,
    pub(crate) units: f32,
    pub(crate) y_min: f32,
    pub(crate) y_max: f32,
}

#[derive(Debug)]
struct PlacedLine {
    glyphs: Vec<GlyphUse>,
    baseline: f32,
    /// Advance width of the line in points.
    width: f32,
}

#[derive(Debug)]
pub(crate) struct LaidText {
    size: f32,
    lines: Vec<PlacedLine>,
}

enum FitError {
    Missing(char),
    NoAdvance,
    TooBig,
}

impl LaidText {
    pub(crate) fn fit(
        face: &Face<'_>,
        field_name: &str,
        text: &str,
        rect: PdfRect,
        start_size: f32,
        v_align: VAlign,
    ) -> Result<Self> {
        if text.is_empty() {
            return Ok(Self {
                size: start_size,
                lines: Vec::new(),
            });
        }
        let rounded = (f64::from(start_size) * 1000.0).round();
        if rounded < 1.0 {
            return lay_at(face, text, rect, start_size, v_align)
                .map(|laid| shrink_orphan(face, text, rect, v_align, 0, laid))
                .map_err(|err| fail(field_name, &err));
        }
        #[allow(clippy::cast_possible_truncation)] // 第 2 步已拒绝放不进 i64 的值
        let mut mp = rounded as i64;
        loop {
            #[allow(clippy::cast_precision_loss)]
            // 上限 256_000，f32 能精确表示；不要改除法来躲 lint
            let size = mp as f32 / 1000.0;
            match lay_at(face, text, rect, size, v_align) {
                Ok(laid) => return Ok(shrink_orphan(face, text, rect, v_align, mp, laid)),
                Err(FitError::TooBig) => {
                    if mp <= MIN_MP {
                        return Err(fail(field_name, &FitError::TooBig));
                    }
                    let next = mp - STEP_MP;
                    mp = if next < MIN_MP { MIN_MP } else { next };
                }
                Err(err) => return Err(fail(field_name, &err)),
            }
        }
    }

    pub(crate) fn size(&self) -> f32 {
        self.size
    }

    /// 每一行的基线、进宽（pt）、以及该行字形（空行的切片为空）。`Content::text` 用这个。
    pub(crate) fn lines(&self) -> impl Iterator<Item = (f32, f32, &[GlyphUse])> + '_ {
        self.lines
            .iter()
            .map(|line| (line.baseline, line.width, line.glyphs.as_slice()))
    }

    /// 展平所有行。只给 `record` 折叠 `used`。不要拿它画 `Tm`。
    pub(crate) fn glyphs(&self) -> impl Iterator<Item = GlyphUse> + '_ {
        self.lines
            .iter()
            .flat_map(|line| line.glyphs.iter().copied())
    }
}

fn fail(field_name: &str, err: &FitError) -> Error {
    match err {
        FitError::TooBig => ErrorKind::BoxOverflow {
            field_name: field_name.to_string(),
        }
        .into(),
        FitError::Missing(scalar) => ErrorKind::MissingGlyph {
            field_name: field_name.to_string(),
            scalar: *scalar,
        }
        .into(),
        FitError::NoAdvance => font_unusable(FontReason::NoAdvance),
    }
}

fn lay_at(
    face: &Face<'_>,
    text: &str,
    rect: PdfRect,
    size: f32,
    v_align: VAlign,
) -> std::result::Result<LaidText, FitError> {
    let max_w = rect.width() - (PAD_X * 2.0);
    let inner_h = rect.height() - PAD_TOP - PAD_BOTTOM;
    if max_w <= 0.0 || inner_h <= 0.0 || face.upem() <= 0.0 {
        return Err(FitError::TooBig);
    }
    let max_units = max_w * face.upem() / size;
    let mut built = Vec::new();
    for para in text.split('\n') {
        if para.is_empty() {
            built.push(Vec::new());
            continue;
        }
        let tokens = tokenize(face, para)?;
        if tokens.is_empty() {
            continue;
        }
        built.extend(wrap(face, &tokens, max_units)?);
    }
    place(face, rect, size, v_align, built)
}

fn fit_fault(fault: GlyphFault) -> FitError {
    match fault {
        GlyphFault::Missing(scalar) => FitError::Missing(scalar),
        GlyphFault::NoAdvance => FitError::NoAdvance,
    }
}

fn map_scalar(face: &Face<'_>, scalar: char) -> std::result::Result<GlyphUse, FitError> {
    let glyph = face.glyph(scalar).map_err(fit_fault)?;
    Ok(GlyphUse {
        id: glyph.id,
        scalar,
        units: glyph.units,
        y_min: glyph.y_min,
        y_max: glyph.y_max,
    })
}

struct Token {
    glyphs: Vec<GlyphUse>,
    units: f32,
    space_before: bool,
}

fn tokenize(face: &Face<'_>, para: &str) -> std::result::Result<Vec<Token>, FitError> {
    let mut tokens = Vec::new();
    let mut chars = para.chars().peekable();
    let mut space_before = false;
    while let Some(ch) = chars.next() {
        if ch.is_whitespace() {
            space_before = true;
            continue;
        }
        if u32::from(ch) > 0x7F {
            let glyph = map_scalar(face, ch)?;
            tokens.push(Token {
                units: glyph.units,
                glyphs: vec![glyph],
                space_before,
            });
            space_before = false;
            continue;
        }
        let mut glyphs = vec![map_scalar(face, ch)?];
        while let Some(next) = chars.peek().copied() {
            if next.is_whitespace() || u32::from(next) > 0x7F {
                break;
            }
            chars.next();
            glyphs.push(map_scalar(face, next)?);
        }
        let units = glyphs.iter().map(|glyph| glyph.units).sum();
        tokens.push(Token {
            glyphs,
            units,
            space_before,
        });
        space_before = false;
    }
    Ok(tokens)
}

fn wrap(
    face: &Face<'_>,
    tokens: &[Token],
    max_units: f32,
) -> std::result::Result<Vec<Vec<GlyphUse>>, FitError> {
    let mut lines = Vec::new();
    let mut current = Vec::new();
    let mut units = 0.0_f32;
    let mut space: Option<GlyphUse> = None;
    for token in tokens {
        if token.units > max_units {
            flush(&mut lines, &mut current);
            let pieces = hard_break(&token.glyphs, max_units)?;
            let last = pieces.len().saturating_sub(1);
            for (index, piece) in pieces.into_iter().enumerate() {
                if index == last {
                    units = piece.iter().map(|glyph| glyph.units).sum();
                    current = piece;
                } else {
                    lines.push(piece);
                }
            }
            continue;
        }
        let gap = if token.space_before && !current.is_empty() {
            cached_space(face, &mut space)?.units
        } else {
            0.0
        };
        if !current.is_empty() && units + gap + token.units > max_units {
            flush(&mut lines, &mut current);
            units = 0.0;
        }
        if token.space_before && !current.is_empty() {
            let glyph = cached_space(face, &mut space)?;
            current.push(glyph);
            units += glyph.units;
        }
        units += token.units;
        current.extend_from_slice(&token.glyphs);
    }
    flush(&mut lines, &mut current);
    Ok(lines)
}

fn cached_space(
    face: &Face<'_>,
    slot: &mut Option<GlyphUse>,
) -> std::result::Result<GlyphUse, FitError> {
    if let Some(glyph) = *slot {
        return Ok(glyph);
    }
    let glyph = map_scalar(face, ' ')?;
    *slot = Some(glyph);
    Ok(glyph)
}

fn flush(lines: &mut Vec<Vec<GlyphUse>>, current: &mut Vec<GlyphUse>) {
    if !current.is_empty() {
        lines.push(std::mem::take(current));
    }
}

fn hard_break(
    glyphs: &[GlyphUse],
    max_units: f32,
) -> std::result::Result<Vec<Vec<GlyphUse>>, FitError> {
    let mut lines = Vec::new();
    let mut current = Vec::new();
    let mut units = 0.0_f32;
    for glyph in glyphs {
        if glyph.units > max_units {
            return Err(FitError::TooBig);
        }
        if !current.is_empty() && units + glyph.units > max_units {
            lines.push(std::mem::take(&mut current));
            units = 0.0;
        }
        current.push(*glyph);
        units += glyph.units;
    }
    if !current.is_empty() {
        lines.push(current);
    }
    Ok(lines)
}

/// `(yMin, yMax)` of a line in font units. An empty line is `(0, 0)`.
fn line_ink(glyphs: &[GlyphUse]) -> (f32, f32) {
    let mut bottom = 0.0_f32;
    let mut top = 0.0_f32;
    for glyph in glyphs {
        bottom = bottom.min(glyph.y_min);
        top = top.max(glyph.y_max);
    }
    (bottom, top)
}

/// Python drops the size by up to 2 pt when the last line is a single glyph, if that
/// yields fewer lines. `chosen_mp` is the size that already fit, in millipoints.
fn shrink_orphan(
    face: &Face<'_>,
    text: &str,
    rect: PdfRect,
    v_align: VAlign,
    chosen_mp: i64,
    laid: LaidText,
) -> LaidText {
    let Some(last) = laid.lines.last() else {
        return laid;
    };
    let visible = last
        .glyphs
        .iter()
        .filter(|glyph| !glyph.scalar.is_whitespace())
        .count();
    if laid.lines.len() < 2 || visible != 1 || chosen_mp <= MIN_MP {
        return laid;
    }
    let floor = (chosen_mp - 2_000).max(MIN_MP);
    let mut probe = chosen_mp - STEP_MP;
    while probe >= floor {
        #[allow(clippy::cast_precision_loss)]
        let size = probe as f32 / 1000.0;
        if let Ok(candidate) = lay_at(face, text, rect, size, v_align) {
            if candidate.lines.len() < laid.lines.len() {
                return candidate;
            }
        }
        probe -= STEP_MP;
    }
    laid
}

fn place(
    face: &Face<'_>,
    rect: PdfRect,
    size: f32,
    v_align: VAlign,
    lines: Vec<Vec<GlyphUse>>,
) -> std::result::Result<LaidText, FitError> {
    if lines.is_empty() {
        return Ok(LaidText {
            size,
            lines: Vec::new(),
        });
    }
    let Ok(count_u) = u16::try_from(lines.len()) else {
        return Err(FitError::TooBig);
    };
    let scale = size / face.upem();
    let line_h = size * LINE_HEIGHT_FACTOR;
    let count = f32::from(count_u);
    let ink_top = line_ink(lines.first().map(Vec::as_slice).unwrap_or_default()).1;
    let ink_bottom = line_ink(lines.last().map(Vec::as_slice).unwrap_or_default()).0;
    let block = scale * (ink_top - ink_bottom) + (count - 1.0) * line_h;
    let baseline_last = match v_align {
        VAlign::Bottom => rect.y0 + PAD_BOTTOM - scale * ink_bottom,
        VAlign::Top => rect.y1 - PAD_TOP - scale * ink_top - (count - 1.0) * line_h,
        VAlign::Center => {
            let block_bottom = f32::midpoint(rect.y0, rect.y1) - block / 2.0;
            block_bottom - scale * ink_bottom
        }
    };
    let baseline_first = baseline_last + (count - 1.0) * line_h;
    let block_bottom = baseline_last + scale * ink_bottom;
    let block_top = baseline_first + scale * ink_top;
    if block_bottom < rect.y0 + PAD_BOTTOM - 0.01 || block_top > rect.y1 - PAD_TOP + 0.01 {
        return Err(FitError::TooBig);
    }
    let mut placed = Vec::with_capacity(lines.len());
    for (index, glyphs) in lines.into_iter().enumerate() {
        let Ok(index) = u16::try_from(index) else {
            return Err(FitError::TooBig);
        };
        let width_units: f32 = glyphs.iter().map(|glyph| glyph.units).sum();
        placed.push(PlacedLine {
            width: width_units * scale,
            glyphs,
            baseline: baseline_first - f32::from(index) * line_h,
        });
    }
    Ok(LaidText {
        size,
        lines: placed,
    })
}

#[cfg(test)]
mod tests {
    use super::{GlyphUse, LaidText};
    use crate::error::ErrorKind;
    use crate::fields::{VAlign, LINE_HEIGHT_FACTOR, PAD_BOTTOM, PAD_TOP};
    use crate::font::Face;
    use crate::inspect::PdfRect;

    fn with_face<T>(body: impl FnOnce(&Face<'_>) -> T) -> Option<T> {
        let path = std::env::var_os("PDFCRAFT_FORMS_FONT")?;
        let bytes = std::fs::read(path).unwrap();
        let face = Face::load(&bytes, 0).unwrap();
        Some(body(&face))
    }

    fn scalars(glyphs: &[GlyphUse]) -> Vec<char> {
        glyphs.iter().map(|glyph| glyph.scalar).collect()
    }

    fn fit<'a>(
        face: &'a Face<'a>,
        text: &str,
        rect: PdfRect,
        size: f32,
    ) -> crate::error::Result<LaidText> {
        LaidText::fit(face, "employee_name", text, rect, size, VAlign::Bottom)
    }

    #[test]
    fn wraps_ascii_on_spaces_without_keeping_the_space_glyph() {
        let Some(()) = with_face(|face| {
            let laid = fit(
                face,
                "hello world",
                PdfRect::new(0.0, 0.0, 50.0, 80.0),
                12.0,
            )
            .unwrap();
            let lines: Vec<_> = laid.lines().map(|(_, _, glyphs)| scalars(glyphs)).collect();
            assert_eq!(
                lines,
                vec![vec!['h', 'e', 'l', 'l', 'o'], vec!['w', 'o', 'r', 'l', 'd']]
            );
            assert!(laid.glyphs().all(|glyph| glyph.scalar != ' '));
            assert_eq!(laid.size().to_bits(), 12.0_f32.to_bits());
        }) else {
            return;
        };
    }

    #[test]
    fn breaks_an_overwide_ascii_word_per_glyph() {
        let Some(()) = with_face(|face| {
            let laid = fit(face, "xxxx", PdfRect::new(0.0, 0.0, 10.0, 80.0), 12.0).unwrap();
            let lines: Vec<_> = laid.lines().map(|(_, _, glyphs)| scalars(glyphs)).collect();
            assert_eq!(lines, vec![vec!['x'], vec!['x'], vec!['x'], vec!['x']]);
            assert_eq!(laid.size().to_bits(), 12.0_f32.to_bits());
        }) else {
            return;
        };
    }

    #[test]
    fn keeps_an_explicit_newline_as_two_lines() {
        let Some(()) = with_face(|face| {
            let laid = fit(face, "a\nb", PdfRect::new(0.0, 0.0, 200.0, 80.0), 12.0).unwrap();
            let lines: Vec<_> = laid.lines().collect();
            assert_eq!(scalars(lines[0].2), vec!['a']);
            assert_eq!(scalars(lines[1].2), vec!['b']);
            let gap = lines[0].0 - lines[1].0;
            assert_eq!(gap.to_bits(), (12.0 * LINE_HEIGHT_FACTOR).to_bits());
        }) else {
            return;
        };
    }

    #[test]
    fn reports_a_missing_glyph() {
        let Some(()) = with_face(|face| {
            let err = fit(face, "王", PdfRect::new(0.0, 0.0, 200.0, 80.0), 12.0).unwrap_err();
            assert_eq!(
                err.kind,
                ErrorKind::MissingGlyph {
                    field_name: "employee_name".to_string(),
                    scalar: '王',
                }
            );
        }) else {
            return;
        };
    }

    #[test]
    fn reports_box_overflow_for_five_hundred_xes() {
        let Some(()) = with_face(|face| {
            let err = fit(
                face,
                &"x".repeat(500),
                PdfRect::new(0.0, 0.0, 200.0, 30.0),
                12.0,
            )
            .unwrap_err();
            assert_eq!(
                err.kind,
                ErrorKind::BoxOverflow {
                    field_name: "employee_name".to_string(),
                }
            );
        }) else {
            return;
        };
    }

    #[test]
    fn aligns_a_single_line_to_the_bottom() {
        let Some(()) = with_face(|face| {
            let rect = PdfRect::new(0.0, 100.0, 200.0, 200.0);
            let laid = fit(face, "a", rect, 12.0).unwrap();
            let (baseline, _, glyphs) = laid.lines().next().unwrap();
            let scale = laid.size() / face.upem();
            let ink_bottom = glyphs
                .iter()
                .map(|glyph| glyph.y_min)
                .fold(0.0_f32, f32::min);
            let expected = rect.y0 + PAD_BOTTOM - scale * ink_bottom;
            assert_eq!(baseline.to_bits(), expected.to_bits());
        }) else {
            return;
        };
    }

    #[test]
    fn aligns_a_single_line_to_the_top() {
        let Some(()) = with_face(|face| {
            let rect = PdfRect::new(0.0, 100.0, 200.0, 200.0);
            let laid = LaidText::fit(face, "employee_name", "a", rect, 12.0, VAlign::Top).unwrap();
            let (baseline, _, glyphs) = laid.lines().next().unwrap();
            let scale = laid.size() / face.upem();
            let ink_top = glyphs
                .iter()
                .map(|glyph| glyph.y_max)
                .fold(0.0_f32, f32::max);
            let expected = rect.y1 - PAD_TOP - scale * ink_top;
            assert_eq!(baseline.to_bits(), expected.to_bits());
        }) else {
            return;
        };
    }

    #[test]
    fn aligns_a_single_line_to_the_vertical_center() {
        let Some(()) = with_face(|face| {
            let rect = PdfRect::new(0.0, 100.0, 200.0, 200.0);
            let laid =
                LaidText::fit(face, "employee_name", "a", rect, 12.0, VAlign::Center).unwrap();
            let (baseline, _, glyphs) = laid.lines().next().unwrap();
            let scale = laid.size() / face.upem();
            let ink_bottom = glyphs
                .iter()
                .map(|glyph| glyph.y_min)
                .fold(0.0_f32, f32::min);
            let ink_top = glyphs
                .iter()
                .map(|glyph| glyph.y_max)
                .fold(0.0_f32, f32::max);
            let block = scale * (ink_top - ink_bottom);
            let block_bottom = f32::midpoint(rect.y0, rect.y1) - block / 2.0;
            let expected = block_bottom - scale * ink_bottom;
            assert_eq!(baseline.to_bits(), expected.to_bits());
        }) else {
            return;
        };
    }

    #[test]
    fn does_not_record_a_space_for_a_newline() {
        let Some(()) = with_face(|face| {
            let laid = fit(face, "a\nb", PdfRect::new(0.0, 0.0, 200.0, 80.0), 12.0).unwrap();
            assert!(laid
                .glyphs()
                .all(|glyph| glyph.scalar != ' ' && glyph.id != 3));
        }) else {
            return;
        };
    }

    #[test]
    fn records_a_space_when_a_gap_is_drawn() {
        let Some(()) = with_face(|face| {
            let laid = fit(face, "a b", PdfRect::new(0.0, 0.0, 200.0, 80.0), 12.0).unwrap();
            assert!(laid.glyphs().any(|glyph| glyph.scalar == ' '));
        }) else {
            return;
        };
    }

    #[test]
    fn folds_nbsp_and_ideographic_space_into_u0020() {
        let Some(()) = with_face(|face| {
            for text in ["a\u{00A0}b", "a\u{3000}b"] {
                let laid = fit(face, text, PdfRect::new(0.0, 0.0, 200.0, 80.0), 12.0).unwrap();
                let scalars: Vec<char> = laid.glyphs().map(|glyph| glyph.scalar).collect();
                assert_eq!(scalars, vec!['a', ' ', 'b']);
            }
        }) else {
            return;
        };
    }

    #[test]
    fn starts_the_search_at_the_caller_size() {
        let Some(()) = with_face(|face| {
            let wide = PdfRect::new(0.0, 0.0, 200.0, 80.0);
            let laid = fit(face, "a", wide, 10.25).unwrap();
            assert_eq!(laid.size().to_bits(), 10.25_f32.to_bits());

            let short = PdfRect::new(0.0, 0.0, 200.0, 11.0);
            let laid = fit(face, "a", short, 10.25).unwrap();
            #[allow(clippy::cast_possible_truncation)] // 选中的字号不超过 10.25 pt
            let mp = (f64::from(laid.size()) * 1000.0).round() as i64;
            assert!(mp <= 10_250);
            assert!(mp >= 7_000);
            assert!(mp == 7_000 || (10_250 - mp) % 500 == 0);
        }) else {
            return;
        };
    }

    #[test]
    fn tries_a_sub_millipoint_size_once() {
        let Some(()) = with_face(|face| {
            let err = fit(face, "a", PdfRect::new(0.0, 0.0, 0.0, 40.0), 0.0001).unwrap_err();
            assert_eq!(
                err.kind,
                ErrorKind::BoxOverflow {
                    field_name: "employee_name".to_string(),
                }
            );
            let laid = fit(face, "a", PdfRect::new(0.0, 0.0, 200.0, 80.0), 0.0001).unwrap();
            assert_eq!(laid.size().to_bits(), 0.0001_f32.to_bits());
            assert_ne!(laid.size().to_bits(), 0.0_f32.to_bits());
            assert_ne!(laid.size().to_bits(), 7.0_f32.to_bits());
        }) else {
            return;
        };
    }
}
