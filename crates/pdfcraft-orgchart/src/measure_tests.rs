//! Wrap, box height, and the HC `ExtraLight` face.

use crate::captions::CueSide;
use crate::display::{company_fill, LineKind, LogicalLine};
use crate::ids::NodeId;
use crate::measure::{
    build_node_boxes, cont_cue_height_mm, default_node_width_mm, overflow_box,
    promote_to_page_subject, sized_cue_anchors, wrap_text, FontMetrics, InboxNode, Measurer,
    NodeBox, WrappedLine,
};
use crate::{ErrorCode, FontBytes, Node, NodeType};

fn chars_f64(text: &str) -> f64 {
    f64::from(u32::try_from(text.chars().count()).unwrap_or(u32::MAX))
}

struct FakeMeasurer {
    char_w: f64,
    line_h: f64,
}

impl FakeMeasurer {
    fn new(char_w: f64, line_h: f64) -> Self {
        Self { char_w, line_h }
    }
}

impl Measurer for FakeMeasurer {
    fn string_width_mm(&mut self, text: &str, _size_pt: f64) -> Result<f64, crate::Error> {
        Ok(chars_f64(text) * self.char_w)
    }

    fn line_height_mm(&mut self, _size_pt: f64) -> f64 {
        self.line_h
    }

    fn ensure_glyphs(&mut self, _text: &str) -> Result<(), crate::Error> {
        Ok(())
    }
}

struct DigitMeasurer;

impl Measurer for DigitMeasurer {
    fn string_width_mm(&mut self, text: &str, _size_pt: f64) -> Result<f64, crate::Error> {
        Ok(if text == "8" {
            10.0
        } else if text.chars().count() == 1 && text.chars().all(|ch| ch.is_ascii_digit()) {
            1.0
        } else {
            chars_f64(text)
        })
    }

    fn line_height_mm(&mut self, _size_pt: f64) -> f64 {
        5.0
    }

    fn ensure_glyphs(&mut self, _text: &str) -> Result<(), crate::Error> {
        Ok(())
    }
}

fn line(kind: LineKind, text: &str) -> LogicalLine {
    LogicalLine {
        kind,
        text: text.to_owned(),
    }
}

fn inbox<'a>(
    index: usize,
    protocol: &'a str,
    lines: &'a [LogicalLine],
    node: &Node,
) -> InboxNode<'a> {
    InboxNode {
        id: NodeId::from_index(index),
        protocol_id: protocol,
        lines,
        is_company: company_fill(node),
    }
}

fn name_only(id: &str, name: &str) -> Node {
    Node::new(id, name)
}

#[test]
fn cjk_wraps_char_by_char() {
    let mut measurer = FakeMeasurer::new(2.0, 5.0);
    let parts = wrap_text("甲乙丙丁戊己", &mut measurer, 10.0, 6.0, true).expect("wrap");
    assert_eq!(parts, ["甲乙丙", "丁戊己"]);
}

#[test]
fn ascii_keeps_words_intact() {
    let mut measurer = FakeMeasurer::new(2.0, 5.0);
    let parts = wrap_text("ab cd ef", &mut measurer, 10.0, 6.0, true).expect("wrap");
    assert_eq!(parts, ["ab ", "cd ", "ef"]);
}

#[test]
fn overlong_ascii_token_is_cut_by_code_point() {
    let mut measurer = FakeMeasurer::new(2.0, 5.0);
    let text = "abcdefgh";
    let parts = wrap_text(text, &mut measurer, 10.0, 6.0, true).expect("wrap");
    assert_eq!(parts.concat(), text);
    assert!(parts.len() > 1);
    for part in &parts {
        let width = measurer.string_width_mm(part, 10.0).expect("width");
        assert!(width <= 6.0);
    }
}

#[test]
fn ratio_text_stays_one_line_when_wrap_is_off() {
    let mut measurer = FakeMeasurer::new(2.0, 5.0);
    let long_ratio = "12.50%".repeat(10);
    let width = measurer.string_width_mm(&long_ratio, 9.0).expect("width");
    assert!(width > 38.0);
    let parts = wrap_text(&long_ratio, &mut measurer, 9.0, 38.0, false).expect("wrap");
    assert_eq!(parts, [long_ratio]);
}

#[test]
fn detail_line_wraps_when_wider_than_the_box() {
    let mut measurer = FakeMeasurer::new(2.0, 5.0);
    let long_detail = "甲".repeat(20);
    let node = name_only("n1", "甲");
    let lines = [
        line(LineKind::Name, "甲"),
        line(LineKind::Detail, &long_detail),
    ];
    let boxes = build_node_boxes(
        &[inbox(0, "n1", &lines, &node)],
        &mut measurer,
        10.0,
        "other",
        257.0,
    )
    .expect("boxes");
    let joined: String = boxes[0].lines.iter().map(|row| row.text.as_str()).collect();
    assert_eq!(joined, format!("甲{long_detail}"));
    assert!(boxes[0].lines.len() > 2);
}

#[test]
fn subject_lines_keep_their_font_sizes() {
    let mut measurer = FakeMeasurer::new(2.0, 5.0);
    let node = Node {
        legal_person: Some("張三".to_owned()),
        ..name_only("n1", "公司")
    };
    let lines = [
        line(LineKind::Name, "公司"),
        line(LineKind::LegalPerson, "張三"),
        line(LineKind::Detail, "51.20%"),
    ];
    let boxes = build_node_boxes(
        &[inbox(0, "n1", &lines, &node)],
        &mut measurer,
        default_node_width_mm(),
        "n1",
        257.0,
    )
    .expect("boxes");
    let box_ = &boxes[0];
    assert_eq!(
        box_.lines
            .iter()
            .map(|row| (row.kind, row.font_size_pt))
            .collect::<Vec<_>>(),
        vec![
            (LineKind::Name, 12.0),
            (LineKind::LegalPerson, 9.0),
            (LineKind::Detail, 9.0),
        ]
    );
    assert!((box_.height_mm - (2.0 * 3.5 + 5.0 * 3.0)).abs() < 0.0001);
    assert!((box_.width_mm - 80.0).abs() < 0.0001);
    assert!(box_.is_subject);
}

#[test]
fn subject_box_is_larger_than_an_ordinary_peer() {
    let mut measurer = FakeMeasurer::new(2.0, 5.0);
    let root = Node {
        node_type: Some(NodeType::Company),
        ..name_only("root", "主体")
    };
    let holder = Node {
        node_type: Some(NodeType::Person),
        ..name_only("s1", "股东")
    };
    let root_lines = [line(LineKind::Name, "主体")];
    let holder_lines = [line(LineKind::Name, "股东")];
    let boxes = build_node_boxes(
        &[
            inbox(0, "root", &root_lines, &root),
            inbox(1, "s1", &holder_lines, &holder),
        ],
        &mut measurer,
        default_node_width_mm(),
        "root",
        257.0,
    )
    .expect("boxes");
    assert!(boxes[0].width_mm > boxes[1].width_mm);
    assert!(boxes[0].height_mm > boxes[1].height_mm);
    assert!((boxes[1].width_mm - 38.0).abs() < 0.0001);
    assert!((boxes[1].height_mm - (2.0 * 1.5 + 2.0 * 5.0)).abs() < 0.0001);
    assert!(!boxes[1].is_company);
    assert!(boxes[0].is_company);
}

#[test]
fn promote_keeps_detail_and_widens() {
    let mut measurer = FakeMeasurer::new(2.0, 5.0);
    let ordinary = NodeBox {
        id: NodeId::from_index(0),
        protocol_id: "cf3".to_owned(),
        is_overflow: false,
        width_mm: 38.0,
        height_mm: 2.0 * 1.5 + 10.0 + 5.0,
        lines: vec![
            WrappedLine {
                kind: LineKind::Name,
                text: "超频三".to_owned(),
                font_size_pt: 10.0,
            },
            WrappedLine {
                kind: LineKind::Detail,
                text: "实缴 100万".to_owned(),
                font_size_pt: 9.0,
            },
        ],
        is_subject: false,
        is_company: true,
    };
    let promoted = promote_to_page_subject(&ordinary, 38.0, &mut measurer).expect("promote");
    assert!(promoted.is_subject);
    assert!((promoted.width_mm - 80.0).abs() < 0.0001);
    assert_eq!(
        promoted
            .lines
            .iter()
            .map(|row| row.text.as_str())
            .collect::<Vec<_>>(),
        ["超频三", "实缴 100万"]
    );
    assert_eq!(
        promoted
            .lines
            .iter()
            .map(|row| (row.kind, row.font_size_pt))
            .collect::<Vec<_>>(),
        vec![(LineKind::Name, 12.0), (LineKind::Detail, 9.0)]
    );
    assert!((promoted.height_mm - (2.0 * 3.5 + 2.0 * 5.0)).abs() < 0.0001);
}

#[test]
fn promote_rejoins_a_wrapped_name() {
    let mut measurer = FakeMeasurer::new(2.0, 5.0);
    let long_name = "甲".repeat(24);
    let parts = wrap_text(&long_name, &mut measurer, 10.0, 38.0 - 4.0, true).expect("wrap");
    assert!(parts.len() > 1);
    let ordinary = NodeBox {
        id: NodeId::from_index(0),
        protocol_id: "long".to_owned(),
        is_overflow: false,
        width_mm: 38.0,
        height_mm: 2.0 * 1.5 + 5.0 * f64::from(u32::try_from(parts.len()).unwrap_or(u32::MAX)),
        lines: parts
            .iter()
            .map(|part| WrappedLine {
                kind: LineKind::Name,
                text: part.clone(),
                font_size_pt: 10.0,
            })
            .collect(),
        is_subject: false,
        is_company: true,
    };
    let promoted = promote_to_page_subject(&ordinary, 38.0, &mut measurer).expect("promote");
    assert!(promoted.lines.len() < parts.len());
    let joined: String = promoted.lines.iter().map(|row| row.text.as_str()).collect();
    assert_eq!(joined, long_name);
    let content = promoted.width_mm - 4.0;
    for row in &promoted.lines {
        assert_eq!(row.kind, LineKind::Name);
        let width = measurer.string_width_mm(&row.text, 12.0).expect("width");
        assert!(width <= content);
    }
}

#[test]
fn blank_legal_person_does_not_force_company_shade() {
    let mut measurer = FakeMeasurer::new(2.0, 5.0);
    let blank = Node {
        legal_person: Some("   ".to_owned()),
        ..name_only("blank", "空法人")
    };
    let company = Node {
        node_type: Some(NodeType::Company),
        ..name_only("company", "公司型")
    };
    let person = Node {
        node_type: Some(NodeType::Person),
        ..name_only("person", "自然人")
    };
    let person_lp = Node {
        node_type: Some(NodeType::Person),
        legal_person: Some("張三".to_owned()),
        ..name_only("person_lp", "自然人带法人")
    };
    let inferred = Node {
        legal_person: Some("李四".to_owned()),
        ..name_only("inferred", "推断公司")
    };
    let names = ["空法人", "公司型", "自然人", "自然人带法人", "推断公司"];
    let nodes = [&blank, &company, &person, &person_lp, &inferred];
    let lines: Vec<LogicalLine> = names
        .iter()
        .map(|name| line(LineKind::Name, name))
        .collect();
    let inbox: Vec<InboxNode<'_>> = nodes
        .iter()
        .enumerate()
        .map(|(index, node)| {
            inbox(
                index,
                node.id.as_str(),
                std::slice::from_ref(&lines[index]),
                node,
            )
        })
        .collect();
    let boxes = build_node_boxes(&inbox, &mut measurer, 38.0, "blank", 257.0).expect("boxes");
    let shade = |id: &str| {
        boxes
            .iter()
            .find(|box_| box_.protocol_id == id)
            .expect(id)
            .is_company
    };
    assert!(!shade("blank"));
    assert!(shade("company"));
    assert!(!shade("person"));
    assert!(!shade("person_lp"));
    assert!(shade("inferred"));
}

#[test]
fn a_wrapped_name_taller_than_the_portrait_limit_is_node_too_tall() {
    let mut measurer = FakeMeasurer::new(2.0, 5.0);
    let printable_h = 50.0;
    let max_h = (printable_h - 18.0) / 2.0;
    assert!(max_h < printable_h / 2.0);
    let node = name_only("n1", "x");
    let lines = [
        line(LineKind::Name, "甲乙丙丁戊己甲乙丙丁"),
        line(LineKind::Name, "甲乙丙丁戊己甲乙丙丁"),
    ];
    let error = build_node_boxes(
        &[inbox(0, "n1", &lines, &node)],
        &mut measurer,
        10.0,
        "n1",
        printable_h,
    )
    .expect_err("too tall");
    assert_eq!(error.code, ErrorCode::NodeTooTall);
    assert!(error.message.contains("max_node_height"));
}

#[test]
fn cue_slots_budget_three_widest_digits() {
    let boxes = vec![
        NodeBox {
            id: NodeId::from_index(0),
            protocol_id: "s1".to_owned(),
            is_overflow: false,
            width_mm: 10.0,
            height_mm: 12.0,
            lines: vec![WrappedLine {
                kind: LineKind::Name,
                text: "甲".to_owned(),
                font_size_pt: 10.0,
            }],
            is_subject: false,
            is_company: true,
        },
        NodeBox {
            id: NodeId::from_index(1),
            protocol_id: "root".to_owned(),
            is_overflow: false,
            width_mm: 40.0,
            height_mm: 16.0,
            lines: vec![WrappedLine {
                kind: LineKind::Name,
                text: "主体".to_owned(),
                font_size_pt: 12.0,
            }],
            is_subject: true,
            is_company: true,
        },
    ];
    let sized = sized_cue_anchors(
        &[("s1", CueSide::Shareholder), ("root", CueSide::Investment)],
        &boxes,
        &mut DigitMeasurer,
    )
    .expect("cues");
    assert_eq!(sized[0].anchor_id, "s1");
    assert_eq!(sized[0].side, CueSide::Shareholder);
    assert_eq!(sized[1].side, CueSide::Investment);
    assert!((sized[0].width_mm - 36.0).abs() < 0.0001);
    assert!((sized[1].width_mm - 40.0).abs() < 0.0001);
    assert!((sized[0].height_mm - 14.4).abs() < 0.0001);
    assert!((cont_cue_height_mm(&mut DigitMeasurer) - 14.4).abs() < 0.0001);
}

#[test]
fn a_missing_anchor_and_an_overflow_id_are_layout_failed() {
    let boxes = [NodeBox {
        id: NodeId::from_index(0),
        protocol_id: "s1".to_owned(),
        is_overflow: false,
        width_mm: 10.0,
        height_mm: 12.0,
        lines: vec![],
        is_subject: false,
        is_company: true,
    }];
    let missing = sized_cue_anchors(
        &[("gone", CueSide::Shareholder)],
        &boxes,
        &mut DigitMeasurer,
    )
    .expect_err("missing");
    assert_eq!(missing.code, ErrorCode::LayoutFailed);
    let overflow = sized_cue_anchors(
        &[("__overflow__:root:s2", CueSide::Shareholder)],
        &boxes,
        &mut DigitMeasurer,
    )
    .expect_err("overflow");
    assert_eq!(overflow.code, ErrorCode::LayoutFailed);
}

#[test]
fn overflow_box_reserves_a_blank_second_name_row() {
    let mut measurer = FakeMeasurer::new(2.0, 5.0);
    let box_ = overflow_box(
        NodeId::from_index(9),
        "__overflow__:root:s",
        "其他股东(2家)",
        &mut measurer,
        38.0,
    )
    .expect("box");
    assert!(box_.lines.len() >= 2);
    assert_eq!(box_.lines.last().expect("slot").text, "");
    assert!((box_.lines.last().expect("slot").font_size_pt - 10.0).abs() < 0.0001);
    assert!(box_.is_company);
    assert!(!box_.is_subject);
}

fn hc_font() -> Option<crate::FontBytes> {
    crate::test_font::env_font()
}

#[test]
fn hc_line_height_matches_hhea_plus_gap() {
    let Some(bytes) = hc_font() else {
        return;
    };
    let font = FontMetrics::open(&bytes).expect("open");
    let line_h = font.line_height_mm(10.0);
    assert!((4.0..=8.0).contains(&line_h));
    assert!(font.ascent_mm(10.0) < line_h);
}

#[test]
fn hebrew_is_missing_from_the_hc_face() {
    let Some(bytes) = hc_font() else {
        return;
    };
    let mut font = FontMetrics::open(&bytes).expect("open");
    let width = font.string_width_mm("אבג", 10.0).expect_err("width");
    assert_eq!(width.code, ErrorCode::MissingGlyph);
    let ensured = font.ensure_glyphs("אבג").expect_err("ensure");
    assert_eq!(ensured.code, ErrorCode::MissingGlyph);
    assert!(ensured.message.contains("missing glyph"));
}

#[test]
fn a_collection_and_garbage_are_font_error() {
    let ttc = FontMetrics::open(&FontBytes::new(b"ttcf".to_vec())).expect_err("ttc");
    assert_eq!(ttc.code, ErrorCode::FontError);
    assert!(ttc.message.contains("TTC"));
    let garbage = FontMetrics::open(&FontBytes::new(b"not a font".to_vec())).expect_err("bad");
    assert_eq!(garbage.code, ErrorCode::FontError);
    let empty = FontMetrics::open(&FontBytes::new(Vec::<u8>::new())).expect_err("empty");
    assert_eq!(empty.code, ErrorCode::FontError);
}

#[test]
fn hc_latin_glyph_run_is_stable_and_has_width() {
    let Some(bytes) = hc_font() else {
        return;
    };
    let mut font = FontMetrics::open(&bytes).expect("open");
    let first = font.glyph_run("A").expect("glyphs");
    let second = font.glyph_run("A").expect("glyphs");
    assert_eq!(first, second);
    assert_eq!(first.len(), 1);
    assert!(first[0].glyph_id > 0);
    let width = font.string_width_mm("A", 10.0).expect("width");
    assert!(width > 0.0);
}
