//! Turns a buffered page plan into PDF bytes.
//!
//! This is the only backend file that constructs krilla `Document`, `Page`, and
//! `Surface`.
//!
//! krilla traps worth remembering:
//! - `Page::surface()` a second time on the same page clears prior drawing.
//! - `Surface::finish` is drop; unbalanced `push` / `pop` panics.
//! - `SerializeSettings { ..Default::default() }` leaves `enable_tagging: true`.
//! - krilla 0.8.2 parses fonts with skrifa 0.42. This crate measures with
//!   skrifa 0.48. [`check_upem`] rejects a face when those reads disagree.

use std::sync::Arc;

use krilla::action::LinkAction;
use krilla::annotation::{LinkAnnotation, Target};
use krilla::metadata::Metadata;
use krilla::page::PageSettings;
use krilla::surface::Surface;
use krilla::text::{Font, GlyphId, KrillaGlyph};
use krilla::{Data, Document, SerializeSettings};

use crate::backend::convert::{
    to_fill, to_krilla_path, to_krilla_point, to_krilla_rect, to_krilla_size, to_krilla_transform,
    to_stroke,
};
use crate::backend::krilla_canvas::{Mark, PagePlan, QueuedLink, Serialization};
use crate::canvas::DocumentInfo;
use crate::error::{Error, ErrorKind, Fallible as Result};
use crate::geom::Paint;

/// Renders closed page plans into PDF bytes.
pub(super) fn render(
    info: Option<&DocumentInfo>,
    pages: &[PagePlan],
    serialization: Serialization,
) -> Result<Vec<u8>> {
    let mut document = Document::new_with(to_serialize_settings(serialization));
    document.set_metadata(to_metadata(info));

    for (index, page_plan) in pages.iter().enumerate() {
        emit_page(&mut document, page_plan).map_err(|error| error.on_page_index(index))?;
    }

    document.finish().map_err(|error| {
        Error::from(ErrorKind::Backend {
            message: "cannot serialize the PDF".to_owned(),
        })
        .with_source(error)
    })
}

fn to_serialize_settings(serialization: Serialization) -> SerializeSettings {
    SerializeSettings {
        pretty: serialization.pretty,
        compress_content_streams: serialization.compress_content_streams,
        enable_tagging: false,
        ..SerializeSettings::default()
    }
}

fn to_metadata(info: Option<&DocumentInfo>) -> Metadata {
    let mut metadata = Metadata::new().creator("pdfcraft".to_owned());
    let Some(info) = info else {
        return metadata;
    };
    if let Some(title) = info.title.as_ref().filter(|title| !title.is_empty()) {
        metadata = metadata.title(title.clone());
    }
    if let Some(author) = info.author.as_ref().filter(|author| !author.is_empty()) {
        metadata = metadata.authors(vec![author.clone()]);
    }
    if let Some(subject) = info.subject.as_ref().filter(|subject| !subject.is_empty()) {
        metadata = metadata.description(subject.clone());
    }
    let keywords: Vec<String> = info
        .keywords
        .iter()
        .filter(|keyword| !keyword.is_empty())
        .cloned()
        .collect();
    if !keywords.is_empty() {
        metadata = metadata.keywords(keywords);
    }
    if let Some(language) = info
        .language
        .as_ref()
        .filter(|language| !language.is_empty())
    {
        metadata = metadata.language(language.clone());
    }
    metadata
}

fn emit_page(document: &mut Document, page_plan: &PagePlan) -> Result<()> {
    let settings = PageSettings::from_wh(
        page_plan.size.width().as_pt(),
        page_plan.size.height().as_pt(),
    )
    .ok_or(Error::from(ErrorKind::Backend {
        message: "PageSettings::from_wh rejected a positive Size".to_owned(),
    }))?;

    let mut page = document.start_page_with(settings);
    {
        // Call surface() exactly once. A second call clears the page.
        let mut surface = page.surface();
        let mut depth = 0usize;
        let drawn = draw_marks(&mut surface, &page_plan.marks, &mut depth);
        while depth > 0 {
            surface.pop();
            depth -= 1;
        }
        drawn?;
        surface.finish();
    }

    for link in &page_plan.links {
        page.add_annotation(to_annotation(link)?.into());
    }
    page.finish();
    Ok(())
}

fn draw_marks(surface: &mut Surface<'_>, marks: &[Mark], depth: &mut usize) -> Result<()> {
    for mark in marks {
        match mark {
            Mark::PushTransform(affine) => {
                surface.push_transform(&to_krilla_transform(*affine));
                *depth += 1;
            }
            Mark::PopTransform => {
                surface.pop();
                *depth = depth.saturating_sub(1);
            }
            mark => draw_mark(surface, mark)?,
        }
    }
    Ok(())
}

fn draw_mark(surface: &mut Surface<'_>, mark: &Mark) -> Result<()> {
    match mark {
        Mark::Fill { path, paint } => {
            surface.set_fill(Some(to_fill(*paint)));
            surface.set_stroke(None);
            surface.draw_path(&to_krilla_path(path)?);
        }
        Mark::Stroke {
            path,
            paint,
            stroke,
        } => {
            surface.set_fill(None);
            surface.set_stroke(Some(to_stroke(*paint, *stroke)));
            surface.draw_path(&to_krilla_path(path)?);
        }
        Mark::Glyphs {
            font_bytes,
            font_index,
            skrifa_units_per_em,
            font_size,
            color,
            origin,
            glyphs,
            text,
        } => {
            let font = Font::new(Data::from(Arc::clone(font_bytes)), *font_index).ok_or(
                Error::from(ErrorKind::Font {
                    index: *font_index,
                    message: "krilla Font::new returned None".to_owned(),
                }),
            )?;
            check_upem(*skrifa_units_per_em, font.units_per_em())?;
            let drawn: Vec<KrillaGlyph> = glyphs
                .iter()
                .map(|glyph| {
                    KrillaGlyph::new(
                        GlyphId::new(glyph.glyph_id),
                        glyph.advance_norm,
                        0.0,
                        0.0,
                        0.0,
                        glyph.byte_range.clone(),
                        None,
                    )
                })
                .collect();
            surface.set_fill(Some(to_fill(Paint::solid(*color))));
            surface.set_stroke(None);
            surface.draw_glyphs(
                to_krilla_point(*origin),
                &drawn,
                font,
                text,
                font_size.as_pt(),
                false,
            );
        }
        Mark::Image { raster, rect } => {
            let size = to_krilla_size(rect.size()).ok_or(Error::from(ErrorKind::Backend {
                message: "Size::from_wh rejected a positive image Size".to_owned(),
            }))?;
            // `draw_image` paints with the top edge at the current origin. Pair push and pop
            // with nothing fallible between them; an unbalanced transform panics inside krilla.
            let shift = krilla::geom::Transform::from_translate(
                rect.origin().x().as_pt(),
                rect.origin().y().as_pt(),
            );
            surface.push_transform(&shift);
            surface.draw_image(raster.krilla().clone(), size);
            surface.pop();
        }
        Mark::PushTransform(_) | Mark::PopTransform => {
            unreachable!("group transforms are handled in draw_marks");
        }
    }
    Ok(())
}

const UPEM_ABS_TOLERANCE: f32 = 0.01;

// krilla 0.8.2 parses the face with skrifa 0.42. Measurement uses skrifa 0.48.
// A units-per-em mismatch would scale the same advance against two em squares.
fn check_upem(skrifa_upem: f32, krilla_upem: f32) -> Result<()> {
    if (skrifa_upem - krilla_upem).abs() >= UPEM_ABS_TOLERANCE {
        Err(Error::from(ErrorKind::FontMetricsMismatch {
            skrifa_upem,
            krilla_upem,
        }))
    } else {
        Ok(())
    }
}

fn to_annotation(link: &QueuedLink) -> Result<LinkAnnotation> {
    let rect = to_krilla_rect(link.rect).ok_or(Error::from(ErrorKind::Backend {
        message: "Rect::from_xywh rejected a positive Rect".to_owned(),
    }))?;
    Ok(LinkAnnotation::new(
        rect,
        Target::Action(LinkAction::new(link.target.as_str().to_owned()).into()),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::KrillaCanvas;
    use crate::canvas::ImageRegistry;
    use crate::canvas::{Canvas, GlyphPlacement, GlyphRun, ImageId, LinkTarget};
    use crate::fonts::{fixture_bytes, FontId, FontSource, FontStore};
    use crate::geom::{Color, Paint, Path, Point, Positive, Rect, Size, Stroke, A4, LETTER};
    use crate::text::measure::measure_text;

    fn black() -> Paint {
        Paint::solid(Color::rgb(0, 0, 0))
    }

    fn thin() -> Stroke {
        Stroke::new(Positive::pt(1.0).unwrap())
    }

    fn probe_path() -> Path {
        Path::begin(Point::try_pt(10.0, 20.0).unwrap())
            .line_to(Point::try_pt(40.0, 20.0).unwrap())
            .line_to(Point::try_pt(40.0, 50.0).unwrap())
            .finish()
    }

    fn probe_page(canvas: &mut KrillaCanvas) -> Result<()> {
        canvas.start_page(Size::try_pt(200.0, 200.0)?)?;
        canvas.stroke_path(&probe_path(), &black(), &thin())?;
        canvas.annotate_link(
            Rect::try_pt(10.0, 30.0, 80.0, 44.0)?,
            LinkTarget::uri("https://example.test/a")?,
        )?;
        canvas.finish_page()?;
        Ok(())
    }

    #[test]
    fn finish_bytes_start_with_pdf() {
        let mut canvas = KrillaCanvas::new();
        canvas.start_page(A4).unwrap();
        canvas.finish_page().unwrap();
        let bytes = canvas.finish().unwrap();
        assert!(bytes.starts_with(b"%PDF"));
    }

    #[test]
    fn observable_probe_contains_path_ops_and_annot_rect() {
        // Observed from krilla 0.8.2 with pretty + uncompressed streams.
        // The page root transform appears as `1 0 0 -1 0 200 cm`, so path
        // operands stay in API space (10 20 / 40 20 / 40 50). Annot /Rect is
        // written already transformed: [10 126 90 170] for public (10,30,80,44).
        // If a krilla pin bump changes these literals, update them here.
        // Do not flip to_krilla_point.
        let mut canvas = KrillaCanvas::new_observable_for_test();
        canvas
            .set_document_info(&DocumentInfo {
                title: Some("Probe".to_owned()),
                ..DocumentInfo::default()
            })
            .unwrap();
        probe_page(&mut canvas).unwrap();
        let bytes = canvas.finish().unwrap();
        let text = String::from_utf8_lossy(&bytes);
        assert!(bytes.starts_with(b"%PDF"));
        assert!(text.contains("https://example.test/a"));
        assert!(text.contains("Probe"));
        assert!(text.contains("1 0 0 -1 0 200 cm"));
        assert!(text.contains("10 20 m"));
        assert!(text.contains("40 20 l"));
        assert!(text.contains("40 50 l"));
        assert!(text.contains("/Rect [10 126 90 170]"));
    }

    #[test]
    fn fill_then_stroke_clears_the_other_pen() {
        let mut canvas = KrillaCanvas::new_observable_for_test();
        canvas
            .start_page(Size::try_pt(200.0, 200.0).unwrap())
            .unwrap();
        let path = Path::begin(Point::try_pt(10.0, 10.0).unwrap())
            .line_to(Point::try_pt(50.0, 10.0).unwrap())
            .line_to(Point::try_pt(50.0, 50.0).unwrap())
            .close();
        canvas.fill_path(&path, &black()).unwrap();
        canvas.stroke_path(&path, &black(), &thin()).unwrap();
        canvas.finish_page().unwrap();
        let bytes = canvas.finish().unwrap();
        let text = String::from_utf8_lossy(&bytes);
        assert!(text.contains('f') || text.contains('F') || text.contains("f*"));
        assert!(text.contains('S') || text.contains('s'));
    }

    #[test]
    fn two_pages_with_different_sizes() {
        let mut canvas = KrillaCanvas::new();
        canvas.start_page(A4).unwrap();
        canvas.finish_page().unwrap();
        canvas.start_page(LETTER).unwrap();
        canvas.finish_page().unwrap();
        let bytes = canvas.finish().unwrap();
        assert!(bytes.starts_with(b"%PDF"));
    }

    #[test]
    fn invalid_canvas_state_nested_start_page() {
        let mut canvas = KrillaCanvas::new();
        canvas.start_page(A4).unwrap();
        let err = canvas.start_page(A4).unwrap_err();
        assert!(matches!(
            err.kind,
            ErrorKind::InvalidCanvasState {
                operation: "start_page",
                state: "a page is already open"
            }
        ));
    }

    #[test]
    fn draw_without_open_page_is_invalid() {
        let mut canvas = KrillaCanvas::new();
        let err = canvas
            .stroke_path(&probe_path(), &black(), &thin())
            .unwrap_err();
        assert!(matches!(
            err.kind,
            ErrorKind::InvalidCanvasState {
                operation: "stroke_path",
                state: "no page is open"
            }
        ));
    }

    #[test]
    fn finish_with_open_page_is_invalid() {
        let mut canvas = KrillaCanvas::new();
        canvas.start_page(A4).unwrap();
        let err = canvas.finish().unwrap_err();
        assert!(matches!(
            err.kind,
            ErrorKind::InvalidCanvasState {
                operation: "finish",
                state: "a page is still open"
            }
        ));
    }

    #[test]
    fn unknown_font_poisons_finish() {
        let mut canvas = KrillaCanvas::new();
        canvas.start_page(A4).unwrap();
        let run = GlyphRun {
            font: FontId::from_raw(0),
            font_size: Positive::pt(12.0).unwrap(),
            color: Color::rgb(0, 0, 0),
            origin: Point::try_pt(0.0, 0.0).unwrap(),
            glyphs: vec![GlyphPlacement {
                glyph_id: 1,
                advance_norm: 0.5,
                byte_range: 0..1,
            }],
            text: "a".to_owned(),
        };
        let err = canvas.draw_glyph_run(&run).unwrap_err();
        assert!(matches!(err.kind, ErrorKind::UnknownFont { id: 0 }));
        assert!(canvas.finish_page().is_err());
        assert!(canvas.finish().is_err());
    }

    #[test]
    fn measured_glyphs_emit_pdf() {
        let Some(bytes) = fixture_bytes() else {
            return;
        };
        let mut store = FontStore::new();
        let id = store.register(FontSource { bytes, index: 0 }).unwrap();
        let mut errors = crate::error::ErrorBag::default();
        let measured = measure_text(&mut store, &mut errors, id, "A").unwrap();
        let fonts = Arc::new(store);
        let run = GlyphRun {
            font: id,
            font_size: Positive::pt(12.0).unwrap(),
            color: Color::rgb(0, 0, 0),
            origin: Point::try_pt(72.0, 100.0).unwrap(),
            glyphs: measured
                .glyphs
                .iter()
                .map(|glyph| GlyphPlacement {
                    glyph_id: glyph.glyph_id,
                    advance_norm: glyph.advance_norm,
                    byte_range: glyph.byte_range.clone(),
                })
                .collect(),
            text: measured.text,
        };
        let mut canvas = KrillaCanvas::new_with_fonts(fonts);
        canvas.start_page(A4).unwrap();
        canvas.draw_glyph_run(&run).unwrap();
        canvas.finish_page().unwrap();
        let bytes = canvas.finish().unwrap();
        assert!(bytes.starts_with(b"%PDF"));
    }

    #[test]
    fn mismatched_advance_writes_observed_tj_adjustment() {
        let Some(bytes) = fixture_bytes() else {
            return;
        };
        let mut store = FontStore::new();
        let id = store.register(FontSource { bytes, index: 0 }).unwrap();
        let mut errors = crate::error::ErrorBag::default();
        let measured = measure_text(&mut store, &mut errors, id, "AA").unwrap();
        let fonts = Arc::new(store);
        let first = &measured.glyphs[0];
        let second = &measured.glyphs[1];
        let run = GlyphRun {
            font: id,
            font_size: Positive::pt(12.0).unwrap(),
            color: Color::rgb(0, 0, 0),
            origin: Point::try_pt(72.0, 100.0).unwrap(),
            glyphs: vec![
                GlyphPlacement {
                    glyph_id: first.glyph_id,
                    advance_norm: first.advance_norm + 0.25,
                    byte_range: first.byte_range.clone(),
                },
                GlyphPlacement {
                    glyph_id: second.glyph_id,
                    advance_norm: second.advance_norm,
                    byte_range: second.byte_range.clone(),
                },
            ],
            text: "AA".to_owned(),
        };
        let mut canvas = KrillaCanvas::new_observable_with_fonts(fonts);
        canvas
            .start_page(Size::try_pt(200.0, 200.0).unwrap())
            .unwrap();
        canvas.draw_glyph_run(&run).unwrap();
        canvas.finish_page().unwrap();
        let bytes = canvas.finish().unwrap();
        let text = String::from_utf8_lossy(&bytes);
        // The number is the 1000-unit hmtx delta, not advance_norm.
        // krilla writes it before the next glyph, so the run has two glyphs.
        let adjustment = tj_adjustment(&text);
        assert_eq!(adjustment.to_bits(), (-250.0f32).to_bits(), "{text}");
    }

    fn tj_adjustment(pdf: &str) -> f32 {
        let mut rest = pdf;
        while let Some(at) = rest.find("TJ") {
            let before = &rest[..at];
            let Some(open) = before.rfind('[') else {
                rest = &rest[at + 2..];
                continue;
            };
            let body = &before[open + 1..];
            for token in body.split_whitespace() {
                let token = token.trim_end_matches(']');
                if let Ok(number) = token.parse::<f32>() {
                    return number;
                }
            }
            rest = &rest[at + 2..];
        }
        panic!("no TJ adjustment number in stream:\n{pdf}");
    }

    #[test]
    fn fixture_upem_and_hmtx_match_across_skrifa_copies() {
        // Measure 'A' from the env face, then paint advance_norm + 0.25 vs hmtx.
        // The TJ number is the 1000-unit hmtx delta (−250 for +0.25 em).
        let Some(bytes) = fixture_bytes() else {
            return;
        };
        let font = Font::new(Data::from(Arc::clone(&bytes)), 0).unwrap();
        let skrifa_upem = {
            let mut store = FontStore::new();
            let id = store
                .register(FontSource {
                    bytes: Arc::clone(&bytes),
                    index: 0,
                })
                .unwrap();
            store.metrics(id).unwrap().units_per_em
        };
        assert_eq!(font.units_per_em().to_bits(), skrifa_upem.to_bits());

        let mut store = FontStore::new();
        let id = store
            .register(FontSource {
                bytes: Arc::clone(&bytes),
                index: 0,
            })
            .unwrap();
        let mut errors = crate::error::ErrorBag::default();
        let measured = measure_text(&mut store, &mut errors, id, "A").unwrap();
        assert!(errors.is_empty());
        let glyph = &measured.glyphs[0];
        let advance = glyph.advance_norm;

        let run = GlyphRun {
            font: id,
            font_size: Positive::pt(12.0).unwrap(),
            color: Color::rgb(0, 0, 0),
            origin: Point::try_pt(72.0, 100.0).unwrap(),
            glyphs: vec![
                GlyphPlacement {
                    glyph_id: glyph.glyph_id,
                    advance_norm: advance + 0.25,
                    byte_range: 0..1,
                },
                GlyphPlacement {
                    glyph_id: glyph.glyph_id,
                    advance_norm: advance,
                    byte_range: 1..2,
                },
            ],
            text: "AA".to_owned(),
        };
        let mut canvas = KrillaCanvas::new_observable_with_fonts(Arc::new(store));
        canvas
            .start_page(Size::try_pt(200.0, 200.0).unwrap())
            .unwrap();
        canvas.draw_glyph_run(&run).unwrap();
        canvas.finish_page().unwrap();
        let pdf = canvas.finish().unwrap();
        let text = String::from_utf8_lossy(&pdf);
        let adjustment = tj_adjustment(&text);
        assert_eq!(adjustment.to_bits(), (-250.0f32).to_bits(), "{text}");
    }

    #[test]
    fn upem_delta_of_one_hundredth_is_mismatch() {
        let err = check_upem(1000.0, 1000.01).unwrap_err();
        assert!(matches!(err.kind, ErrorKind::FontMetricsMismatch {
                skrifa_upem,
                krilla_upem
            } if (skrifa_upem - 1000.0).abs() < f32::EPSILON && (krilla_upem - 1000.01).abs() < f32::EPSILON
        ));
        assert!(check_upem(1000.0, 1000.0).is_ok());
    }

    fn registered_png() -> (
        crate::backend::images::ImageTable,
        crate::canvas::DecodedImage,
    ) {
        let mut table = crate::backend::images::ImageTable::default();
        let image = table
            .register(
                crate::backend::images::one_pixel_png().to_vec(),
                crate::canvas::ImageFormat::Png,
            )
            .unwrap();
        (table, image)
    }

    #[test]
    fn unknown_image_poisons_finish() {
        let mut canvas = KrillaCanvas::new();
        canvas.start_page(A4).unwrap();
        let image = crate::canvas::DecodedImage {
            id: ImageId::from_raw(0),
            pixel_width: 1,
            pixel_height: 1,
        };
        let err = canvas
            .draw_image(&image, Rect::try_pt(0.0, 0.0, 10.0, 10.0).unwrap())
            .unwrap_err();
        assert!(matches!(err.kind, ErrorKind::UnknownImage { id: 0 }));
        assert!(canvas.finish().is_err());
    }

    #[test]
    fn one_pixel_png_finishes_as_pdf() {
        let (table, image) = registered_png();
        let mut canvas = KrillaCanvas::new_with_resources(Arc::new(FontStore::new()), table);
        canvas.start_page(A4).unwrap();
        canvas
            .draw_image(&image, Rect::try_pt(12.0, 24.0, 30.0, 30.0).unwrap())
            .unwrap();
        canvas.finish_page().unwrap();
        let bytes = canvas.finish().unwrap();
        assert!(bytes.starts_with(b"%PDF"));
    }

    #[test]
    fn three_pages_share_one_image_xobject() {
        let (table, image) = registered_png();
        let mut canvas =
            KrillaCanvas::new_observable_with_resources(Arc::new(FontStore::new()), table);
        let rect = Rect::try_pt(10.0, 10.0, 20.0, 20.0).unwrap();
        for _ in 0..3 {
            canvas
                .start_page(Size::try_pt(200.0, 200.0).unwrap())
                .unwrap();
            canvas.draw_image(&image, rect).unwrap();
            canvas.finish_page().unwrap();
        }
        let bytes = canvas.finish().unwrap();
        assert!(bytes.starts_with(b"%PDF"));
        let text = String::from_utf8_lossy(&bytes);
        assert_eq!(text.matches("/Subtype /Image").count(), 1, "{text}");
    }

    #[test]
    fn late_document_info_still_applies() {
        let mut canvas = KrillaCanvas::new_observable_for_test();
        canvas
            .start_page(Size::try_pt(200.0, 200.0).unwrap())
            .unwrap();
        canvas.finish_page().unwrap();
        canvas
            .set_document_info(&DocumentInfo {
                title: Some("LateTitle".to_owned()),
                ..DocumentInfo::default()
            })
            .unwrap();
        let bytes = canvas.finish().unwrap();
        let text = String::from_utf8_lossy(&bytes);
        assert!(text.contains("LateTitle"));
    }

    /// Re-run manually after bumping the krilla pin to refresh observed literals.
    #[test]
    #[ignore = "writes probe.pdf next to the crate; re-run when refreshing stream literals"]
    fn dump_experiment_pdf() {
        let mut canvas = KrillaCanvas::new_observable_for_test();
        canvas
            .set_document_info(&DocumentInfo {
                title: Some("Probe".to_owned()),
                ..DocumentInfo::default()
            })
            .unwrap();
        probe_page(&mut canvas).unwrap();
        let bytes = canvas.finish().unwrap();
        std::fs::write("probe.pdf", bytes).unwrap();
    }

    #[test]
    fn rgba_png_emits_smask() {
        // 4x4 RGBA: transparent black outside, opaque red inside.
        let png: &[u8] = &[
            137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 13, 73, 72, 68, 82, 0, 0, 0, 4, 0, 0, 0, 4,
            8, 6, 0, 0, 0, 169, 241, 158, 126, 0, 0, 0, 21, 73, 68, 65, 84, 120, 218, 99, 96, 192,
            6, 254, 3, 209, 127, 48, 133, 75, 0, 25, 0, 0, 15, 67, 7, 249, 150, 143, 128, 25, 0, 0,
            0, 0, 73, 69, 78, 68, 174, 66, 96, 130,
        ];
        let mut table = crate::backend::images::ImageTable::default();
        let image = table
            .register(png.to_vec(), crate::canvas::ImageFormat::Png)
            .unwrap();
        let mut canvas =
            KrillaCanvas::new_observable_with_resources(Arc::new(FontStore::new()), table);
        canvas
            .start_page(Size::try_pt(200.0, 200.0).unwrap())
            .unwrap();
        canvas
            .draw_image(&image, Rect::try_pt(10.0, 10.0, 40.0, 40.0).unwrap())
            .unwrap();
        canvas.finish_page().unwrap();
        let bytes = canvas.finish().unwrap();
        let text = String::from_utf8_lossy(&bytes);
        assert!(text.contains("/SMask"), "expected soft mask, got:\n{text}");
        assert_eq!(text.matches("/Subtype /Image").count(), 2, "{text}");
    }
}
