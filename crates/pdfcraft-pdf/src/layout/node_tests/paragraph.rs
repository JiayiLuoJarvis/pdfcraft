use crate::canvas::{pt_near, CanvasCommand, LinkTarget, RecordingCanvas};
use crate::fonts::{fixture_bytes, EmOffset, FontId, FontSource, FontStore};
use crate::geom::{Finite, NonNegative, PathVerb, Point, Positive};
use crate::layout::decorate::Decorations;
use crate::layout::flow::FlowNode;
use crate::layout::frame::{draw_fragment, Fragment, FragmentCommand, Frame, LayoutCtx, Split};
use crate::layout::paragraph::{BlankWidth, InlineBlank, InlineItem, TextNode, TextRun};
use crate::layout::style::{Decoration, GlyphStyle, Style, TextAlign};
use crate::layout::test_util::{fonts_and_style, page, render};
use crate::text::measure::{FaceExtent, LineMetrics};

fn uri(text: &str) -> LinkTarget {
    LinkTarget::uri(text).unwrap()
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
            FragmentCommand::Link { rect, target } => Some(Hit {
                x: rect.origin().x().as_pt(),
                y: rect.origin().y().as_pt(),
                width: rect.size().width().as_pt(),
                height: rect.size().height().as_pt(),
                target: target.as_str().to_owned(),
            }),
            _ => None,
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
            _ => None,
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
        _ => None,
    });
    assert_eq!(glyph.map(f32::to_bits), Some(0.0f32.to_bits()));
    let end = fragment.commands.iter().find_map(|command| match command {
        FragmentCommand::Stroke { path, .. } => path.verbs().last().map(|verb| match verb {
            crate::geom::PathVerb::LineTo(point) => point.x().as_pt(),
            _ => f32::NAN,
        }),
        _ => None,
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
            CanvasCommand::AnnotateLink { rect, target } => Some((
                rect.origin().x().as_pt(),
                rect.origin().y().as_pt(),
                target.as_str().to_owned(),
            )),
            _ => None,
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
            _ => {}
        }
    }
    assert_eq!(drawn.len(), 1, "{drawn:?}");
    let baseline = baseline.expect("glyphs");
    // -(offset/upem)*size + 0.5 at 12 pt. Offsets are already in em.
    assert!(pt_near(drawn[0].0, baseline + 3.5));
    assert!(!pt_near(drawn[0].0, baseline + 10.1));
    assert!(pt_near(drawn[0].1, 1.0));
}
