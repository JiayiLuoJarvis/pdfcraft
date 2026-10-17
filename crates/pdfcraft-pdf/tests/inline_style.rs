//! Inline `RunStyle` through `layout_spec`, read from recorded canvas commands.

use pdfcraft_pdf::canvas::pt_near;
use pdfcraft_pdf::spec::{
    Alignment, BlankSpec, BlankWidth, Block, FontFace, FontFamily, FontRegistry, HLineSpec,
    InlineRun, ParagraphSpec, Resources, RunStyle, SectionSpec,
};
use pdfcraft_pdf::{
    layout_spec, CanvasCommand, Color, DocumentSpec, ErrorKind, Finite, Length, NonNegative,
    PageSize, PathVerb, Positive, RecordedPage, Rendered, Result,
};
use skrifa::instance::{LocationRef, Size};
use skrifa::{FontRef, MetadataProvider};

#[path = "common/mod.rs"]
mod common;

const RED: Color = Color::rgb(200, 0, 0);

struct Faces {
    bold: bool,
    italic: bool,
    bold_italic: bool,
}

const REGULAR_ONLY: Faces = Faces {
    bold: false,
    italic: false,
    bold_italic: false,
};

fn render(faces: &Faces, body: Vec<Block<()>>) -> Option<Result<Rendered>> {
    let bytes = common::test_font_bytes()?;
    let face = |on: bool| on.then(|| FontFace::new(bytes.clone(), 0));
    let family = FontFamily {
        regular: FontFace::new(bytes.clone(), 0),
        bold: face(faces.bold),
        italic: face(faces.italic),
        bold_italic: face(faces.bold_italic),
    };
    Some(layout_spec(DocumentSpec {
        page_size: PageSize::A4,
        resources: Resources::with_fonts(FontRegistry::single(family)),
        limits: pdfcraft_pdf::Limits::default(),
        info: pdfcraft_pdf::spec::DocumentInfo::default(),
        sections: vec![SectionSpec {
            body,
            ..SectionSpec::default()
        }],
    }))
}

fn run(text: &str, style: RunStyle) -> InlineRun<()> {
    InlineRun::Text {
        text: text.into(),
        font: (),
        style,
    }
}

fn para(runs: Vec<InlineRun<()>>) -> Block<()> {
    Block::Paragraph(ParagraphSpec {
        runs,
        ..ParagraphSpec::default()
    })
}

fn underlined() -> RunStyle {
    RunStyle {
        underline: Some(true),
        ..RunStyle::default()
    }
}

fn tracked(pt: f32) -> RunStyle {
    RunStyle {
        tracking: Some(Finite::pt(pt).unwrap()),
        ..RunStyle::default()
    }
}

fn pos(value: f32) -> Length<Positive> {
    Positive::pt(value).unwrap()
}

fn nn(value: f32) -> Length<NonNegative> {
    NonNegative::pt(value).unwrap()
}

struct Glyphs {
    text: String,
    font: u32,
    x: f32,
    baseline: f32,
    width: f32,
    advances: Vec<f32>,
}

fn glyph_runs(page: &RecordedPage) -> Vec<Glyphs> {
    page.ops()
        .iter()
        .filter_map(|op| match op {
            CanvasCommand::DrawGlyphRun { run } => {
                let size = run.font_size.as_pt();
                let advances: Vec<f32> = run
                    .glyphs
                    .iter()
                    .map(|glyph| glyph.advance_norm * size)
                    .collect();
                Some(Glyphs {
                    text: run.text.clone(),
                    font: run.font.as_u32(),
                    x: run.origin.x().as_pt(),
                    baseline: run.origin.y().as_pt(),
                    width: advances.iter().sum(),
                    advances,
                })
            }
            _ => None,
        })
        .collect()
}

#[derive(Debug)]
struct Line {
    x0: f32,
    x1: f32,
    y: f32,
    thickness: f32,
    color: Color,
}

fn strokes(page: &RecordedPage) -> Vec<Line> {
    page.ops()
        .iter()
        .filter_map(|op| match op {
            CanvasCommand::StrokePath {
                path,
                paint,
                stroke,
            } => match path.verbs() {
                [PathVerb::MoveTo(from), PathVerb::LineTo(to)] => {
                    assert!(pt_near(from.y().as_pt(), to.y().as_pt()), "horizontal");
                    Some(Line {
                        x0: from.x().as_pt(),
                        x1: to.x().as_pt(),
                        y: from.y().as_pt(),
                        thickness: stroke.width().as_pt(),
                        color: paint.color(),
                    })
                }
                _ => None,
            },
            _ => None,
        })
        .collect()
}

/// Centerline below the baseline, `-(offset / upem) * size + 0.5`, and the
/// drawn thickness, which is 1 pt. The font's own thickness is not 1 pt at `size`.
fn decoration_at(strike: bool, size: f32) -> (f32, f32) {
    let bytes = common::test_font_bytes().unwrap();
    let font = FontRef::from_index(&bytes, 0).unwrap();
    let metrics = font.metrics(Size::unscaled(), LocationRef::default());
    let upem = f32::from(metrics.units_per_em);
    let line = if strike {
        metrics.strikeout.expect("fixture has OS/2 strikeout")
    } else {
        metrics.underline.expect("fixture has post underline")
    };
    let font_thickness = line.thickness / upem * size;
    assert!(
        (font_thickness - 1.0).abs() > 1.0e-3,
        "font thickness {font_thickness} at {size} pt must not already be 1"
    );
    (-(line.offset / upem) * size + 0.5, 1.0)
}

fn underline_at(size: f32) -> (f32, f32) {
    decoration_at(false, size)
}

fn kinds(result: Result<Rendered>) -> Vec<ErrorKind> {
    result
        .err()
        .expect("layout fails")
        .iter()
        .map(|error| error.kind.clone())
        .collect()
}

#[test]
fn underline_sits_at_the_post_offset_and_merges_across_runs() {
    let Some(rendered) = render(
        &REGULAR_ONLY,
        vec![para(vec![run("AB", underlined()), run("CD", underlined())])],
    ) else {
        return;
    };
    let page = &rendered.unwrap().pages()[0];
    let glyphs = glyph_runs(page);
    let lines = strokes(page);
    let (offset, thickness) = underline_at(12.0);
    assert_eq!(lines.len(), 1, "{lines:?}");
    let end = glyphs.last().map(|g| g.x + g.width).unwrap();
    assert!(pt_near(lines[0].x0, glyphs[0].x));
    assert!(pt_near(lines[0].x1, end));
    assert!(pt_near(lines[0].y, glyphs[0].baseline + offset));
    assert!(pt_near(lines[0].thickness, thickness));
    assert!(lines[0].y > glyphs[0].baseline);
}

#[test]
fn strikethrough_is_1pt_at_12_and_18() {
    let small = RunStyle {
        strikethrough: Some(true),
        ..RunStyle::default()
    };
    let large = RunStyle {
        font_size: Some(pos(18.0)),
        ..small
    };
    let Some(rendered) = render(
        &REGULAR_ONLY,
        vec![para(vec![run("A", small), run("B", large)])],
    ) else {
        return;
    };
    let page = &rendered.unwrap().pages()[0];
    let glyphs = glyph_runs(page);
    let lines = strokes(page);
    assert_eq!(lines.len(), 2, "{lines:?}");
    let (small_offset, small_thickness) = decoration_at(true, 12.0);
    let (large_offset, large_thickness) = decoration_at(true, 18.0);
    assert!(pt_near(lines[0].y, glyphs[0].baseline + small_offset));
    assert!(pt_near(lines[0].thickness, small_thickness));
    assert!(pt_near(lines[1].y, glyphs[1].baseline + large_offset));
    assert!(pt_near(lines[1].thickness, large_thickness));
}

fn sfnt_origin(bytes: &[u8]) -> usize {
    if bytes.len() >= 16 && &bytes[0..4] == b"ttcf" {
        u32::from_be_bytes(bytes[12..16].try_into().expect("ttc offset")) as usize
    } else {
        0
    }
}

fn table_offset(bytes: &[u8], origin: usize, tag: [u8; 4]) -> Option<usize> {
    if origin + 6 > bytes.len() {
        return None;
    }
    let num = u16::from_be_bytes(bytes[origin + 4..origin + 6].try_into().ok()?) as usize;
    let mut at = origin + 12;
    for _ in 0..num {
        if at + 16 > bytes.len() {
            return None;
        }
        if bytes[at..at + 4] == tag {
            return Some(u32::from_be_bytes(bytes[at + 8..at + 12].try_into().ok()?) as usize);
        }
        at += 16;
    }
    None
}

fn zero_decoration_thickness(bytes: &[u8]) -> Vec<u8> {
    let mut out = bytes.to_vec();
    let origin = sfnt_origin(&out);
    let post = table_offset(&out, origin, *b"post").expect("post table");
    out[post + 10] = 0;
    out[post + 11] = 0;
    let os2 = table_offset(&out, origin, *b"OS/2").expect("OS/2 table");
    out[os2 + 26] = 0;
    out[os2 + 27] = 0;
    out
}

#[test]
fn zero_font_thickness_still_draws_1pt() {
    let Some(bytes) = common::test_font_bytes() else {
        return;
    };
    let patched = zero_decoration_thickness(&bytes);
    let font = FontRef::from_index(&patched, 0).expect("patched face parses");
    let metrics = font.metrics(Size::unscaled(), LocationRef::default());
    let underline = metrics.underline.expect("post offset");
    let strike = metrics.strikeout.expect("OS/2 offset");
    assert_eq!(underline.thickness.to_bits(), 0.0f32.to_bits());
    assert_eq!(strike.thickness.to_bits(), 0.0f32.to_bits());
    assert!(underline.offset.is_finite());
    assert!(strike.offset.is_finite());
    let upem = f32::from(metrics.units_per_em);
    let family = FontFamily {
        regular: FontFace::new(patched, 0),
        bold: None,
        italic: None,
        bold_italic: None,
    };
    let struck = RunStyle {
        strikethrough: Some(true),
        ..underlined()
    };
    let rendered = layout_spec(DocumentSpec {
        page_size: PageSize::A4,
        resources: Resources::with_fonts(FontRegistry::single(family)),
        limits: pdfcraft_pdf::Limits::default(),
        info: pdfcraft_pdf::spec::DocumentInfo::default(),
        sections: vec![SectionSpec {
            body: vec![para(vec![run("A", struck)])],
            ..SectionSpec::default()
        }],
    })
    .expect("zero thickness still lays out");
    let page = &rendered.pages()[0];
    let glyphs = glyph_runs(page);
    let lines = strokes(page);
    assert_eq!(lines.len(), 2, "{lines:?}");
    let underline_y = -(underline.offset / upem) * 12.0 + 0.5;
    let strike_y = -(strike.offset / upem) * 12.0 + 0.5;
    let mut ys: Vec<f32> = lines.iter().map(|line| line.y).collect();
    ys.sort_by(f32::total_cmp);
    let mut expected = [
        glyphs[0].baseline + underline_y,
        glyphs[0].baseline + strike_y,
    ];
    expected.sort_by(f32::total_cmp);
    assert!(pt_near(ys[0], expected[0]));
    assert!(pt_near(ys[1], expected[1]));
    assert!(lines.iter().all(|line| pt_near(line.thickness, 1.0)));
}

#[test]
fn a_color_or_size_change_starts_a_new_underline() {
    let red = RunStyle {
        color: Some(RED),
        ..underlined()
    };
    let large = RunStyle {
        font_size: Some(pos(18.0)),
        ..underlined()
    };
    let Some(rendered) = render(
        &REGULAR_ONLY,
        vec![para(vec![
            run("A", underlined()),
            run("B", red),
            run("C", large),
        ])],
    ) else {
        return;
    };
    let page = &rendered.unwrap().pages()[0];
    let glyphs = glyph_runs(page);
    let lines = strokes(page);
    assert_eq!(glyphs.len(), 3);
    assert_eq!(lines.len(), 3, "{lines:?}");
    let baseline = glyphs[0].baseline;
    for (line, glyph) in lines.iter().zip(&glyphs) {
        assert!(pt_near(line.x0, glyph.x));
        assert!(pt_near(line.x1, glyph.x + glyph.width));
        assert!(pt_near(glyph.baseline, baseline));
    }
    assert_eq!(lines[0].color, Color::rgb(0, 0, 0));
    assert_eq!(lines[1].color, RED);
    let (small_offset, small_thickness) = underline_at(12.0);
    let (large_offset, large_thickness) = underline_at(18.0);
    assert!(pt_near(lines[1].y, baseline + small_offset));
    assert!(pt_near(lines[1].thickness, small_thickness));
    assert!(pt_near(lines[2].y, baseline + large_offset));
    assert!(pt_near(lines[2].thickness, large_thickness));
}

#[test]
fn styled_flags_inherit_and_some_false_turns_them_off() {
    let off = RunStyle {
        bold: Some(false),
        underline: Some(false),
        ..RunStyle::default()
    };
    let faces = Faces {
        bold: true,
        ..REGULAR_ONLY
    };
    let Some(rendered) = render(
        &faces,
        vec![Block::Styled {
            style: RunStyle {
                bold: Some(true),
                underline: Some(true),
                ..RunStyle::default()
            },
            children: vec![para(vec![run("A", RunStyle::default()), run("B", off)])],
        }],
    ) else {
        return;
    };
    let page = &rendered.unwrap().pages()[0];
    let glyphs = glyph_runs(page);
    assert_eq!(
        glyphs
            .iter()
            .map(|g| (g.text.as_str(), g.font))
            .collect::<Vec<_>>(),
        [("A", 1), ("B", 0)]
    );
    let lines = strokes(page);
    assert_eq!(lines.len(), 1, "{lines:?}");
    assert!(pt_near(lines[0].x0, glyphs[0].x));
    assert!(pt_near(lines[0].x1, glyphs[1].x));
}

#[test]
fn italic_and_bold_italic_pick_their_own_faces() {
    let italic = RunStyle {
        italic: Some(true),
        ..RunStyle::default()
    };
    let both = RunStyle {
        bold: Some(true),
        ..italic
    };
    let faces = Faces {
        bold: true,
        italic: true,
        bold_italic: true,
    };
    let Some(rendered) = render(&faces, vec![para(vec![run("A", italic), run("B", both)])]) else {
        return;
    };
    let page = &rendered.unwrap().pages()[0];
    assert_eq!(
        glyph_runs(page)
            .iter()
            .map(|g| (g.text.as_str(), g.font))
            .collect::<Vec<_>>(),
        [("A", 2), ("B", 3)]
    );
}

#[test]
fn missing_italic_face_fails_the_document() {
    let italic = RunStyle {
        italic: Some(true),
        ..RunStyle::default()
    };
    let Some(result) = render(&REGULAR_ONLY, vec![para(vec![run("A", italic)])]) else {
        return;
    };
    assert_eq!(kinds(result), [ErrorKind::ItalicFaceRequired]);
}

#[test]
fn missing_bold_italic_face_fails_even_with_bold_and_italic() {
    let both = RunStyle {
        bold: Some(true),
        italic: Some(true),
        ..RunStyle::default()
    };
    let faces = Faces {
        bold: true,
        italic: true,
        bold_italic: false,
    };
    let Some(result) = render(&faces, vec![para(vec![run("A", both)])]) else {
        return;
    };
    assert_eq!(kinds(result), [ErrorKind::BoldItalicFaceRequired]);
}

#[test]
fn empty_text_skips_the_face_check() {
    let italic = RunStyle {
        italic: Some(true),
        ..RunStyle::default()
    };
    let Some(result) = render(
        &REGULAR_ONLY,
        vec![para(vec![run("", italic), run("A", RunStyle::default())])],
    ) else {
        return;
    };
    assert!(result.is_ok());
}

#[test]
fn underline_covers_an_inline_space_but_not_a_dropped_leading_one() {
    let Some(rendered) = render(&REGULAR_ONLY, vec![para(vec![run(" AB CD", underlined())])])
    else {
        return;
    };
    let page = &rendered.unwrap().pages()[0];
    let glyphs = glyph_runs(page);
    assert_eq!(glyphs.len(), 1);
    assert_eq!(glyphs[0].text, "AB CD");
    let lines = strokes(page);
    assert_eq!(lines.len(), 1, "{lines:?}");
    assert!(pt_near(lines[0].x0, glyphs[0].x));
    assert!(pt_near(lines[0].x1, glyphs[0].x + glyphs[0].width));
}

#[test]
fn a_blank_keeps_its_own_stroke_inside_underlined_text() {
    let blank = InlineRun::Blank(BlankSpec {
        width: BlankWidth::Fixed(nn(20.0)),
        thickness: Some(pos(3.0)),
        color: None,
        baseline_offset: Some(nn(5.0)),
    });
    let Some(rendered) = render(
        &REGULAR_ONLY,
        vec![para(vec![
            run("A", underlined()),
            blank,
            run("B", underlined()),
        ])],
    ) else {
        return;
    };
    let page = &rendered.unwrap().pages()[0];
    let glyphs = glyph_runs(page);
    let mut lines = strokes(page);
    lines.sort_by(|a, b| a.x0.total_cmp(&b.x0));
    assert_eq!(lines.len(), 3, "{lines:?}");
    let (offset, thickness) = underline_at(12.0);
    let baseline = glyphs[0].baseline;
    let a_end = glyphs[0].x + glyphs[0].width;

    assert!(pt_near(lines[0].x0, glyphs[0].x));
    assert!(pt_near(lines[0].x1, a_end));
    assert!(pt_near(lines[0].y, baseline + offset));

    assert!(pt_near(lines[1].x0, a_end));
    assert!(pt_near(lines[1].x1, a_end + 20.0));
    assert!(pt_near(lines[1].y, baseline + 5.0));
    assert!(pt_near(lines[1].thickness, 3.0));

    assert!(pt_near(lines[2].x0, glyphs[1].x));
    assert!(pt_near(lines[2].x0, a_end + 20.0));
    assert!(pt_near(lines[2].thickness, thickness));
}

#[test]
fn tracking_widens_gaps_between_clusters_but_not_after_the_last() {
    let Some(bytes) = common::test_font_bytes() else {
        return;
    };
    let advance = common::advance_pt(&bytes, '字', 12.0);
    let end_aligned = |runs| {
        Block::Paragraph(ParagraphSpec {
            runs,
            align: Alignment::End,
            ..ParagraphSpec::default()
        })
    };
    let tracked_underline = RunStyle {
        underline: Some(true),
        ..tracked(2.0)
    };
    let rendered = render(
        &REGULAR_ONLY,
        vec![
            end_aligned(vec![run("字字字", tracked_underline)]),
            end_aligned(vec![run("字", RunStyle::default())]),
        ],
    )
    .unwrap()
    .unwrap();
    let page = &rendered.pages()[0];
    let glyphs = glyph_runs(page);
    let tracked_run = &glyphs[0];
    let plain_run = &glyphs[1];

    assert_eq!(tracked_run.advances.len(), 3);
    assert!(pt_near(tracked_run.advances[0], advance + 2.0));
    assert!(pt_near(tracked_run.advances[1], advance + 2.0));
    assert!(pt_near(tracked_run.advances[2], advance));
    let right_edge = plain_run.x + plain_run.width;
    assert!(pt_near(tracked_run.x + tracked_run.width, right_edge));

    let lines = strokes(page);
    assert_eq!(lines.len(), 1, "{lines:?}");
    assert!(pt_near(lines[0].x0, tracked_run.x));
    assert!(pt_near(lines[0].x1, right_edge));
}

#[test]
fn negative_tracking_shrinks_the_receiving_glyph() {
    let Some(bytes) = common::test_font_bytes() else {
        return;
    };
    let advance = common::advance_pt(&bytes, '字', 12.0);
    let rendered = render(&REGULAR_ONLY, vec![para(vec![run("字字", tracked(-3.0))])])
        .unwrap()
        .unwrap();
    let glyphs = glyph_runs(&rendered.pages()[0]);
    assert!(pt_near(glyphs[0].advances[0], advance - 3.0));
    assert!(pt_near(glyphs[0].advances[1], advance));
}

#[test]
fn tracking_below_the_receiving_advance_fails_once() {
    let Some(bytes) = common::test_font_bytes() else {
        return;
    };
    let advance = common::advance_pt(&bytes, '字', 12.0);
    let tracking = -(advance + 1.0);
    let result = render(
        &REGULAR_ONLY,
        vec![para(vec![run("字字", tracked(tracking))])],
    )
    .unwrap();
    let kinds = kinds(result);
    assert_eq!(kinds.len(), 1, "{kinds:?}");
    let ErrorKind::NegativeTracking {
        advance: got_advance,
        tracking: got_tracking,
    } = kinds[0]
    else {
        panic!("expected NegativeTracking, got {kinds:?}");
    };
    assert!(pt_near(got_advance, advance));
    assert!(pt_near(got_tracking, tracking));
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
enum FaceKey {
    First,
    Second,
}

fn read_font(var: &str) -> Option<Vec<u8>> {
    let path = std::env::var_os(var)?;
    Some(std::fs::read(&path).unwrap_or_else(|error| {
        panic!(
            "read {var} {}: {error}",
            std::path::Path::new(&path).display()
        )
    }))
}

/// `(ascent, descent, leading, units_per_em)` from skrifa, unscaled.
fn face_box(bytes: &[u8]) -> (f32, f32, f32, f32) {
    let font = FontRef::from_index(bytes, 0).unwrap();
    let metrics = font.metrics(Size::unscaled(), LocationRef::default());
    (
        metrics.ascent,
        metrics.descent,
        metrics.leading,
        f32::from(metrics.units_per_em),
    )
}

fn scale_box(metrics: (f32, f32, f32, f32), size: f32) -> (f32, f32, f32) {
    let scale = size / metrics.3;
    (metrics.0 * scale, metrics.1 * scale, metrics.2 * scale)
}

fn fold_height(ascent: f32, descent: f32, leading: f32, largest: f32, factor: f32) -> f32 {
    (ascent - descent + leading).max(largest * factor)
}

fn layout_body(resources: Resources<FaceKey>, body: Vec<Block<FaceKey>>) -> Rendered {
    layout_spec(DocumentSpec {
        page_size: PageSize::custom(pdfcraft_pdf::Size::try_pt(200.0, 400.0).unwrap()),
        resources,
        limits: pdfcraft_pdf::Limits::default(),
        info: pdfcraft_pdf::spec::DocumentInfo::default(),
        sections: vec![SectionSpec {
            body,
            ..SectionSpec::default()
        }],
    })
    .expect("document lays out")
}

#[test]
fn two_faces_on_one_line_use_each_faces_own_size() {
    let Some(first_bytes) = read_font("PDFCRAFT_PDF_FONT") else {
        return;
    };
    let Some(second_bytes) = read_font("PDFCRAFT_CJK_FONT") else {
        return;
    };
    let first_box = face_box(&first_bytes);
    let second_box = face_box(&second_bytes);
    let first_ratio = first_box.0 / first_box.3;
    let second_ratio = second_box.0 / second_box.3;
    assert!(
        (first_ratio - second_ratio).abs() > 1.0e-4,
        "the two fixture faces need different ascent per em"
    );
    let (small_bytes, small_key, large_bytes, large_key, small_box, large_box) =
        if first_ratio < second_ratio {
            (
                first_bytes,
                FaceKey::First,
                second_bytes,
                FaceKey::Second,
                first_box,
                second_box,
            )
        } else {
            (
                second_bytes,
                FaceKey::Second,
                first_bytes,
                FaceKey::First,
                second_box,
                first_box,
            )
        };
    let (small_ascent, _, _) = scale_box(small_box, 12.0);
    let (large_ascent, large_descent, large_leading) = scale_box(large_box, 18.0);
    let (small_at_18, small_descent_18, small_leading_18) = scale_box(small_box, 18.0);
    let ascent = small_ascent.max(large_ascent);
    let (descent_12, leading_12) = {
        let (_, descent, leading) = scale_box(small_box, 12.0);
        (descent.min(large_descent), leading.max(large_leading))
    };
    let height = fold_height(ascent, descent_12, leading_12, 18.0, 1.2);
    let wrong_ascent = small_at_18;
    let wrong_height = fold_height(small_at_18, small_descent_18, small_leading_18, 18.0, 1.2);
    assert!((ascent - wrong_ascent).abs() > 0.2);
    let mut fonts = FontRegistry::new();
    fonts.register(
        small_key,
        FontFamily {
            regular: FontFace::new(small_bytes, 0),
            bold: None,
            italic: None,
            bold_italic: None,
        },
    );
    fonts.register(
        large_key,
        FontFamily {
            regular: FontFace::new(large_bytes, 0),
            bold: None,
            italic: None,
            bold_italic: None,
        },
    );
    let run = |text: &str, font: FaceKey, size: f32| InlineRun::Text {
        text: text.into(),
        font,
        style: RunStyle {
            font_size: Some(pos(size)),
            ..RunStyle::default()
        },
    };
    let rendered = layout_body(
        Resources::with_fonts(fonts),
        vec![
            Block::Paragraph(ParagraphSpec {
                runs: vec![run("A", small_key, 12.0), run("B", large_key, 18.0)],
                line_height_factor: Some(1.2),
                ..ParagraphSpec::default()
            }),
            Block::HLine(HLineSpec {
                thickness: pos(1.0),
                ..HLineSpec::default()
            }),
        ],
    );
    let page = &rendered.pages()[0];
    let glyphs = glyph_runs(page);
    assert_eq!(glyphs.len(), 2);
    assert!(pt_near(glyphs[0].baseline, ascent));
    assert!(pt_near(glyphs[1].baseline, ascent));
    assert!(!pt_near(glyphs[0].baseline, wrong_ascent));
    let lines = strokes(page);
    assert_eq!(lines.len(), 1, "{lines:?}");
    assert!(pt_near(lines[0].y, height + 0.5));
    assert!(!pt_near(lines[0].y, wrong_height + 0.5));
}

#[test]
fn blank_line_after_bold_text_uses_the_bold_face_at_paragraph_size() {
    let Some(regular_bytes) = read_font("PDFCRAFT_PDF_FONT") else {
        return;
    };
    let Some(bold_bytes) = read_font("PDFCRAFT_CJK_FONT") else {
        return;
    };
    let regular = face_box(&regular_bytes);
    let bold = face_box(&bold_bytes);
    assert!((regular.0 / regular.3 - bold.0 / bold.3).abs() > 1.0e-4);
    let (line_ascent, line_descent, line_leading) = scale_box(bold, 22.0);
    let line_height = fold_height(line_ascent, line_descent, line_leading, 22.0, 1.2);
    let (blank_ascent, _, _) = scale_box(bold, 14.0);
    let (regular_ascent, _, _) = scale_box(regular, 14.0);
    let (run_ascent, _, _) = scale_box(bold, 22.0);
    let expected = line_height + blank_ascent + 1.0;
    let at_run_size = line_height + run_ascent + 1.0;
    let at_regular = line_height + regular_ascent + 1.0;
    assert!((expected - at_run_size).abs() > 0.2);
    assert!((expected - at_regular).abs() > 0.2);
    let rendered = layout_spec(DocumentSpec {
        page_size: PageSize::custom(pdfcraft_pdf::Size::try_pt(200.0, 400.0).unwrap()),
        resources: Resources::with_fonts(FontRegistry::single(FontFamily {
            regular: FontFace::new(regular_bytes, 0),
            bold: Some(FontFace::new(bold_bytes, 0)),
            italic: None,
            bold_italic: None,
        })),
        limits: pdfcraft_pdf::Limits::default(),
        info: pdfcraft_pdf::spec::DocumentInfo::default(),
        sections: vec![SectionSpec {
            body: vec![Block::Paragraph(ParagraphSpec {
                runs: vec![
                    InlineRun::Text {
                        text: "A\n".into(),
                        font: (),
                        style: RunStyle {
                            bold: Some(true),
                            font_size: Some(pos(22.0)),
                            ..RunStyle::default()
                        },
                    },
                    InlineRun::Blank(BlankSpec {
                        width: BlankWidth::Fixed(nn(20.0)),
                        thickness: None,
                        color: None,
                        baseline_offset: Some(nn(1.0)),
                    }),
                ],
                font_size: Some(pos(14.0)),
                line_height_factor: Some(1.2),
                ..ParagraphSpec::default()
            })],
            ..SectionSpec::default()
        }],
    })
    .expect("bold blank line lays out");
    let page = &rendered.pages()[0];
    let glyphs = glyph_runs(page);
    let letter = glyphs.iter().find(|glyph| glyph.text == "A").expect("A");
    assert!(pt_near(letter.baseline, line_ascent));
    let empty: Vec<_> = glyphs
        .iter()
        .filter(|glyph| glyph.text.is_empty())
        .collect();
    assert_eq!(empty.len(), 1);
    assert_eq!(empty[0].font, 1);
    assert!(pt_near(empty[0].baseline, line_height + blank_ascent));
    let lines = strokes(page);
    assert_eq!(lines.len(), 1, "{lines:?}");
    assert!(pt_near(lines[0].y, expected));
    assert!(!pt_near(lines[0].y, at_run_size));
    assert!(!pt_near(lines[0].y, at_regular));
}

#[test]
fn blank_only_paragraph_reads_no_font() {
    let rendered = layout_spec(DocumentSpec {
        page_size: PageSize::custom(pdfcraft_pdf::Size::try_pt(200.0, 80.0).unwrap()),
        resources: Resources::with_fonts(FontRegistry::<()>::new()),
        limits: pdfcraft_pdf::Limits::default(),
        info: pdfcraft_pdf::spec::DocumentInfo::default(),
        sections: vec![SectionSpec {
            body: vec![
                Block::Paragraph(ParagraphSpec {
                    runs: vec![InlineRun::Blank(BlankSpec {
                        width: BlankWidth::Fixed(nn(20.0)),
                        thickness: None,
                        color: None,
                        baseline_offset: Some(nn(1.0)),
                    })],
                    font_size: Some(pos(14.0)),
                    line_height_factor: Some(1.2),
                    ..ParagraphSpec::default()
                }),
                Block::HLine(HLineSpec {
                    thickness: pos(1.0),
                    ..HLineSpec::default()
                }),
            ],
            ..SectionSpec::default()
        }],
    })
    .expect("blank-only paragraph needs no font");
    let page = &rendered.pages()[0];
    assert!(glyph_runs(page).is_empty());
    let lines = strokes(page);
    assert_eq!(lines.len(), 2, "{lines:?}");
    let mut ys: Vec<f32> = lines.iter().map(|line| line.y).collect();
    ys.sort_by(f32::total_cmp);
    assert!(pt_near(ys[0], 15.0));
    assert!(pt_near(ys[1], 17.3));
    #[cfg(feature = "backend-krilla")]
    {
        let pdf = pdfcraft_pdf::to_pdf(rendered).expect("blank-only paragraph emits pdf");
        assert!(pdf.starts_with(b"%PDF"));
    }
}
