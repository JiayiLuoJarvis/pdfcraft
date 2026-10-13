use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write;

use lopdf::{Dictionary, Document, Object};

use crate::error::{Error, ErrorKind, FlattenReason, FontFault, Result};
use crate::fields::{
    FONT_SIZE_STEP, LINE_HEIGHT_FACTOR, MIN_FONT_SIZE, PAD_BOTTOM, PAD_TOP, PAD_X,
};
use crate::font::{self, Face, Glyph, GlyphFault};
use crate::inspect::{self, PdfRect};

pub(crate) struct LaidText {
    size: f32,
    lines: Vec<PlacedLine>,
}

struct PlacedLine {
    glyphs: Vec<u16>,
    baseline: f32,
}

pub(crate) enum Mark {
    Text(LaidText),
    Check,
}

pub(crate) struct Stamp {
    pub(crate) page_index: usize,
    pub(crate) rect: PdfRect,
    pub(crate) mark: Mark,
}

enum FitError {
    Missing(char),
    NoAdvance,
    TooBig,
}

pub(crate) fn lay_text(
    face: &Face<'_>,
    field_name: &str,
    text: &str,
    rect: PdfRect,
    start_size: f32,
    used: &mut BTreeMap<u16, (char, f32)>,
) -> Result<LaidText> {
    if text.is_empty() {
        return Ok(LaidText {
            size: start_size,
            lines: Vec::new(),
        });
    }
    let mut size = start_size;
    loop {
        match lay_at(face, text, rect, size) {
            Ok(laid) => {
                remember(face, field_name, text, used)?;
                return Ok(laid);
            }
            Err(FitError::Missing(scalar)) => {
                return Err(ErrorKind::MissingGlyph {
                    field_name: field_name.to_string(),
                    scalar,
                }
                .into());
            }
            Err(FitError::NoAdvance) => {
                return Err(
                    Error::from(ErrorKind::FontNotConfigured).with_source(FontFault::NoAdvance)
                );
            }
            Err(FitError::TooBig) => {
                if size <= MIN_FONT_SIZE + 0.001 {
                    return Err(ErrorKind::BoxOverflow {
                        field_name: field_name.to_string(),
                    }
                    .into());
                }
                let next = size - FONT_SIZE_STEP;
                size = if next < MIN_FONT_SIZE {
                    MIN_FONT_SIZE
                } else {
                    next
                };
            }
        }
    }
}

fn remember(
    face: &Face<'_>,
    field_name: &str,
    text: &str,
    used: &mut BTreeMap<u16, (char, f32)>,
) -> Result<()> {
    for scalar in text.chars() {
        if scalar == '\n' || scalar.is_whitespace() {
            continue;
        }
        let glyph = map_glyph(face.glyph(scalar), field_name)?;
        used.entry(glyph.id).or_insert((scalar, glyph.units));
    }
    if text.chars().any(char::is_whitespace) {
        let space = map_glyph(face.glyph(' '), field_name)?;
        used.entry(space.id).or_insert((' ', space.units));
    }
    Ok(())
}

fn map_glyph(glyph: std::result::Result<Glyph, GlyphFault>, field_name: &str) -> Result<Glyph> {
    match glyph {
        Ok(glyph) => Ok(glyph),
        Err(GlyphFault::Missing(scalar)) => Err(ErrorKind::MissingGlyph {
            field_name: field_name.to_string(),
            scalar,
        }
        .into()),
        Err(GlyphFault::NoAdvance) => {
            Err(Error::from(ErrorKind::FontNotConfigured).with_source(FontFault::NoAdvance))
        }
    }
}

fn lay_at(
    face: &Face<'_>,
    text: &str,
    rect: PdfRect,
    size: f32,
) -> std::result::Result<LaidText, FitError> {
    let max_w = rect.width() - (PAD_X * 2.0);
    let inner_h = rect.height() - PAD_TOP - PAD_BOTTOM;
    if max_w <= 0.0 || inner_h <= 0.0 || face.upem() <= 0.0 {
        return Err(FitError::TooBig);
    }
    let max_units = max_w * face.upem() / size;
    let space = face.glyph(' ').map_err(fit_fault)?;
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
        built.extend(wrap(&tokens, space, max_units)?);
    }
    place(face, rect, size, built)
}

fn fit_fault(fault: GlyphFault) -> FitError {
    match fault {
        GlyphFault::Missing(scalar) => FitError::Missing(scalar),
        GlyphFault::NoAdvance => FitError::NoAdvance,
    }
}

struct Token {
    glyphs: Vec<Glyph>,
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
            let glyph = face.glyph(ch).map_err(fit_fault)?;
            tokens.push(Token {
                units: glyph.units,
                glyphs: vec![glyph],
                space_before,
            });
            space_before = false;
            continue;
        }
        let mut glyphs = vec![face.glyph(ch).map_err(fit_fault)?];
        while let Some(next) = chars.peek().copied() {
            if next.is_whitespace() || u32::from(next) > 0x7F {
                break;
            }
            chars.next();
            glyphs.push(face.glyph(next).map_err(fit_fault)?);
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
    tokens: &[Token],
    space: Glyph,
    max_units: f32,
) -> std::result::Result<Vec<Vec<Glyph>>, FitError> {
    let mut lines = Vec::new();
    let mut current = Vec::new();
    let mut units = 0.0_f32;
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
            space.units
        } else {
            0.0
        };
        if !current.is_empty() && units + gap + token.units > max_units {
            flush(&mut lines, &mut current);
            units = 0.0;
        }
        if token.space_before && !current.is_empty() {
            current.push(space);
            units += space.units;
        }
        units += token.units;
        current.extend_from_slice(&token.glyphs);
    }
    flush(&mut lines, &mut current);
    Ok(lines)
}

fn flush(lines: &mut Vec<Vec<Glyph>>, current: &mut Vec<Glyph>) {
    if !current.is_empty() {
        lines.push(std::mem::take(current));
    }
}

fn hard_break(glyphs: &[Glyph], max_units: f32) -> std::result::Result<Vec<Vec<Glyph>>, FitError> {
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

fn place(
    face: &Face<'_>,
    rect: PdfRect,
    size: f32,
    lines: Vec<Vec<Glyph>>,
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
    let ascent = face.ascent() * scale;
    let descent = face.descent() * scale;
    let line_h = size * LINE_HEIGHT_FACTOR;
    let count = f32::from(count_u);
    let block = (ascent - descent) + (count - 1.0) * line_h;
    let inner_h = rect.height() - PAD_TOP - PAD_BOTTOM;
    if block > inner_h + 0.05 {
        return Err(FitError::TooBig);
    }
    let baseline_last = rect.y0 + PAD_BOTTOM - descent;
    let baseline_first = baseline_last + (count - 1.0) * line_h;
    if baseline_first + ascent > rect.y1 - PAD_TOP + 0.05 {
        return Err(FitError::TooBig);
    }
    let mut placed = Vec::with_capacity(lines.len());
    for (index, glyphs) in lines.into_iter().enumerate() {
        let Ok(index) = u16::try_from(index) else {
            return Err(FitError::TooBig);
        };
        placed.push(PlacedLine {
            glyphs: glyphs.into_iter().map(|glyph| glyph.id).collect(),
            baseline: baseline_first - f32::from(index) * line_h,
        });
    }
    Ok(LaidText {
        size,
        lines: placed,
    })
}

pub(crate) fn paint(
    doc: &mut Document,
    stamps: &[Stamp],
    font_bytes: Option<&[u8]>,
    used: &BTreeMap<u16, (char, f32)>,
    face: Option<&Face<'_>>,
) -> Result<()> {
    let font_id = match (face, font_bytes) {
        (Some(face), Some(bytes)) if !used.is_empty() => Some(font::embed(
            doc,
            bytes,
            used,
            face.upem(),
            face.ascent(),
            face.descent(),
        )?),
        _ => None,
    };
    let pages: Vec<_> = doc.page_iter().collect();
    let mut by_page: BTreeMap<usize, Vec<&Stamp>> = BTreeMap::new();
    for stamp in stamps {
        by_page.entry(stamp.page_index).or_default().push(stamp);
    }
    for (index, page_stamps) in by_page {
        let page_id = pages.get(index).copied().ok_or_else(|| {
            Error::from(ErrorKind::FlattenFailed {
                reason: FlattenReason::PageOutOfRange { page_index: index },
            })
        })?;
        let needs_font = page_stamps.iter().any(|stamp| {
            matches!(&stamp.mark, Mark::Text(text) if text.lines.iter().any(|line| !line.glyphs.is_empty()))
        });
        let font_name = if needs_font {
            let font_id = font_id.ok_or_else(|| {
                Error::from(ErrorKind::FontNotConfigured).with_source(FontFault::Missing)
            })?;
            Some(attach_font(doc, page_id, font_id)?)
        } else {
            None
        };
        let content = page_content(&page_stamps, font_name.as_deref())?;
        doc.add_page_contents(page_id, content.into_bytes())
            .map_err(|err| {
                Error::from(ErrorKind::FlattenFailed {
                    reason: FlattenReason::AppendContent,
                })
                .with_source(err)
            })?;
    }
    strip_form(doc)?;
    prune(doc);
    Ok(())
}

fn page_content(stamps: &[&Stamp], font_name: Option<&str>) -> Result<String> {
    let mut out = String::new();
    for stamp in stamps {
        match &stamp.mark {
            Mark::Check => out.push_str(&check_mark(stamp.rect)?),
            Mark::Text(text) => {
                if let Some(font_name) = font_name {
                    out.push_str(&text_block(stamp.rect, text, font_name)?);
                }
            }
        }
    }
    Ok(out)
}

fn text_block(rect: PdfRect, text: &LaidText, font_name: &str) -> Result<String> {
    let mut out = clip(rect)?;
    out.push_str("0 0 0 rg\nBT\n");
    let x = rect.x0 + PAD_X;
    for line in &text.lines {
        if line.glyphs.is_empty() {
            continue;
        }
        let _ = writeln!(out, "/{font_name} {} Tf", pdf_num(text.size)?);
        let _ = write!(
            out,
            "1 0 0 1 {} {} Tm\n<",
            pdf_num(x)?,
            pdf_num(line.baseline)?
        );
        for gid in &line.glyphs {
            let _ = write!(out, "{gid:04X}");
        }
        out.push_str("> Tj\n");
    }
    out.push_str("ET\nQ\n");
    Ok(out)
}

fn check_mark(rect: PdfRect) -> Result<String> {
    let mark = rect.width().min(rect.height()) * 0.7;
    let mid_x = f32::midpoint(rect.x0, rect.x1);
    let mid_y = f32::midpoint(rect.y0, rect.y1);
    let left_x = mid_x - mark * 0.32;
    let joint_x = mid_x - mark * 0.05;
    let right_x = mid_x + mark * 0.34;
    let left_y = mid_y + mark * 0.02;
    let joint_y = mid_y - mark * 0.28;
    let right_y = mid_y + mark * 0.32;
    let stroke = (mark * 0.14).max(0.8);
    let mut out = clip(rect)?;
    let _ = write!(
        out,
        "{} w\n1 J\n1 j\n{} {} m\n{} {} l\n{} {} l\nS\nQ\n",
        pdf_num(stroke)?,
        pdf_num(left_x)?,
        pdf_num(left_y)?,
        pdf_num(joint_x)?,
        pdf_num(joint_y)?,
        pdf_num(right_x)?,
        pdf_num(right_y)?,
    );
    Ok(out)
}

fn clip(rect: PdfRect) -> Result<String> {
    Ok(format!(
        "q\n{} {} {} {} re\nW\nn\n",
        pdf_num(rect.x0)?,
        pdf_num(rect.y0)?,
        pdf_num(rect.width())?,
        pdf_num(rect.height())?
    ))
}

fn pdf_num(value: f32) -> Result<String> {
    let scaled = (value * 1000.0).round() / 1000.0;
    if !scaled.is_finite() {
        return Err(ErrorKind::FlattenFailed {
            reason: FlattenReason::NonFinite,
        }
        .into());
    }
    let mut text = format!("{scaled:.3}");
    if text.contains('.') {
        while text.ends_with('0') {
            text.pop();
        }
        if text.ends_with('.') {
            text.pop();
        }
    }
    if text == "-0" {
        Ok("0".to_string())
    } else {
        Ok(text)
    }
}

/// A page `Resources` dictionary replaces an inherited one, so the copy keeps the old keys.
fn attach_font(
    doc: &mut Document,
    page_id: lopdf::ObjectId,
    font_id: lopdf::ObjectId,
) -> Result<String> {
    let mut resources = copied_resources(doc, page_id)?;
    let mut fonts = match resources.get(b"Font").ok().cloned() {
        Some(obj) => match inspect::deep_deref(doc, obj) {
            Object::Dictionary(dict) => dict,
            _ => Dictionary::new(),
        },
        None => Dictionary::new(),
    };
    let resource_name = fresh_font_name(&fonts)?;
    fonts.set(resource_name.as_str(), Object::Reference(font_id));
    resources.set("Font", Object::Dictionary(fonts));
    let resources_id = doc.add_object(Object::Dictionary(resources));
    let page = doc
        .get_object_mut(page_id)
        .and_then(Object::as_dict_mut)
        .map_err(|err| {
            Error::from(ErrorKind::FlattenFailed {
                reason: FlattenReason::UpdateResources,
            })
            .with_source(err)
        })?;
    page.set("Resources", Object::Reference(resources_id));
    Ok(resource_name)
}

fn fresh_font_name(fonts: &Dictionary) -> Result<String> {
    for n in 0..64 {
        let candidate = if n == 0 {
            "QuireForms".to_string()
        } else {
            format!("QuireForms{}", n + 1)
        };
        if !fonts.has(candidate.as_bytes()) {
            return Ok(candidate);
        }
    }
    Err(ErrorKind::FlattenFailed {
        reason: FlattenReason::FontNameTaken,
    }
    .into())
}

fn copied_resources(doc: &Document, page_id: lopdf::ObjectId) -> Result<Dictionary> {
    let mut id = page_id;
    for _ in 0..32 {
        let found = {
            let dict = doc.get_dictionary(id).map_err(|err| {
                Error::from(ErrorKind::FlattenFailed {
                    reason: FlattenReason::ReadPage,
                })
                .with_source(err)
            })?;
            let resources = dict.get(b"Resources").ok().cloned();
            let parent = dict.get(b"Parent").and_then(Object::as_reference).ok();
            (resources, parent)
        };
        if let Some(resources) = found.0 {
            return match inspect::deep_deref(doc, resources) {
                Object::Dictionary(dict) => Ok(dict),
                _ => Err(ErrorKind::FlattenFailed {
                    reason: FlattenReason::ResourcesNotDictionary,
                }
                .into()),
            };
        }
        let Some(parent) = found.1 else {
            break;
        };
        id = parent;
    }
    Ok(Dictionary::new())
}

fn strip_form(doc: &mut Document) -> Result<()> {
    if let Ok(catalog) = doc.catalog_mut() {
        catalog.remove(b"AcroForm");
    }
    let pages: Vec<_> = doc.page_iter().collect();
    for page_id in pages {
        let kept = {
            let page = doc.get_dictionary(page_id).map_err(|err| {
                Error::from(ErrorKind::FlattenFailed {
                    reason: FlattenReason::ReadPage,
                })
                .with_source(err)
            })?;
            match page.get(b"Annots").ok().cloned() {
                Some(annots) => filter_annots(doc, annots),
                None => None,
            }
        };
        let Some(kept) = kept else {
            continue;
        };
        let page = doc
            .get_object_mut(page_id)
            .and_then(Object::as_dict_mut)
            .map_err(|err| {
                Error::from(ErrorKind::FlattenFailed {
                    reason: FlattenReason::UpdateAnnotations,
                })
                .with_source(err)
            })?;
        if kept.is_empty() {
            page.remove(b"Annots");
        } else {
            page.set("Annots", Object::Array(kept));
        }
    }
    Ok(())
}

fn filter_annots(doc: &Document, annots: Object) -> Option<Vec<Object>> {
    let Object::Array(items) = inspect::deep_deref(doc, annots) else {
        return None;
    };
    Some(
        items
            .into_iter()
            .filter(|annot| keep_annot(doc, annot))
            .collect(),
    )
}

fn keep_annot(doc: &Document, annot: &Object) -> bool {
    let Object::Dictionary(dict) = inspect::deep_deref(doc, annot.clone()) else {
        return true;
    };
    dict.get(b"Subtype").and_then(Object::as_name_str).ok() != Some("Widget")
}

fn prune(doc: &mut Document) {
    let mut stack = Vec::new();
    for key in [
        b"Root".as_slice(),
        b"Info".as_slice(),
        b"Encrypt".as_slice(),
    ] {
        if let Ok(obj) = doc.trailer.get(key) {
            push_refs(obj, &mut stack);
        }
    }
    let mut seen = BTreeSet::new();
    while let Some(id) = stack.pop() {
        if !seen.insert(id) {
            continue;
        }
        if let Some(obj) = doc.objects.get(&id) {
            push_refs(obj, &mut stack);
        }
    }
    doc.objects.retain(|id, _| seen.contains(id));
}

fn push_refs(obj: &Object, stack: &mut Vec<lopdf::ObjectId>) {
    match obj {
        Object::Reference(id) => stack.push(*id),
        Object::Array(items) => {
            for item in items {
                push_refs(item, stack);
            }
        }
        Object::Dictionary(dict) => {
            for (_, value) in dict {
                push_refs(value, stack);
            }
        }
        Object::Stream(stream) => {
            for (_, value) in &stream.dict {
                push_refs(value, stack);
            }
        }
        _ => {}
    }
}

pub(crate) fn save(doc: &mut Document) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    doc.save_to(&mut bytes).map_err(|err| {
        Error::from(ErrorKind::FlattenFailed {
            reason: FlattenReason::Write,
        })
        .with_source(err)
    })?;
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::pdf_num;
    use crate::error::{ErrorKind, FlattenReason};

    #[test]
    fn pdf_num_rejects_nan_and_formats_finite_values() {
        let err = pdf_num(f32::NAN).unwrap_err();
        assert_eq!(
            err.kind,
            ErrorKind::FlattenFailed {
                reason: FlattenReason::NonFinite,
            }
        );
        assert_eq!(pdf_num(1.5).unwrap(), "1.5");
        assert_eq!(pdf_num(-0.0).unwrap(), "0");
    }
}
