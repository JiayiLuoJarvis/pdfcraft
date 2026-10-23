use std::collections::{BTreeMap, BTreeSet};

use crate::error::{self, Error, ErrorKind, FlattenReason, FontReason, Result};
use crate::fields::{HAlign, PAD_X};
use crate::font::PreparedFont;
use crate::inspect::{PdfDoc, PdfRect};
use crate::layout::LaidText;

const CHECK_SHORT_SIDE: f32 = 0.7;
const CHECK_LEFT_DX: f32 = 0.32;
const CHECK_JOINT_DX: f32 = 0.05;
const CHECK_RIGHT_DX: f32 = 0.34;
const CHECK_LEFT_DY: f32 = 0.02;
const CHECK_JOINT_DY: f32 = 0.28;
const CHECK_RIGHT_DY: f32 = 0.32;
const CHECK_STROKE_RATIO: f32 = 0.14;
const CHECK_STROKE_MIN: f32 = 0.8;

pub(crate) enum Mark {
    Text {
        laid: LaidText,
        h_align: HAlign,
    },
    Check,
    /// Black 1 pt rule `offset` points below the widget bottom.
    LineBelow {
        offset: f32,
    },
}

pub(crate) struct Stamp {
    pub(crate) page_index: usize,
    pub(crate) rect: PdfRect,
    pub(crate) mark: Mark,
}

pub(crate) struct Stroke {
    pub(crate) page_index: usize,
    pub(crate) x1: f32,
    pub(crate) y1: f32,
    pub(crate) x2: f32,
    pub(crate) y2: f32,
    pub(crate) width: f32,
    pub(crate) red: f32,
    pub(crate) green: f32,
    pub(crate) blue: f32,
}

struct Content {
    buf: Vec<u8>,
}

impl Content {
    fn clip(&mut self, rect: PdfRect) -> Result<()> {
        self.push(&format!(
            "q\n{} {} {} {} re\nW\nn\n",
            pdf_num(rect.x0)?,
            pdf_num(rect.y0)?,
            pdf_num(rect.width())?,
            pdf_num(rect.height())?
        ));
        Ok(())
    }

    fn check(&mut self, rect: PdfRect) -> Result<()> {
        let mark = rect.width().min(rect.height()) * CHECK_SHORT_SIDE;
        let mid_x = f32::midpoint(rect.x0, rect.x1);
        let mid_y = f32::midpoint(rect.y0, rect.y1);
        let left_x = mid_x - mark * CHECK_LEFT_DX;
        let joint_x = mid_x - mark * CHECK_JOINT_DX;
        let right_x = mid_x + mark * CHECK_RIGHT_DX;
        let left_y = mid_y + mark * CHECK_LEFT_DY;
        let joint_y = mid_y - mark * CHECK_JOINT_DY;
        let right_y = mid_y + mark * CHECK_RIGHT_DY;
        let stroke = (mark * CHECK_STROKE_RATIO).max(CHECK_STROKE_MIN);
        self.clip(rect)?;
        self.push(&format!(
            "{} w\n1 J\n1 j\n{} {} m\n{} {} l\n{} {} l\nS\nQ\n",
            pdf_num(stroke)?,
            pdf_num(left_x)?,
            pdf_num(left_y)?,
            pdf_num(joint_x)?,
            pdf_num(joint_y)?,
            pdf_num(right_x)?,
            pdf_num(right_y)?,
        ));
        Ok(())
    }

    fn text(
        &mut self,
        rect: PdfRect,
        laid: &LaidText,
        h_align: HAlign,
        font_name: &str,
    ) -> Result<()> {
        self.clip(rect)?;
        self.push("0 0 0 rg\nBT\n");
        for (baseline, width, glyphs) in laid.lines() {
            if glyphs.is_empty() {
                continue;
            }
            let x = match h_align {
                HAlign::Left => rect.x0 + PAD_X,
                HAlign::Right => rect.x1 - PAD_X - width,
                HAlign::Center => rect.x0 + (rect.width() - width) / 2.0,
            };
            self.push(&format!("/{font_name} {} Tf\n", pdf_num(laid.size())?));
            self.push(&format!(
                "1 0 0 1 {} {} Tm\n<",
                pdf_num(x)?,
                pdf_num(baseline)?
            ));
            for glyph in glyphs {
                self.push(&format!("{:04X}", glyph.id));
            }
            self.push("> Tj\n");
        }
        self.push("ET\nQ\n");
        Ok(())
    }

    fn line_below(&mut self, rect: PdfRect, offset: f32) -> Result<()> {
        let y = rect.y0 - offset;
        self.push(&format!(
            "q\n0 0 0 RG\n1 w\n{} {} m\n{} {} l\nS\nQ\n",
            pdf_num(rect.x0)?,
            pdf_num(y)?,
            pdf_num(rect.x1)?,
            pdf_num(y)?,
        ));
        Ok(())
    }

    fn stroke(&mut self, stroke: &Stroke) -> Result<()> {
        self.push(&format!(
            "q\n{} {} {} RG\n{} w\n{} {} m\n{} {} l\nS\nQ\n",
            pdf_num(stroke.red)?,
            pdf_num(stroke.green)?,
            pdf_num(stroke.blue)?,
            pdf_num(stroke.width)?,
            pdf_num(stroke.x1)?,
            pdf_num(stroke.y1)?,
            pdf_num(stroke.x2)?,
            pdf_num(stroke.y2)?,
        ));
        Ok(())
    }

    fn push(&mut self, text: &str) {
        self.buf.extend_from_slice(text.as_bytes());
    }

    fn into_bytes(self) -> Vec<u8> {
        self.buf
    }
}

impl PdfDoc {
    pub(crate) fn paint(
        &mut self,
        stamps: &[Stamp],
        strokes: &[Stroke],
        font: Option<&PreparedFont<'_>>,
    ) -> Result<()> {
        let font_id = match font {
            Some(prepared) if !prepared.is_empty() => Some(prepared.embed(self)?),
            _ => None,
        };
        let pages = self.page_ids();
        let mut by_page: BTreeMap<usize, Vec<&Stamp>> = BTreeMap::new();
        for stamp in stamps {
            by_page.entry(stamp.page_index).or_default().push(stamp);
        }
        let mut strokes_by_page: BTreeMap<usize, Vec<&Stroke>> = BTreeMap::new();
        for stroke in strokes {
            strokes_by_page
                .entry(stroke.page_index)
                .or_default()
                .push(stroke);
        }
        let mut pages_to_paint = BTreeSet::new();
        pages_to_paint.extend(by_page.keys().copied());
        pages_to_paint.extend(strokes_by_page.keys().copied());
        for index in pages_to_paint {
            let page_stamps = by_page.get(&index).map_or(&[][..], Vec::as_slice);
            let page_strokes = strokes_by_page.get(&index).map_or(&[][..], Vec::as_slice);
            let page_id = pages.get(index).copied().ok_or_else(|| {
                Error::from(ErrorKind::FlattenFailed {
                    reason: FlattenReason::PageOutOfRange { page_index: index },
                })
            })?;
            let needs_font = page_stamps.iter().any(|stamp| {
                matches!(
                    &stamp.mark,
                    Mark::Text { laid, .. }
                        if laid.lines().any(|(_, _, glyphs)| !glyphs.is_empty())
                )
            });
            let font_name = if needs_font {
                let font_id = font_id.ok_or_else(|| error::font_unusable(FontReason::Missing))?;
                Some(self.attach_font(page_id, font_id)?)
            } else {
                None
            };
            let content = page_bytes(page_stamps, page_strokes, font_name.as_deref())?;
            self.add_page_contents(page_id, content)?;
        }
        self.strip_form()?;
        self.prune()?;
        Ok(())
    }
}

fn page_bytes(stamps: &[&Stamp], strokes: &[&Stroke], font_name: Option<&str>) -> Result<Vec<u8>> {
    let mut content = Content { buf: Vec::new() };
    for stamp in stamps {
        match &stamp.mark {
            Mark::Check => content.check(stamp.rect)?,
            Mark::Text { laid, h_align } => {
                if let Some(font_name) = font_name {
                    content.text(stamp.rect, laid, *h_align, font_name)?;
                }
            }
            Mark::LineBelow { offset } => content.line_below(stamp.rect, *offset)?,
        }
    }
    for stroke in strokes {
        content.stroke(stroke)?;
    }
    Ok(content.into_bytes())
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
