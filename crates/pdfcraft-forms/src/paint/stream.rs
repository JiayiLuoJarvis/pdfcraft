//! Marks, stamps, and strokes painted into page content streams.

use std::collections::{BTreeMap, BTreeSet};

use super::embed::embed;
use crate::error::{Error, ErrorKind, FlattenReason, Result};
use crate::geom::PdfRect;
use crate::layout::LaidText;
use crate::layout::PAD_X;
use crate::pdf::{opaque_extgstate, OverlayIds, OverlayNames, PdfDoc};
use crate::request::HAlign;
use lopdf::Dictionary;
use lopdf::Object;
use lopdf::Stream;

use super::PaintFont;

const CHECK_SHORT_SIDE: f32 = 0.7;
const CHECK_LEFT_DX: f32 = 0.32;
const CHECK_JOINT_DX: f32 = 0.05;
const CHECK_RIGHT_DX: f32 = 0.34;
const CHECK_LEFT_DY: f32 = 0.02;
const CHECK_JOINT_DY: f32 = 0.28;
const CHECK_RIGHT_DY: f32 = 0.32;
const CHECK_STROKE_RATIO: f32 = 0.14;
const CHECK_STROKE_MIN: f32 = 0.8;
const RESTORE_PAGE_STATE: &str = "Q\n";
const TEXT_STATE_RESET: &str = "0 Tr\n0 Tc\n0 Tw\n100 Tz\n0 Ts\n";
const SOLID_DASH: &str = "[] 0 d\n";
const BUTT_MITER: &str = "0 J\n0 j\n";

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
        self.push("0 0 0 RG\n");
        self.push(SOLID_DASH);
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
        self.push("0 0 0 rg\n");
        self.push(TEXT_STATE_RESET);
        self.push("BT\n");
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
        self.push("q\n0 0 0 RG\n1 w\n");
        self.push(BUTT_MITER);
        self.push(SOLID_DASH);
        self.push(&format!(
            "{} {} m\n{} {} l\nS\nQ\n",
            pdf_num(rect.x0)?,
            pdf_num(y)?,
            pdf_num(rect.x1)?,
            pdf_num(y)?,
        ));
        Ok(())
    }

    fn stroke(&mut self, stroke: &Stroke) -> Result<()> {
        self.push(&format!(
            "q\n{} {} {} RG\n{} w\n",
            pdf_num(stroke.red)?,
            pdf_num(stroke.green)?,
            pdf_num(stroke.blue)?,
            pdf_num(stroke.width)?,
        ));
        self.push(BUTT_MITER);
        self.push(SOLID_DASH);
        self.push(&format!(
            "{} {} m\n{} {} l\nS\nQ\n",
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

pub(crate) fn paint(
    doc: &mut PdfDoc,
    stamps: &[Stamp],
    strokes: &[Stroke],
    font: Option<PaintFont<'_>>,
) -> Result<()> {
    let font_id = match font {
        Some(prepared) if !prepared.used.is_empty() => Some(embed(&prepared, doc)?),
        _ => None,
    };
    let pages = doc.page_ids();
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
    if !pages_to_paint.is_empty() {
        let opaque = doc.add_object(Object::Dictionary(opaque_extgstate()));
        let save = doc.add_object(Object::Stream(Stream::new(
            Dictionary::new(),
            b"q\n".to_vec(),
        )));
        for index in pages_to_paint {
            let page_stamps = by_page.get(&index).map_or(&[][..], Vec::as_slice);
            let page_strokes = strokes_by_page.get(&index).map_or(&[][..], Vec::as_slice);
            let page_id = pages
                .get(index)
                .copied()
                .ok_or_else(|| Error::new(ErrorKind::FlattenFailed(FlattenReason::ReadPage)))?;
            let needs_font = page_stamps.iter().any(|stamp| {
                matches!(
                    &stamp.mark,
                    Mark::Text { laid, .. }
                        if laid.lines().any(|(_, _, glyphs)| !glyphs.is_empty())
                )
            });
            let page_font =
                if needs_font {
                    Some(font_id.ok_or_else(|| {
                        Error::new(ErrorKind::FlattenFailed(FlattenReason::ReadPage))
                    })?)
                } else {
                    None
                };
            let names = doc.attach_overlay_resources(
                page_id,
                &OverlayIds {
                    font: page_font,
                    opaque,
                },
            )?;
            let content = page_bytes(page_stamps, page_strokes, &names)?;
            doc.append_isolated(page_id, save, content)?;
        }
    }
    doc.strip_form()?;
    doc.prune()?;
    Ok(())
}

fn page_bytes(stamps: &[&Stamp], strokes: &[&Stroke], names: &OverlayNames) -> Result<Vec<u8>> {
    let mut content = Content {
        buf: b"\n".to_vec(),
    };
    content.push(RESTORE_PAGE_STATE);
    content.push(&format!("/{} gs\n", names.opaque));
    for stamp in stamps {
        match &stamp.mark {
            Mark::Check => content.check(stamp.rect)?,
            Mark::Text { laid, h_align } => {
                if let Some(font_name) = names.font.as_deref() {
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
        return Err(Error::new(ErrorKind::FlattenFailed(
            FlattenReason::NonFinite,
        )));
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
        Ok("0".to_owned())
    } else {
        Ok(text)
    }
}

#[cfg(test)]
mod tests {
    use super::{
        page_bytes, pdf_num, Mark, Stamp, Stroke, BUTT_MITER, SOLID_DASH, TEXT_STATE_RESET,
    };
    use crate::error::{ErrorKind, FlattenReason};
    use crate::geom::PdfRect;
    use crate::layout::LaidText;
    use crate::pdf::OverlayNames;
    use crate::request::HAlign;

    fn opaque_names(font: Option<&str>) -> OverlayNames {
        OverlayNames {
            font: font.map(str::to_owned),
            opaque: "PdfCraftOpaque".to_owned(),
        }
    }

    fn rect() -> PdfRect {
        PdfRect::new(0.0, 0.0, 10.0, 10.0)
    }

    fn before_stroke(block: &str) -> &str {
        let at = block
            .find("\nS\n")
            .unwrap_or_else(|| panic!("no stroke in {block}"));
        &block[..at]
    }

    #[test]
    fn pdf_num_rejects_nan_and_formats_finite_values() {
        let err = pdf_num(f32::NAN).unwrap_err();
        assert_eq!(
            err.kind(),
            ErrorKind::FlattenFailed(FlattenReason::NonFinite)
        );
        assert_eq!(pdf_num(1.5).unwrap(), "1.5");
        assert_eq!(pdf_num(-0.0).unwrap(), "0");
    }

    #[test]
    fn page_bytes_restores_and_sets_opaque_first() {
        let stamp = Stamp {
            page_index: 0,
            rect: rect(),
            mark: Mark::Check,
        };
        let bytes = page_bytes(&[&stamp], &[], &opaque_names(None)).unwrap();
        assert!(
            bytes.starts_with(b"\nQ\n/PdfCraftOpaque gs\n"),
            "{}",
            String::from_utf8_lossy(&bytes)
        );
    }

    #[test]
    fn text_resets_text_state_before_bt() {
        let stamp = Stamp {
            page_index: 0,
            rect: rect(),
            mark: Mark::Text {
                laid: LaidText::stub_line(12.0),
                h_align: HAlign::Left,
            },
        };
        let text = String::from_utf8(page_bytes(&[&stamp], &[], &opaque_names(Some("F"))).unwrap())
            .unwrap();
        let q = text.find("q\n").expect("q");
        let reset = text.find(TEXT_STATE_RESET).expect("text state reset");
        let bt = text.find("BT\n").expect("BT");
        assert!(q < reset, "{text}");
        assert!(reset < bt, "{text}");
    }

    #[test]
    fn check_sets_black_stroke_and_solid_dash() {
        let stamp = Stamp {
            page_index: 0,
            rect: rect(),
            mark: Mark::Check,
        };
        let text =
            String::from_utf8(page_bytes(&[&stamp], &[], &opaque_names(None)).unwrap()).unwrap();
        let check = match text.split_once("/PdfCraftOpaque gs\n") {
            Some((_, rest)) => rest,
            None => &text,
        };
        let before = before_stroke(check);
        assert!(before.contains("0 0 0 RG"), "{text}");
        assert!(before.contains("[] 0 d"), "{text}");
    }

    #[test]
    fn rules_and_strokes_set_cap_join_and_dash() {
        let rule = Stamp {
            page_index: 0,
            rect: rect(),
            mark: Mark::LineBelow { offset: 2.0 },
        };
        let stroke = Stroke {
            page_index: 0,
            x1: 0.0,
            y1: 0.0,
            x2: 10.0,
            y2: 0.0,
            width: 1.0,
            red: 1.0,
            green: 0.0,
            blue: 0.0,
        };
        let text =
            String::from_utf8(page_bytes(&[&rule], &[&stroke], &opaque_names(None)).unwrap())
                .unwrap();
        for block in text.split("q\n").skip(1) {
            let before = before_stroke(block);
            assert!(before.contains(BUTT_MITER.trim_end()), "{text}");
            assert!(before.contains(SOLID_DASH.trim_end()), "{text}");
        }
    }
}
