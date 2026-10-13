use std::collections::BTreeMap;
use std::fmt::Write;

use crate::error::{Error, ErrorKind, FlattenReason, FontFault, Result};
use crate::fields::PAD_X;
use crate::font::{self, Face};
use crate::inspect::{PdfDoc, PdfRect};
use crate::layout::LaidText;

pub(crate) enum Mark {
    Text(LaidText),
    Check,
}

pub(crate) struct Stamp {
    pub(crate) page_index: usize,
    pub(crate) rect: PdfRect,
    pub(crate) mark: Mark,
}

pub(crate) fn paint(
    doc: &mut PdfDoc,
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
    let pages = doc.page_ids();
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
            matches!(&stamp.mark, Mark::Text(text) if text.lines().any(|(_, glyphs)| !glyphs.is_empty()))
        });
        let font_name = if needs_font {
            let font_id = font_id.ok_or_else(|| {
                Error::from(ErrorKind::FontNotConfigured).with_source(FontFault::Missing)
            })?;
            Some(doc.attach_font(page_id, font_id)?)
        } else {
            None
        };
        let content = page_content(&page_stamps, font_name.as_deref())?;
        doc.add_page_contents(page_id, content.into_bytes())?;
    }
    doc.strip_form()?;
    doc.prune();
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
    for (baseline, glyphs) in text.lines() {
        if glyphs.is_empty() {
            continue;
        }
        let _ = writeln!(out, "/{font_name} {} Tf", pdf_num(text.size())?);
        let _ = write!(out, "1 0 0 1 {} {} Tm\n<", pdf_num(x)?, pdf_num(baseline)?);
        for glyph in glyphs {
            let _ = write!(out, "{:04X}", glyph.id);
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
