//! Turns a buffered page plan into PDF bytes.
//!
//! This is the only backend file that constructs krilla `Document`, `Page`, and
//! `Surface`.
//!
//! krilla traps worth remembering:
//! - `Page::surface()` a second time on the same page clears prior drawing.
//! - `Surface::finish` is drop; unbalanced `push` / `pop` panics.
//! - `SerializeSettings { ..Default::default() }` leaves `enable_tagging: true`.

use krilla::action::LinkAction;
use krilla::annotation::{LinkAnnotation, Target};
use krilla::metadata::Metadata;
use krilla::page::PageSettings;
use krilla::surface::Surface;
use krilla::{Document, SerializeSettings};

use crate::backend::convert::{to_fill, to_krilla_path, to_krilla_rect, to_stroke};
use crate::backend::krilla_canvas::{Mark, PagePlan, QueuedLink, Serialization};
use crate::canvas::DocumentInfo;
use crate::error::{Error, Result};

/// Renders closed page plans into PDF bytes.
pub(super) fn render(
    info: Option<&DocumentInfo>,
    pages: &[PagePlan],
    serialization: Serialization,
) -> Result<Vec<u8>> {
    let mut document = Document::new_with(to_serialize_settings(serialization));
    document.set_metadata(to_metadata(info));

    for page_plan in pages {
        emit_page(&mut document, page_plan)?;
    }

    document
        .finish()
        .map_err(|error| Error::Backend(error.to_string()))
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
    let mut metadata = Metadata::new().creator("quire".to_owned());
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
    .ok_or(Error::Backend(
        "PageSettings::from_wh rejected a positive Size".to_owned(),
    ))?;

    let mut page = document.start_page_with(settings);
    {
        // Call surface() exactly once. A second call clears the page.
        let mut surface = page.surface();
        for mark in &page_plan.marks {
            draw_mark(&mut surface, mark)?;
        }
        surface.finish();
    }

    for link in &page_plan.links {
        page.add_annotation(to_annotation(link)?.into());
    }
    page.finish();
    Ok(())
}

fn draw_mark(surface: &mut Surface<'_>, mark: &Mark) -> Result<()> {
    match mark {
        Mark::Fill { path, paint } => {
            surface.set_fill(Some(to_fill(*paint)?));
            surface.set_stroke(None);
            surface.draw_path(&to_krilla_path(path)?);
        }
        Mark::Stroke {
            path,
            paint,
            stroke,
        } => {
            surface.set_fill(None);
            surface.set_stroke(Some(to_stroke(*paint, *stroke)?));
            surface.draw_path(&to_krilla_path(path)?);
        }
    }
    Ok(())
}

fn to_annotation(link: &QueuedLink) -> Result<LinkAnnotation> {
    let rect = to_krilla_rect(link.rect).ok_or(Error::Backend(
        "Rect::from_xywh rejected a positive Rect".to_owned(),
    ))?;
    Ok(LinkAnnotation::new(
        rect,
        Target::Action(LinkAction::new(link.target.as_str().to_owned()).into()),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::KrillaCanvas;
    use crate::canvas::{Canvas, GlyphPlacement, GlyphRun, ImageId, LinkTarget};
    use crate::fonts::FontId;
    use crate::geom::{Color, Paint, Path, Point, Positive, Rect, Size, Stroke, A4, LETTER};

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
            err,
            Error::InvalidCanvasState {
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
            err,
            Error::InvalidCanvasState {
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
            err,
            Error::InvalidCanvasState {
                operation: "finish",
                state: "a page is still open"
            }
        ));
    }

    #[test]
    fn unsupported_glyph_run_poisons_finish() {
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
        assert!(matches!(
            err,
            Error::UnsupportedCanvasOperation {
                operation: "draw_glyph_run"
            }
        ));
        assert!(canvas.finish_page().is_err());
        assert!(canvas.finish().is_err());
    }

    #[test]
    fn unsupported_image_poisons_finish() {
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
        assert!(matches!(
            err,
            Error::UnsupportedCanvasOperation {
                operation: "draw_image"
            }
        ));
        assert!(canvas.finish().is_err());
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
}
