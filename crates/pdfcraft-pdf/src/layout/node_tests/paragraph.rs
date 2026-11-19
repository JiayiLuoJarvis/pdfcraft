use std::num::NonZeroU8;

use crate::canvas::{pt_near, CanvasCommand, RecordingCanvas};
use crate::fonts::{fixture_bytes, EmOffset, FontId, FontSource, FontStore};
use crate::geom::{Finite, NonNegative, PathVerb, Point, Positive};
use crate::layout::decorate::Decorations;
use crate::layout::flow::{FlowNode, SpacerNode};
use crate::layout::frame::{
    draw_fragment, Fragment, FragmentCommand, Frame, LayoutCtx, LayoutLink, Split,
};
use crate::layout::paragraph::{
    BlankWidth, InlineBlank, InlineItem, ParagraphNode, TextNode, TextRun,
};
use crate::layout::style::{Decoration, GlyphStyle, Style, TextAlign};
use crate::layout::test_util::{fonts_and_style, page, render};
use crate::text::measure::{FaceExtent, LineMetrics};

fn uri(text: &str) -> LayoutLink {
    LayoutLink::uri(text)
}

fn text_item(text: &str, style: &Style, font: FontId, link: Option<&str>) -> InlineItem {
    let glyph = GlyphStyle {
        font,
        ..GlyphStyle::plain(style)
    };
    InlineItem::Text(TextRun::new(text, glyph).with_link(link.map(uri)))
}

fn fit(
    items: Vec<InlineItem>,
    style: &Style,
    fonts: &mut FontStore,
    width: f32,
    height: f32,
) -> Fragment {
    let node = TextNode::from_items(items, style.clone());
    let mut errors = crate::error::ErrorBag::default();
    let mut ctx = LayoutCtx {
        natural: false,
        fonts,
        errors: &mut errors,
    };
    let Split::Fit { fragment, .. } = node
        .split(
            &mut ctx,
            Frame {
                width: Positive::pt(width).unwrap(),
                height: NonNegative::pt(height).unwrap(),
                full_height: Positive::pt(height).unwrap(),
            },
        )
        .unwrap()
    else {
        panic!("paragraph fits");
    };
    fragment
}

struct Hit {
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    target: String,
}

fn hits(fragment: &Fragment) -> Vec<Hit> {
    fragment
        .commands
        .iter()
        .filter_map(|command| match command {
            FragmentCommand::Link { rect, link, .. } => Some(Hit {
                x: rect.origin().x().as_pt(),
                y: rect.origin().y().as_pt(),
                width: rect.size().width().as_pt(),
                height: rect.size().height().as_pt(),
                target: link.uri_str().unwrap_or("").to_owned(),
            }),
            FragmentCommand::Glyphs(_)
            | FragmentCommand::Fill { .. }
            | FragmentCommand::Stroke { .. }
            | FragmentCommand::Image { .. }
            | FragmentCommand::PushTransform { .. }
            | FragmentCommand::PopTransform => None,
        })
        .collect()
}

fn glyph_spans(fragment: &Fragment) -> Vec<(String, f32, f32)> {
    fragment
        .commands
        .iter()
        .filter_map(|command| match command {
            FragmentCommand::Glyphs(run) => {
                let width = run
                    .glyphs
                    .iter()
                    .map(|glyph| glyph.advance_norm * run.font_size.as_pt())
                    .sum();
                Some((run.text.clone(), run.origin.x().as_pt(), width))
            }
            FragmentCommand::Fill { .. }
            | FragmentCommand::Stroke { .. }
            | FragmentCommand::Image { .. }
            | FragmentCommand::Link { .. }
            | FragmentCommand::PushTransform { .. }
            | FragmentCommand::PopTransform => None,
        })
        .collect()
}

fn body_line(fonts: &FontStore, style: &Style) -> f32 {
    let face = fonts
        .metrics(style.font.expect("test style has a face"))
        .unwrap();
    LineMetrics::new(
        FaceExtent::scaled(face, style.font_size).into(),
        style.line_height_factor,
    )
    .unwrap()
    .line_height
    .as_pt()
}

#[test]
fn centered_fill_blank_starts_at_the_frame_x() {
    let Some((mut fonts, mut style)) = fonts_and_style() else {
        return;
    };
    style.align = TextAlign::Center;
    let node = TextNode::from_items(
        vec![
            InlineItem::Text(TextRun::new("A", GlyphStyle::plain(&style))),
            InlineItem::Blank(InlineBlank::new(BlankWidth::ToEndOfLine)),
        ],
        style,
    );
    let mut errors = crate::error::ErrorBag::default();
    let mut ctx = LayoutCtx {
        natural: false,
        fonts: &mut fonts,
        errors: &mut errors,
    };
    let Split::Fit { fragment, .. } = node
        .split(
            &mut ctx,
            Frame {
                width: Positive::pt(120.0).unwrap(),
                height: NonNegative::pt(400.0).unwrap(),
                full_height: Positive::pt(400.0).unwrap(),
            },
        )
        .unwrap()
    else {
        panic!("paragraph fits");
    };
    let glyph = fragment.commands.iter().find_map(|command| match command {
        FragmentCommand::Glyphs(run) if run.text == "A" => Some(run.origin.x().as_pt()),
        FragmentCommand::Glyphs(_)
        | FragmentCommand::Fill { .. }
        | FragmentCommand::Stroke { .. }
        | FragmentCommand::Image { .. }
        | FragmentCommand::Link { .. }
        | FragmentCommand::PushTransform { .. }
        | FragmentCommand::PopTransform => None,
    });
    assert_eq!(glyph.map(f32::to_bits), Some(0.0f32.to_bits()));
    let end = fragment.commands.iter().find_map(|command| match command {
        FragmentCommand::Stroke { path, .. } => path.verbs().last().map(|verb| match verb {
            PathVerb::LineTo(point) => point.x().as_pt(),
            PathVerb::MoveTo(_) | PathVerb::CubicTo { .. } | PathVerb::Close => f32::NAN,
        }),
        FragmentCommand::Glyphs(_)
        | FragmentCommand::Fill { .. }
        | FragmentCommand::Image { .. }
        | FragmentCommand::Link { .. }
        | FragmentCommand::PushTransform { .. }
        | FragmentCommand::PopTransform => None,
    });
    assert_eq!(end.map(f32::to_bits), Some(120.0f32.to_bits()));
}

#[test]
fn partial_link_rect_covers_only_the_linked_glyphs() {
    let Some((mut fonts, style)) = fonts_and_style() else {
        return;
    };
    let line = body_line(&fonts, &style);
    let fragment = fit(
        vec![
            text_item(
                "A",
                &style,
                style.font.expect("test style has a face"),
                None,
            ),
            text_item(
                "BC",
                &style,
                style.font.expect("test style has a face"),
                Some("https://example.com/bc"),
            ),
            text_item(
                "D",
                &style,
                style.font.expect("test style has a face"),
                None,
            ),
        ],
        &style,
        &mut fonts,
        200.0,
        80.0,
    );
    let glyphs = glyph_spans(&fragment);
    assert_eq!(
        glyphs
            .iter()
            .map(|(text, _, _)| text.as_str())
            .collect::<Vec<_>>(),
        ["A", "BC", "D"]
    );
    let links = hits(&fragment);
    assert_eq!(links.len(), 1);
    assert_eq!(links[0].target, "https://example.com/bc");
    assert!(pt_near(links[0].x, glyphs[1].1));
    assert!(pt_near(links[0].width, glyphs[1].2));
    assert!(pt_near(glyphs[1].1, glyphs[0].2));
    assert_eq!(links[0].y.to_bits(), 0.0f32.to_bits());
    assert_eq!(links[0].height.to_bits(), line.to_bits());
}

#[test]
fn wrapped_link_emits_one_rect_per_visual_line() {
    let Some((mut fonts, style)) = fonts_and_style() else {
        return;
    };
    let line = body_line(&fonts, &style);
    let fragment = fit(
        vec![text_item(
            "第第",
            &style,
            style.font.expect("test style has a face"),
            Some("https://example.com/wrap"),
        )],
        &style,
        &mut fonts,
        12.0,
        80.0,
    );
    let links = hits(&fragment);
    assert_eq!(links.len(), 2);
    assert!(links
        .iter()
        .all(|hit| hit.target == "https://example.com/wrap"));
    assert_eq!(links[0].x.to_bits(), 0.0f32.to_bits());
    assert_eq!(links[0].y.to_bits(), 0.0f32.to_bits());
    assert_eq!(links[0].width.to_bits(), 12.0f32.to_bits());
    assert_eq!(links[0].height.to_bits(), line.to_bits());
    assert_eq!(links[1].x.to_bits(), 0.0f32.to_bits());
    assert_eq!(links[1].y.to_bits(), line.to_bits());
    assert_eq!(links[1].width.to_bits(), 12.0f32.to_bits());
    assert_eq!(links[1].height.to_bits(), line.to_bits());
}

#[test]
fn same_link_keeps_body_and_bold_in_one_rect() {
    let Some((mut fonts, style)) = fonts_and_style() else {
        return;
    };
    let Some(bold_bytes) = fixture_bytes() else {
        return;
    };
    let bold = fonts
        .register(FontSource {
            bytes: bold_bytes,
            index: 0,
        })
        .unwrap();
    let fragment = fit(
        vec![
            text_item(
                "A",
                &style,
                style.font.expect("test style has a face"),
                Some("u"),
            ),
            text_item("B", &style, bold, Some("u")),
            text_item(
                "C",
                &style,
                style.font.expect("test style has a face"),
                Some("u"),
            ),
        ],
        &style,
        &mut fonts,
        200.0,
        80.0,
    );
    let glyphs = glyph_spans(&fragment);
    assert_eq!(glyphs.len(), 3);
    let links = hits(&fragment);
    assert_eq!(links.len(), 1);
    assert_eq!(links[0].target, "u");
    assert_eq!(links[0].x.to_bits(), 0.0f32.to_bits());
    assert!(pt_near(
        links[0].width,
        glyphs.iter().map(|(_, _, width)| width).sum()
    ));
}

#[test]
fn link_changes_flush_and_a_repeat_uri_stays_its_own_rect() {
    let Some((mut fonts, style)) = fonts_and_style() else {
        return;
    };
    let fragment = fit(
        vec![
            text_item(
                "A",
                &style,
                style.font.expect("test style has a face"),
                Some("a"),
            ),
            text_item(
                "B",
                &style,
                style.font.expect("test style has a face"),
                Some("b"),
            ),
            text_item(
                "C",
                &style,
                style.font.expect("test style has a face"),
                None,
            ),
            text_item(
                "D",
                &style,
                style.font.expect("test style has a face"),
                Some("a"),
            ),
        ],
        &style,
        &mut fonts,
        200.0,
        80.0,
    );
    let glyphs = glyph_spans(&fragment);
    let links = hits(&fragment);
    assert_eq!(
        links
            .iter()
            .map(|hit| hit.target.as_str())
            .collect::<Vec<_>>(),
        ["a", "b", "a"]
    );
    assert!(pt_near(links[0].x, glyphs[0].1));
    assert!(pt_near(links[0].width, glyphs[0].2));
    assert!(pt_near(links[1].x, glyphs[1].1));
    assert!(pt_near(links[1].width, glyphs[1].2));
    assert!(pt_near(links[2].x, glyphs[3].1));
    assert!(pt_near(links[2].width, glyphs[3].2));
    assert!(links[0].x + links[0].width <= links[1].x + f32::EPSILON);
    assert!(links[1].x + links[1].width < links[2].x);
}

#[test]
fn adjacent_runs_with_the_same_link_share_one_rect() {
    let Some((mut fonts, style)) = fonts_and_style() else {
        return;
    };
    let fragment = fit(
        vec![
            text_item(
                "A",
                &style,
                style.font.expect("test style has a face"),
                Some("u"),
            ),
            text_item(
                "B",
                &style,
                style.font.expect("test style has a face"),
                Some("u"),
            ),
        ],
        &style,
        &mut fonts,
        200.0,
        80.0,
    );
    let glyphs = glyph_spans(&fragment);
    let links = hits(&fragment);
    assert_eq!(glyphs.len(), 1);
    assert_eq!(glyphs[0].0, "AB");
    assert_eq!(links.len(), 1);
    assert!(pt_near(links[0].width, glyphs[0].2));
}

#[test]
fn blank_splits_a_link_and_is_not_inside_either_rect() {
    let Some((mut fonts, style)) = fonts_and_style() else {
        return;
    };
    let fragment = fit(
        vec![
            text_item(
                "A",
                &style,
                style.font.expect("test style has a face"),
                Some("u"),
            ),
            InlineItem::Blank(InlineBlank::new(BlankWidth::Fixed(
                NonNegative::pt(30.0).unwrap(),
            ))),
            text_item(
                "B",
                &style,
                style.font.expect("test style has a face"),
                Some("u"),
            ),
        ],
        &style,
        &mut fonts,
        200.0,
        80.0,
    );
    let links = hits(&fragment);
    assert_eq!(links.len(), 2);
    assert!(fragment
        .commands
        .iter()
        .any(|command| matches!(command, FragmentCommand::Stroke { .. })));
    assert!(pt_near(links[1].x - (links[0].x + links[0].width), 30.0));
}

#[test]
fn link_rect_follows_alignment_and_first_line_indent() {
    let Some((mut fonts, mut style)) = fonts_and_style() else {
        return;
    };
    style.align = TextAlign::Center;
    let centered = fit(
        vec![text_item(
            "A",
            &style,
            style.font.expect("test style has a face"),
            Some("u"),
        )],
        &style,
        &mut fonts,
        120.0,
        80.0,
    );
    let center_glyphs = glyph_spans(&centered);
    let center_links = hits(&centered);
    assert!(pt_near(center_links[0].x, center_glyphs[0].1));
    assert!(center_links[0].x > 0.0);

    style.align = TextAlign::End;
    let ended = fit(
        vec![text_item(
            "A",
            &style,
            style.font.expect("test style has a face"),
            Some("u"),
        )],
        &style,
        &mut fonts,
        120.0,
        80.0,
    );
    let end_glyphs = glyph_spans(&ended);
    let end_links = hits(&ended);
    assert!(pt_near(end_links[0].x, end_glyphs[0].1));
    assert!(pt_near(end_links[0].x + end_links[0].width, 120.0));

    style.align = TextAlign::Start;
    style.first_line_indent = NonNegative::pt(10.0).unwrap();
    let indented = fit(
        vec![text_item(
            "A",
            &style,
            style.font.expect("test style has a face"),
            Some("u"),
        )],
        &style,
        &mut fonts,
        120.0,
        80.0,
    );
    let indent_links = hits(&indented);
    assert!(pt_near(indent_links[0].x, 10.0));
    assert!(pt_near(indent_links[0].x, glyph_spans(&indented)[0].1));
}

#[test]
fn offset_and_paint_move_the_link_rect() {
    let Some((mut fonts, style)) = fonts_and_style() else {
        return;
    };
    let fragment = fit(
        vec![text_item(
            "A",
            &style,
            style.font.expect("test style has a face"),
            Some("https://example.com/a"),
        )],
        &style,
        &mut fonts,
        80.0,
        40.0,
    );
    let local = &hits(&fragment)[0];
    let shifted = fragment
        .clone()
        .with_offset(Finite::pt(10.0).unwrap(), Finite::pt(20.0).unwrap())
        .unwrap();
    let moved = &hits(&shifted)[0];
    assert!(pt_near(moved.x, local.x + 10.0));
    assert!(pt_near(moved.y, local.y + 20.0));
    assert_eq!(moved.target, local.target);

    let mut rec = RecordingCanvas::new();
    draw_fragment(&fragment, &mut rec, Point::try_pt(3.0, 4.0).unwrap()).unwrap();
    let painted: Vec<_> = rec
        .commands()
        .iter()
        .filter_map(|command| match command {
            CanvasCommand::AnnotateLink { rect, dest } => Some((
                rect.origin().x().as_pt(),
                rect.origin().y().as_pt(),
                dest.uri_str().unwrap_or("").to_owned(),
            )),
            CanvasCommand::SetDocumentInfo(_)
            | CanvasCommand::StartPage(_)
            | CanvasCommand::FinishPage
            | CanvasCommand::FillPath { .. }
            | CanvasCommand::StrokePath { .. }
            | CanvasCommand::DrawImage { .. }
            | CanvasCommand::DrawGlyphRun { .. }
            | CanvasCommand::PushTransform { .. }
            | CanvasCommand::PopTransform => None,
        })
        .collect();
    assert_eq!(painted.len(), 1);
    assert!(pt_near(painted[0].0, local.x + 3.0));
    assert!(pt_near(painted[0].1, local.y + 4.0));
    assert_eq!(painted[0].2, "https://example.com/a");
}

#[test]
fn empty_linked_text_emits_no_rect() {
    let Some((mut fonts, style)) = fonts_and_style() else {
        return;
    };
    let fragment = fit(
        vec![text_item(
            "",
            &style,
            style.font.expect("test style has a face"),
            Some("u"),
        )],
        &style,
        &mut fonts,
        80.0,
        40.0,
    );
    assert!(hits(&fragment).is_empty());
}

#[test]
fn merged_underline_uses_the_first_segment_and_draws_1pt() {
    let Some((mut fonts, style)) = fonts_and_style() else {
        return;
    };
    assert_eq!(style.font_size.as_pt().to_bits(), 12.0f32.to_bits());
    let size = style.font_size;
    let first = Decoration::at(EmOffset::new(-0.25).unwrap(), size).unwrap();
    let second = Decoration::at(EmOffset::new(-0.8).unwrap(), size).unwrap();
    let face = style.font.expect("test style has a face");
    let item = |text: &str, line: Decoration| {
        InlineItem::Text(TextRun::new(
            text,
            GlyphStyle {
                font: face,
                underline: Some(line),
                ..GlyphStyle::plain(&style)
            },
        ))
    };
    let node = TextNode::from_items(vec![item("A", first), item("B", second)], style);
    let (result, rec) = render(
        FlowNode::Text(node),
        &mut fonts,
        page(200.0, 400.0),
        &Decorations::none(),
    );
    result.expect("line lays out");
    let recorded = &rec.pages()[0];
    let mut baseline = None;
    let mut drawn = Vec::new();
    for op in recorded.ops() {
        match op {
            CanvasCommand::DrawGlyphRun { run } => {
                baseline.get_or_insert(run.origin.y().as_pt());
            }
            CanvasCommand::StrokePath { path, stroke, .. } => {
                if let [PathVerb::MoveTo(from), PathVerb::LineTo(_)] = path.verbs() {
                    drawn.push((from.y().as_pt(), stroke.width().as_pt()));
                }
            }
            CanvasCommand::SetDocumentInfo(_)
            | CanvasCommand::StartPage(_)
            | CanvasCommand::FinishPage
            | CanvasCommand::FillPath { .. }
            | CanvasCommand::DrawImage { .. }
            | CanvasCommand::AnnotateLink { .. }
            | CanvasCommand::PushTransform { .. }
            | CanvasCommand::PopTransform => {}
        }
    }
    assert_eq!(drawn.len(), 1, "{drawn:?}");
    let baseline = baseline.expect("glyphs");
    // -(offset/upem)*size + 0.5 at 12 pt. Offsets are already in em.
    assert!(pt_near(drawn[0].0, baseline + 3.5));
    assert!(!pt_near(drawn[0].0, baseline + 10.1));
    assert!(pt_near(drawn[0].1, 1.0));
}

#[test]
fn justify_middle_line_stretches_last_line_matches_start() {
    let Some((mut fonts, mut style)) = fonts_and_style() else {
        return;
    };
    // Enough CJK to wrap. Frame 130 leaves remainder when glyphs are ~12pt wide
    // (a 120pt frame packs exactly 10 fullwidth cells and would not stretch).
    let text = "两端对齐的中文段落用来检查中间行拉开而末行保持靠左。";
    let frame_w = 130.0;
    style.align = TextAlign::Justify;
    let justified = fit(
        vec![InlineItem::Text(TextRun::new(
            text,
            GlyphStyle::plain(&style),
        ))],
        &style,
        &mut fonts,
        frame_w,
        400.0,
    );
    style.align = TextAlign::Start;
    let started = fit(
        vec![InlineItem::Text(TextRun::new(
            text,
            GlyphStyle::plain(&style),
        ))],
        &style,
        &mut fonts,
        frame_w,
        400.0,
    );
    let just_lines = glyph_spans(&justified);
    let start_lines = glyph_spans(&started);
    assert!(
        just_lines.len() >= 2,
        "need a wrapped paragraph, got {}",
        just_lines.len()
    );
    assert_eq!(just_lines.len(), start_lines.len());
    // Intermediate lines stretch: painted end closer to frame right than Start twin.
    for index in 0..just_lines.len() - 1 {
        let just_end = just_lines[index].1 + just_lines[index].2;
        let start_end = start_lines[index].1 + start_lines[index].2;
        assert!(
            just_end > start_end + 0.5,
            "line {index}: justify end {just_end} should exceed start end {start_end}"
        );
        assert!(
            (just_end - frame_w).abs() < 0.05 || just_end <= frame_w + 0.05,
            "line {index}: justify end {just_end} should meet the frame {frame_w}"
        );
    }
    // Last line stays Start-shaped.
    let last = just_lines.len() - 1;
    assert!(pt_near(just_lines[last].1, start_lines[last].1));
    assert!(pt_near(just_lines[last].2, start_lines[last].2));
}

#[test]
fn justify_hard_break_line_matches_start() {
    let Some((mut fonts, mut style)) = fonts_and_style() else {
        return;
    };
    let text = "第一行内容足够短\n第二行";
    style.align = TextAlign::Justify;
    let justified = fit(
        vec![InlineItem::Text(TextRun::new(
            text,
            GlyphStyle::plain(&style),
        ))],
        &style,
        &mut fonts,
        200.0,
        200.0,
    );
    style.align = TextAlign::Start;
    let started = fit(
        vec![InlineItem::Text(TextRun::new(
            text,
            GlyphStyle::plain(&style),
        ))],
        &style,
        &mut fonts,
        200.0,
        200.0,
    );
    let just = glyph_spans(&justified);
    let start = glyph_spans(&started);
    assert!(just.len() >= 2);
    assert!(pt_near(just[0].1, start[0].1));
    assert!(pt_near(just[0].2, start[0].2));
}

#[test]
fn justify_fill_line_stays_start_and_fill_still_eats_width() {
    let Some((mut fonts, mut style)) = fonts_and_style() else {
        return;
    };
    style.align = TextAlign::Justify;
    let fragment = fit(
        vec![
            InlineItem::Text(TextRun::new("好", GlyphStyle::plain(&style))),
            InlineItem::Blank(InlineBlank::new(BlankWidth::ToEndOfLine)),
        ],
        &style,
        &mut fonts,
        120.0,
        80.0,
    );
    let glyphs = glyph_spans(&fragment);
    assert!(pt_near(glyphs[0].1, 0.0));
    let end = fragment.commands.iter().find_map(|command| match command {
        FragmentCommand::Stroke { path, .. } => path.verbs().last().map(|verb| match verb {
            PathVerb::LineTo(point) => point.x().as_pt(),
            PathVerb::MoveTo(_) | PathVerb::CubicTo { .. } | PathVerb::Close => f32::NAN,
        }),
        FragmentCommand::Glyphs(_)
        | FragmentCommand::Fill { .. }
        | FragmentCommand::Image { .. }
        | FragmentCommand::Link { .. }
        | FragmentCommand::PushTransform { .. }
        | FragmentCommand::PopTransform => None,
    });
    assert!(pt_near(end.unwrap_or(f32::NAN), 120.0));
}

#[test]
fn justify_space_cap_is_half_em() {
    let Some((mut fonts, mut style)) = fonts_and_style() else {
        return;
    };
    // Two short Latin words with one space; wide frame → large remainder; only
    // phase-1 slots. Cap is 0.5 * 12 = 6 pt.
    style.align = TextAlign::Justify;
    let text = "Hi there";
    let justified = fit(
        vec![InlineItem::Text(TextRun::new(
            text,
            GlyphStyle::plain(&style),
        ))],
        &style,
        &mut fonts,
        200.0,
        80.0,
    );
    style.align = TextAlign::Start;
    let started = fit(
        vec![InlineItem::Text(TextRun::new(
            text,
            GlyphStyle::plain(&style),
        ))],
        &style,
        &mut fonts,
        200.0,
        80.0,
    );
    // Single-line paragraph is last line → Start fallback; force two lines.
    let long = "aa bb cc dd ee ff gg hh ii jj kk";
    style.align = TextAlign::Justify;
    style.font_size = Positive::pt(12.0).unwrap();
    let wrapped = fit(
        vec![InlineItem::Text(TextRun::new(
            long,
            GlyphStyle::plain(&style),
        ))],
        &style,
        &mut fonts,
        80.0,
        200.0,
    );
    style.align = TextAlign::Start;
    let wrapped_start = fit(
        vec![InlineItem::Text(TextRun::new(
            long,
            GlyphStyle::plain(&style),
        ))],
        &style,
        &mut fonts,
        80.0,
        200.0,
    );
    let just_lines = glyph_spans(&wrapped);
    let start_lines = glyph_spans(&wrapped_start);
    assert!(just_lines.len() >= 2, "need wrap for non-last stretch");
    let mid = 0usize;
    let growth = (just_lines[mid].2) - (start_lines[mid].2);
    let space_count = start_lines[mid]
        .0
        .chars()
        .filter(|ch| *ch == ' ')
        .count()
        .max(1);
    #[expect(
        clippy::cast_precision_loss,
        reason = "space count on one line stays within f32 exact integer range"
    )]
    let cap_total = 0.5 * style.font_size.as_pt() * space_count as f32;
    assert!(
        growth <= cap_total + 0.05,
        "growth {growth} exceeds space caps {cap_total}"
    );
    // Single-line control stays Start (last line).
    assert!(pt_near(
        glyph_spans(&justified)[0].2,
        glyph_spans(&started)[0].2
    ));
}

#[test]
fn justify_cjk_latin_boundary_stretches() {
    let Some((mut fonts, mut style)) = fonts_and_style() else {
        return;
    };
    style.align = TextAlign::Justify;
    // Wraps on ideograph breaks; CJK–Latin gaps are phase-2 slots.
    let mixed = "中A中B中C中D中E中F中G中H";
    let mixed_just = fit(
        vec![InlineItem::Text(TextRun::new(
            mixed,
            GlyphStyle::plain(&style),
        ))],
        &style,
        &mut fonts,
        72.0,
        200.0,
    );
    style.align = TextAlign::Start;
    let mixed_start = fit(
        vec![InlineItem::Text(TextRun::new(
            mixed,
            GlyphStyle::plain(&style),
        ))],
        &style,
        &mut fonts,
        72.0,
        200.0,
    );
    let mj = glyph_spans(&mixed_just);
    let ms = glyph_spans(&mixed_start);
    assert!(mj.len() >= 2);
    assert!(mj[0].2 > ms[0].2 + 0.5);
}

#[test]
fn justify_link_rect_follows_stretched_glyphs() {
    let Some((mut fonts, mut style)) = fonts_and_style() else {
        return;
    };
    let text = "两端对齐链接覆盖拉开后的整段文字内容。";
    style.align = TextAlign::Justify;
    let face = style.font.expect("test style has a face");
    let justified = fit(
        vec![text_item(text, &style, face, Some("https://example.com"))],
        &style,
        &mut fonts,
        100.0,
        200.0,
    );
    style.align = TextAlign::Start;
    let started = fit(
        vec![text_item(text, &style, face, Some("https://example.com"))],
        &style,
        &mut fonts,
        100.0,
        200.0,
    );
    let just_links = hits(&justified);
    let start_links = hits(&started);
    assert!(!just_links.is_empty());
    assert_eq!(just_links.len(), start_links.len());
    if just_links.len() >= 2 {
        assert!(just_links[0].width > start_links[0].width + 0.5);
    }
}

#[test]
fn justify_with_tracking_stretches_after_tracking_width() {
    let Some((mut fonts, style)) = fonts_and_style() else {
        return;
    };
    let face = style.font.expect("test style has a face");
    let tracking = Finite::pt(1.0).unwrap();
    let text = "你好世界你好世界你好世界";
    // 100pt leaves remainder after tracking-inflated cluster widths.
    let mut place = |align: TextAlign| {
        let mut local = style.clone();
        local.align = align;
        fit(
            vec![InlineItem::Text(TextRun::new(
                text,
                GlyphStyle {
                    font: face,
                    tracking,
                    ..GlyphStyle::plain(&local)
                },
            ))],
            &local,
            &mut fonts,
            100.0,
            200.0,
        )
    };
    let justified = place(TextAlign::Justify);
    let started = place(TextAlign::Start);
    let mj = glyph_spans(&justified);
    let ms = glyph_spans(&started);
    assert!(mj.len() >= 2);
    // Start twin already includes tracking in line width; justify adds on top.
    assert!(mj[0].2 > ms[0].2 + 0.5);
}

#[test]
fn orphans_and_widows_r10_tests() {
    let Some((mut fonts, style)) = fonts_and_style() else {
        return;
    };
    let text = "Line 1\nLine 2\nLine 3\nLine 4\nLine 5";
    let node = ParagraphNode::layout(&mut fonts, text, &style, 200.0).unwrap();
    assert_eq!(node.lines.len(), 5);
    let lh = node.lines[0].height.as_pt();
    let full_h = Positive::pt(lh * 10.0).unwrap();

    // 1. Default: 5-line paragraph with 1 line space at page bottom pushes whole paragraph to next page.
    {
        let frame = Frame {
            width: Positive::pt(200.0).unwrap(),
            height: NonNegative::pt(lh * 1.5).unwrap(),
            full_height: full_h,
        };
        let split = node.clone().split(frame).unwrap();
        assert!(matches!(split, Split::MoveToNextPage(_)));
    }

    // 2. 5-line paragraph with 4 lines space: puts 3 lines on current page, 2 on next page.
    {
        let frame = Frame {
            width: Positive::pt(200.0).unwrap(),
            height: NonNegative::pt(lh * 4.5).unwrap(),
            full_height: full_h,
        };
        let split = node.split(frame).unwrap();
        match split {
            Split::Partial { head, tail, .. } => {
                assert_eq!(tail.lines.len(), 2);
                assert!((head.height.as_pt() - lh * 3.0).abs() < 1.0);
            }
            Split::Fit { .. } | Split::MoveToNextPage(_) => {
                panic!("expected Partial, got {split:?}")
            }
        }
    }

    // 3. orphans = 1, widows = 1: behaves identically to old split.
    {
        let mut old_style = style.clone();
        old_style.orphans = NonZeroU8::new(1).unwrap();
        old_style.widows = NonZeroU8::new(1).unwrap();
        let node_old = ParagraphNode::layout(&mut fonts, text, &old_style, 200.0).unwrap();
        let frame = Frame {
            width: Positive::pt(200.0).unwrap(),
            height: NonNegative::pt(lh * 4.5).unwrap(),
            full_height: full_h,
        };
        let split = node_old.split(frame).unwrap();
        match split {
            Split::Partial { head, tail, .. } => {
                assert_eq!(tail.lines.len(), 1);
                assert!((head.height.as_pt() - lh * 4.0).abs() < 1.0);
            }
            Split::Fit { .. } | Split::MoveToNextPage(_) => {
                panic!("expected Partial with 4 lines in head, got {split:?}")
            }
        }
    }
}

#[test]
fn keep_with_next_title_r10_tests() {
    let Some((mut fonts, style)) = fonts_and_style() else {
        return;
    };
    let m = crate::layout::test_util::metrics(&fonts, &style);
    let lh = m.line_height.as_pt();

    // 4. Title paragraph keep_with_next = true: following paragraph cannot fit first 2 lines -> title pushed to next page.
    let page_h = 200.0;
    let space_left = lh * 2.0;
    let spacer_h = page_h - space_left;

    let mut title_style = style.clone();
    title_style.keep_with_next = true;
    let title_node = TextNode::new("Title", GlyphStyle::plain(&title_style), title_style);

    let body_text = "Line 1\nLine 2\nLine 3\nLine 4\nLine 5";
    let body_node = TextNode::new(body_text, GlyphStyle::plain(&style), style);

    let spacer = FlowNode::Spacer(SpacerNode {
        height: NonNegative::pt(spacer_h).unwrap(),
    });
    let root = FlowNode::Stack(vec![
        spacer,
        FlowNode::Text(title_node),
        FlowNode::Text(body_node),
    ]);

    let (result, rec) = render(root, &mut fonts, page(200.0, page_h), &Decorations::none());
    result.unwrap();
    assert_eq!(rec.pages().len(), 2);
    let runs0 = crate::layout::test_util::runs(&rec.pages()[0]);
    let runs1 = crate::layout::test_util::runs(&rec.pages()[1]);
    assert!(runs0.is_empty(), "Page 1 must not contain Title");
    assert!(!runs1.is_empty(), "Page 2 must contain Title and body");
    assert_eq!(runs1[0].0, "Title");
}

#[test]
fn keep_with_next_chaining_r10_tests() {
    use crate::layout::column_width::Ratio;
    use crate::layout::table::{Cell, Rules, TableNode, TableSpec};

    let Some((mut fonts, style)) = fonts_and_style() else {
        return;
    };
    let m = crate::layout::test_util::metrics(&fonts, &style);
    let lh = m.line_height.as_pt();

    // 5. 3 keep_with_next paragraphs chaining into table: chain and table header+first row stay on same page.
    {
        let page_h = 300.0;
        let space_left = lh * 4.0;
        let spacer_h = page_h - space_left;

        let mut kwn_style = style.clone();
        kwn_style.keep_with_next = true;

        let p1 = TextNode::new("Title 1", GlyphStyle::plain(&kwn_style), kwn_style.clone());
        let p2 = TextNode::new("Title 2", GlyphStyle::plain(&kwn_style), kwn_style.clone());
        let p3 = TextNode::new("Title 3", GlyphStyle::plain(&kwn_style), kwn_style.clone());

        let mut spec = TableSpec::new(vec![crate::layout::column_width::ColumnWidth::Ratio(
            Ratio::new(1.0).unwrap(),
        )]);
        spec.rules = Rules::None;
        spec.header = vec![vec![Cell::new(vec![FlowNode::Text(TextNode::new(
            "TH",
            GlyphStyle::plain(&style),
            style.clone(),
        ))])
        .unwrap()]];
        spec.body = vec![
            vec![Cell::new(vec![FlowNode::Text(TextNode::new(
                "TR1",
                GlyphStyle::plain(&style),
                style.clone(),
            ))])
            .unwrap()],
            vec![Cell::new(vec![FlowNode::Text(TextNode::new(
                "TR2",
                GlyphStyle::plain(&style),
                style.clone(),
            ))])
            .unwrap()],
        ];
        let table = TableNode::new(spec).unwrap();

        let spacer = FlowNode::Spacer(SpacerNode {
            height: NonNegative::pt(spacer_h).unwrap(),
        });
        let root = FlowNode::Stack(vec![
            spacer,
            FlowNode::Text(p1),
            FlowNode::Text(p2),
            FlowNode::Text(p3),
            FlowNode::Table(table),
        ]);

        let (result, rec) = render(root, &mut fonts, page(200.0, page_h), &Decorations::none());
        result.unwrap();
        assert_eq!(rec.pages().len(), 2);
        let runs0 = crate::layout::test_util::runs(&rec.pages()[0]);
        let runs1 = crate::layout::test_util::runs(&rec.pages()[1]);
        assert!(runs0.is_empty(), "Page 1 must not contain the chain");
        let texts1: Vec<&str> = runs1.iter().map(|r| r.0.as_str()).collect();
        assert!(texts1.contains(&"Title 1"));
        assert!(texts1.contains(&"Title 2"));
        assert!(texts1.contains(&"Title 3"));
        assert!(texts1.contains(&"TH"));
        assert!(texts1.contains(&"TR1"));
    }

    // 6. Chain taller than entire page: lays out normally, no error, no infinite loop.
    {
        let page_h = 100.0;
        let mut kwn_style = style;
        kwn_style.keep_with_next = true;

        let mut nodes = Vec::new();
        for i in 0..10 {
            nodes.push(FlowNode::Text(TextNode::new(
                format!("Heading {i}"),
                GlyphStyle::plain(&kwn_style),
                kwn_style.clone(),
            )));
        }
        let root = FlowNode::Stack(nodes);
        let (result, rec) = render(root, &mut fonts, page(200.0, page_h), &Decorations::none());
        result.unwrap();
        assert!(rec.pages().len() >= 2);
    }
}
