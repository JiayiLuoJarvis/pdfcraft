//! `layout_spec` against recorded canvas commands.

use std::num::{NonZeroU16, NonZeroU32};

use pdfcraft_pdf::canvas::pt_near;
#[cfg(feature = "backend-krilla")]
use pdfcraft_pdf::spec::ImageSpec;
use pdfcraft_pdf::spec::{
    Alignment, BandInset, BandSlot, BandSpec, BlankSpec, BlankWidth, Block, CanvasMark, CanvasSpec,
    CellBlock, CellSpec, ColumnSpec, ColumnsSpec, FontFace, FontFamily, FontRegistry, HLineSpec,
    InlineRun, LayerMark, LayerShape, ListItemSpec, ListKind, ListSpec, PageWhen, ParagraphSpec,
    Resources, RunStyle, SectionSpec, TableSpec, TextRun,
};
use pdfcraft_pdf::{
    layout_spec, CanvasCommand, CellSpanReason, Color, ColumnWidthsReason, DocumentSpec, Error,
    ErrorKind, GeometryKind, ImageFormat, Ink, Length, LinkTarget, MarginPreset, Margins,
    NonNegative, Numbering, PageSize, Path, PathVerb, Pen, Point, Positive, Ratio, RecordedPage,
    Rect, Region, Rendered, Result, Size, Stroke, Track, A4,
};
#[cfg(feature = "backend-krilla")]
use pdfcraft_pdf::{to_pdf, Paper};

#[path = "common/mod.rs"]
mod common;

const ONE_PIXEL: &[u8] = include_bytes!("fixtures/one-pixel.png");
#[cfg(feature = "backend-krilla")]
const BAD_IDAT: &[u8] = include_bytes!("fixtures/bad-idat.png");

fn skip_without_font() -> bool {
    common::test_font_bytes().is_none()
}

fn font_bytes() -> Vec<u8> {
    common::test_font_bytes().unwrap_or_else(|| {
        panic!("set PDFCRAFT_PDF_FONT or PDFCRAFT_CJK_FONT to run document layout tests")
    })
}

fn baseline() -> f32 {
    common::body_line_12(&font_bytes()).0
}

fn line() -> f32 {
    common::body_line_12(&font_bytes()).1
}

type Located = (
    Option<usize>,
    Option<u32>,
    Option<Region>,
    Option<(usize, usize)>,
);

fn location(error: &Error) -> Located {
    let at = error.at.unwrap_or_default();
    (
        at.section,
        at.page,
        at.region,
        at.cell.map(|cell| (cell.row, cell.col)),
    )
}

struct GlyphSample {
    text: String,
    font: u32,
    x: f32,
    y: f32,
    width: f32,
}

#[derive(PartialEq)]
enum Kind {
    A,
    B,
}

struct Item {
    name: String,
    amount: String,
}

struct Contract {
    id: String,
    kind: Kind,
    items: Vec<Item>,
    party_a: String,
    party_b: String,
}

fn nn(value: f32) -> Length<NonNegative> {
    NonNegative::pt(value).unwrap()
}

fn pos(value: f32) -> Length<Positive> {
    Positive::pt(value).unwrap()
}

fn render(
    page: PageSize,
    resources: Resources<()>,
    sections: Vec<SectionSpec<()>>,
) -> Result<Rendered> {
    layout_spec(DocumentSpec {
        page_size: page,
        resources,
        limits: pdfcraft_pdf::Limits::default(),
        info: pdfcraft_pdf::spec::DocumentInfo::default(),
        sections,
    })
}

fn font_resources() -> Resources<()> {
    Resources::with_fonts(FontRegistry::single(FontFamily {
        regular: FontFace::new(font_bytes(), 0),
        bold: None,
        italic: None,
        bold_italic: None,
    }))
}

fn bold_resources() -> Resources<()> {
    let bytes = font_bytes();
    Resources::with_fonts(FontRegistry::single(FontFamily {
        regular: FontFace::new(bytes.clone(), 0),
        bold: Some(FontFace::new(bytes, 0)),
        italic: None,
        bold_italic: None,
    }))
}

fn font_pages(page: PageSize, section: SectionSpec<()>) -> Result<Rendered> {
    render(page, font_resources(), vec![section])
}

fn bold_pages(page: PageSize, section: SectionSpec<()>) -> Result<Rendered> {
    render(page, bold_resources(), vec![section])
}

fn sec(body: Vec<Block<()>>) -> SectionSpec<()> {
    SectionSpec {
        body,
        ..SectionSpec::default()
    }
}

fn para(text: impl Into<String>) -> Block<()> {
    Block::paragraph((), text)
}

fn text_run(text: impl Into<String>) -> InlineRun<()> {
    InlineRun::Text {
        text: text.into(),
        font: (),
        style: RunStyle::default(),
    }
}

fn bold_run(text: impl Into<String>) -> InlineRun<()> {
    InlineRun::Text {
        text: text.into(),
        font: (),
        style: RunStyle {
            bold: Some(true),
            ..RunStyle::default()
        },
    }
}

fn link_run(uri: &str, runs: Vec<InlineRun<()>>) -> InlineRun<()> {
    InlineRun::Link {
        target: if uri.is_empty() {
            LinkTarget::Uri(String::new())
        } else {
            LinkTarget::uri(uri).expect("test uri")
        },
        runs,
    }
}

fn blank_run(
    width: BlankWidth,
    thickness: Option<f32>,
    color: Option<Color>,
    offset: Option<f32>,
) -> InlineRun<()> {
    InlineRun::Blank(BlankSpec {
        width,
        thickness: thickness.map(pos),
        color,
        baseline_offset: offset.map(nn),
    })
}

fn runs_para(runs: Vec<InlineRun<()>>) -> Block<()> {
    Block::Paragraph(ParagraphSpec {
        runs,
        ..ParagraphSpec::default()
    })
}

fn paragraph(
    runs: Vec<InlineRun<()>>,
    indent: f32,
    align: Alignment,
    color: Option<Color>,
    size: Option<f32>,
) -> Block<()> {
    Block::Paragraph(ParagraphSpec {
        runs,
        first_line_indent: nn(indent),
        align,
        color,
        font_size: size.map(pos),
        line_height_factor: None,
    })
}

fn styled(size: Option<f32>, color: Option<Color>, children: Vec<Block<()>>) -> Block<()> {
    Block::Styled {
        style: RunStyle {
            font_size: size.map(pos),
            color,
            ..RunStyle::default()
        },
        children,
    }
}

fn gap_columns(gap: f32, columns: Vec<ColumnSpec<()>>) -> Block<()> {
    Block::Columns(ColumnsSpec {
        gap: nn(gap),
        columns,
    })
}

fn ratio_col(weight: f32, blocks: Vec<Block<()>>) -> ColumnSpec<()> {
    ColumnSpec {
        width: Track::Ratio(Ratio::new(weight).unwrap()),
        blocks,
    }
}

fn fixed_col(width: f32, blocks: Vec<Block<()>>) -> ColumnSpec<()> {
    ColumnSpec {
        width: Track::Fixed(pos(width)),
        blocks,
    }
}

fn auto_col(blocks: Vec<Block<()>>) -> ColumnSpec<()> {
    ColumnSpec {
        width: Track::Auto,
        blocks,
    }
}

fn bullet_list(items: Vec<ListItemSpec<()>>) -> Block<()> {
    Block::List(ListSpec {
        items,
        ..ListSpec::default()
    })
}

fn ordered_list(numbering: Numbering, start: u32, items: Vec<ListItemSpec<()>>) -> Block<()> {
    Block::List(ListSpec {
        kind: ListKind::Ordered(numbering),
        start: NonZeroU32::new(start).unwrap(),
        items,
        ..ListSpec::default()
    })
}

fn plain_item(blocks: Vec<Block<()>>) -> ListItemSpec<()> {
    ListItemSpec {
        marker: None,
        blocks,
    }
}

fn marked_item(marker: &str, blocks: Vec<Block<()>>) -> ListItemSpec<()> {
    ListItemSpec {
        marker: Some(marker.into()),
        blocks,
    }
}

fn text_cell(text: impl Into<String>) -> CellSpec<()> {
    CellSpec {
        content: vec![CellBlock::Text {
            text: text.into(),
            font: (),
        }],
        ..CellSpec::default()
    }
}

fn cell(content: Vec<CellBlock<()>>) -> CellSpec<()> {
    CellSpec {
        content,
        ..CellSpec::default()
    }
}

fn span_cell(text: &str, cols: u16) -> CellSpec<()> {
    CellSpec {
        content: vec![CellBlock::Text {
            text: text.into(),
            font: (),
        }],
        col_span: NonZeroU16::new(cols).unwrap(),
        ..CellSpec::default()
    }
}

fn filled_cell(text: &str, fill: Color) -> CellSpec<()> {
    CellSpec {
        content: vec![CellBlock::Text {
            text: text.into(),
            font: (),
        }],
        fill: Some(fill),
        ..CellSpec::default()
    }
}

fn text_row(items: &[&str]) -> Vec<CellSpec<()>> {
    items.iter().copied().map(text_cell).collect()
}

fn grid(header: Vec<Vec<CellSpec<()>>>, body: Vec<Vec<CellSpec<()>>>) -> Block<()> {
    Block::Table(TableSpec {
        header,
        body,
        ..TableSpec::default()
    })
}

fn rule(spec: HLineSpec) -> Block<()> {
    Block::HLine(spec)
}

#[cfg(feature = "backend-krilla")]
fn image_block(
    image: pdfcraft_pdf::spec::ImageRef,
    width: Option<f32>,
    align: Alignment,
) -> Block<()> {
    Block::Image(ImageSpec {
        image,
        width: width.map(pos),
        height: None,
        align,
    })
}

fn left_band(text: &str) -> BandSpec<()> {
    BandSpec {
        left: Some(BandSlot::Text {
            runs: vec![TextRun::Literal(text.into())],
            font: (),
            style: RunStyle::default(),
        }),
        ..BandSpec::default()
    }
}

fn page_label() -> Vec<TextRun> {
    vec![
        TextRun::Literal("第 ".into()),
        TextRun::PageNumber,
        TextRun::Literal(" 页 / 共 ".into()),
        TextRun::PageCount,
        TextRun::Literal(" 页".into()),
    ]
}

fn slot_text(runs: Vec<TextRun>) -> BandSlot<()> {
    BandSlot::Text {
        runs,
        font: (),
        style: RunStyle::default(),
    }
}

fn slot_link(uri: Vec<TextRun>, text: &str) -> BandSlot<()> {
    BandSlot::Link {
        font: (),
        uri,
        text: vec![TextRun::Literal(text.into())],
        style: RunStyle::default(),
    }
}

fn mark_text(when: PageWhen, text: &str, x: f32, y: f32, style: RunStyle) -> LayerMark<()> {
    LayerMark::Text {
        font: (),
        when,
        text: vec![TextRun::Literal(text.into())],
        at: Point::try_pt(x, y).unwrap(),
        style,
    }
}

fn contract(kind: Kind) -> Contract {
    let item = |name: &str, amount: &str| Item {
        name: name.into(),
        amount: amount.into(),
    };
    Contract {
        id: "C-18".into(),
        kind,
        items: vec![item("Pen", "3"), item("Ink", "5")],
        party_a: "Party A".into(),
        party_b: "Party B".into(),
    }
}

fn runs(page: &RecordedPage) -> Vec<GlyphSample> {
    page.ops()
        .iter()
        .filter_map(|op| match op {
            CanvasCommand::DrawGlyphRun { run } => Some(GlyphSample {
                text: run.text.clone(),
                font: run.font.as_u32(),
                x: run.origin.x().as_pt(),
                y: run.origin.y().as_pt(),
                width: run.glyphs.iter().map(|g| g.advance_norm).sum::<f32>()
                    * run.font_size.as_pt(),
            }),
            _ => None,
        })
        .collect()
}

fn texts(page: &RecordedPage) -> Vec<String> {
    runs(page).into_iter().map(|run| run.text).collect()
}

fn first_example(c: &Contract) -> Result<Rendered> {
    let mut body = vec![para(format!("Contract {}", c.id))];
    if c.kind == Kind::A {
        body.push(para("Plan A."));
        let mut rows = Vec::new();
        for item in &c.items {
            rows.push(text_row(&[&item.name, &item.amount]));
        }
        body.push(grid(vec![text_row(&["Item", "Amount"])], rows));
    } else {
        body.push(para("Plan B."));
        body.push(para("Plan B has no table."));
    }
    font_pages(PageSize::A4, sec(body))
}

#[test]
fn kind_a_flow_is_two_paragraphs_then_a_table() {
    if skip_without_font() {
        return;
    }
    let pages = first_example(&contract(Kind::A)).unwrap().pages();
    assert_eq!(pages.len(), 1);
    assert_eq!(
        texts(&pages[0]),
        [
            "Contract C-18",
            "Plan A.",
            "Item",
            "Amount",
            "Pen",
            "3",
            "Ink",
            "5"
        ]
    );
}

#[test]
fn kind_b_flow_has_no_table() {
    if skip_without_font() {
        return;
    }
    let pages = first_example(&contract(Kind::B)).unwrap().pages();
    assert_eq!(
        texts(&pages[0]),
        ["Contract C-18", "Plan B.", "Plan B has no table."]
    );
}

fn full_template(c: &Contract, company: &str) -> Result<Rendered> {
    font_pages(
        PageSize::A4,
        SectionSpec {
            margins: Margins::uniform(nn(36.0)),
            header: Some(BandSpec {
                left: Some(slot_text(vec![TextRun::Literal(company.into())])),
                right: Some(slot_text(page_label())),
                ..BandSpec::default()
            }),
            behind: vec![mark_text(
                PageWhen::All,
                "DRAFT",
                180.0,
                400.0,
                RunStyle::default(),
            )],
            front: vec![mark_text(
                PageWhen::Last,
                "SEAL",
                430.0,
                680.0,
                RunStyle::default(),
            )],
            body: vec![
                para(format!("Contract no. {}", c.id)),
                Block::PageBreak,
                gap_columns(
                    12.0,
                    vec![
                        ratio_col(1.0, vec![para(c.party_a.clone())]),
                        ratio_col(1.0, vec![para(c.party_b.clone())]),
                    ],
                ),
            ],
            ..SectionSpec::default()
        },
    )
}

#[test]
fn header_numbers_every_page_and_layers_stack_behind_header_body_footer_front() {
    if skip_without_font() {
        return;
    }
    let pages = full_template(&contract(Kind::A), "ACME").unwrap().pages();
    assert_eq!(pages.len(), 2);
    assert_eq!(
        texts(&pages[0]),
        ["DRAFT", "ACME", "第 1 页 / 共 2 页", "Contract no. C-18"]
    );
    assert_eq!(
        texts(&pages[1]),
        [
            "DRAFT",
            "ACME",
            "第 2 页 / 共 2 页",
            "Party A",
            "Party B",
            "SEAL"
        ]
    );
}

#[test]
fn header_slots_span_full_page_width_without_inset_and_body_sits_below_the_band() {
    if skip_without_font() {
        return;
    }
    let pages = full_template(&contract(Kind::A), "ACME").unwrap().pages();
    let r = runs(&pages[0]);
    let (left, right, body) = (&r[1], &r[2], &r[3]);
    assert!(pt_near(left.x, 0.0));
    assert!(pt_near(left.y, baseline()));
    assert!(pt_near(right.x + right.width, A4.width().as_pt()));
    assert!(pt_near(right.y, baseline()));
    assert!(pt_near(body.x, 36.0));
    assert!(pt_near(body.y, 36.0 + baseline()));
}

#[test]
fn header_inset_aligns_slots_with_caller_chosen_side_margins() {
    if skip_without_font() {
        return;
    }
    let pages = font_pages(
        PageSize::A4,
        SectionSpec {
            margins: Margins::uniform(nn(36.0)),
            header: Some(BandSpec {
                inset: BandInset::PageMargins,
                left: Some(slot_text(vec![TextRun::Literal("ACME".into())])),
                right: Some(slot_text(vec![TextRun::Literal("page".into())])),
            }),
            body: vec![para("Body")],
            ..SectionSpec::default()
        },
    )
    .unwrap()
    .pages();
    let r = runs(&pages[0]);
    let (left, right, body) = (&r[0], &r[1], &r[2]);
    assert!(pt_near(left.x, 36.0));
    assert!(pt_near(right.x + right.width, A4.width().as_pt() - 36.0));
    assert!(pt_near(body.x, 36.0));
    assert!(pt_near(body.y, 36.0 + baseline()));
}

#[test]
fn columns_split_body_width_with_a_12pt_gap() {
    if skip_without_font() {
        return;
    }
    let pages = full_template(&contract(Kind::A), "ACME").unwrap().pages();
    let r = runs(&pages[1]);
    let col_w = (A4.width().as_pt() - 72.0 - 12.0) / 2.0;
    assert!(pt_near(r[3].x, 36.0));
    assert!(pt_near(r[4].x, 36.0 + col_w + 12.0));
    assert!(pt_near(r[3].y, 36.0 + baseline()));
    assert!(pt_near(r[4].y, 36.0 + baseline()));
}

#[test]
fn text_at_places_the_top_left_of_the_line_box() {
    if skip_without_font() {
        return;
    }
    let pages = full_template(&contract(Kind::A), "ACME").unwrap().pages();
    let draft = &runs(&pages[0])[0];
    assert!(pt_near(draft.x, 180.0));
    assert!(pt_near(draft.y, 400.0 + baseline()));
    let seal = runs(&pages[1]).pop().unwrap();
    assert!(pt_near(seal.x, 430.0));
    assert!(pt_near(seal.y, 680.0 + baseline()));
}

#[test]
fn stack_style_is_inherited_and_text_at_can_override() {
    if skip_without_font() {
        return;
    }
    let pages = font_pages(
        PageSize::A4,
        SectionSpec {
            body: vec![styled(
                Some(18.0),
                Some(Color::rgb(10, 20, 30)),
                vec![
                    para("AA"),
                    paragraph(
                        vec![text_run("BB")],
                        0.0,
                        Alignment::Start,
                        None,
                        Some(10.0),
                    ),
                ],
            )],
            behind: vec![
                mark_text(
                    PageWhen::All,
                    "CC",
                    8.0,
                    8.0,
                    RunStyle {
                        font_size: Some(pos(14.0)),
                        color: Some(Color::rgb(1, 2, 3)),
                        ..RunStyle::default()
                    },
                ),
                mark_text(
                    PageWhen::All,
                    "DD",
                    8.0,
                    28.0,
                    RunStyle {
                        font_size: Some(pos(9.0)),
                        color: Some(Color::rgb(9, 8, 7)),
                        ..RunStyle::default()
                    },
                ),
            ],
            ..SectionSpec::default()
        },
    )
    .unwrap()
    .pages();
    let page = &pages[0];
    assert_eq!(texts(page), ["CC", "DD", "AA", "BB"]);
    let glyph_runs: Vec<_> = page
        .ops()
        .iter()
        .filter_map(|op| match op {
            CanvasCommand::DrawGlyphRun { run } => Some(run),
            _ => None,
        })
        .collect();
    assert_eq!(glyph_runs[0].font_size.as_pt().to_bits(), 14.0f32.to_bits());
    assert_eq!(glyph_runs[0].color, Color::rgb(1, 2, 3));
    assert_eq!(glyph_runs[1].font_size.as_pt().to_bits(), 9.0f32.to_bits());
    assert_eq!(glyph_runs[1].color, Color::rgb(9, 8, 7));
    assert_eq!(glyph_runs[2].font_size.as_pt().to_bits(), 18.0f32.to_bits());
    assert_eq!(glyph_runs[2].color, Color::rgb(10, 20, 30));
    assert_eq!(glyph_runs[3].font_size.as_pt().to_bits(), 10.0f32.to_bits());
    assert_eq!(glyph_runs[3].color, Color::rgb(10, 20, 30));
}

#[test]
fn table_cell_fill_is_painted() {
    if skip_without_font() {
        return;
    }
    let pages = font_pages(
        PageSize::A4,
        sec(vec![Block::Table(TableSpec {
            header: vec![vec![filled_cell("H", Color::rgb(0x19, 0x2d, 0x74))]],
            body: vec![text_row(&["b"])],
            border_width: nn(0.5),
            ..TableSpec::default()
        })]),
    )
    .unwrap()
    .pages();
    let fills: Vec<_> = pages[0]
        .ops()
        .iter()
        .filter(|op| matches!(op, CanvasCommand::FillPath { .. }))
        .collect();
    assert_eq!(fills.len(), 1);
}

#[test]
fn footer_sees_section_counters_equal_to_document_totals() {
    if skip_without_font() {
        return;
    }
    let pages = font_pages(
        PageSize::A4,
        SectionSpec {
            margins: Margins::uniform(nn(20.0)),
            footer: Some(BandSpec {
                left: Some(slot_text(vec![
                    TextRun::SectionNumber,
                    TextRun::Literal("/".into()),
                    TextRun::SectionCount,
                ])),
                ..BandSpec::default()
            }),
            body: vec![para("A"), Block::PageBreak, para("B")],
            ..SectionSpec::default()
        },
    )
    .unwrap()
    .pages();
    assert_eq!(texts(&pages[0]), ["A", "1/2"]);
    assert_eq!(texts(&pages[1]), ["B", "2/2"]);
    let footer = &runs(&pages[0])[1];
    assert!(pt_near(footer.y, A4.height().as_pt() - 20.0 + baseline()));
}

#[test]
fn a_later_header_call_replaces_the_earlier_one() {
    if skip_without_font() {
        return;
    }
    let pages = font_pages(
        PageSize::A4,
        SectionSpec {
            margins: Margins::uniform(nn(20.0)),
            header: Some(BandSpec {
                right: Some(slot_text(vec![TextRun::Literal("new".into())])),
                ..BandSpec::default()
            }),
            body: vec![para("A")],
            ..SectionSpec::default()
        },
    )
    .unwrap()
    .pages();
    assert_eq!(texts(&pages[0]), ["new", "A"]);
}

#[test]
fn header_with_zero_margin_is_rejected() {
    if skip_without_font() {
        return;
    }
    let result = font_pages(
        PageSize::A4,
        SectionSpec {
            header: Some(left_band("H")),
            body: vec![para("A")],
            ..SectionSpec::default()
        },
    );
    assert!(
        matches!(result.map_err(|error| error.first().kind.clone()), Err(ErrorKind::DecorationTallerThanMargin { height, margin }) if pt_near(height, line()) && margin == 0.0)
    );
}

#[test]
fn lists_number_bullet_and_custom_markers() {
    if skip_without_font() {
        return;
    }
    let pages = font_pages(
        PageSize::A4,
        sec(vec![
            bullet_list(vec![plain_item(vec![para("dot")])]),
            Block::List(ListSpec {
                kind: ListKind::Ordered(Numbering::UpperRoman),
                items: vec![
                    plain_item(vec![para("one")]),
                    plain_item(vec![para("two")]),
                    marked_item("(c)", vec![para("custom")]),
                ],
                ..ListSpec::default()
            }),
        ]),
    )
    .unwrap()
    .pages();
    assert_eq!(
        texts(&pages[0]),
        ["\u{2022}", "dot", "I.", "one", "II.", "two", "(c)", "custom"]
    );
    let r = runs(&pages[0]);
    assert!(pt_near(r[0].x, 0.0));
    assert!(pt_near(r[1].x, 18.0));
}

#[test]
fn custom_marker_between_decimal_items_does_not_advance_the_counter() {
    if skip_without_font() {
        return;
    }
    let pages = font_pages(
        PageSize::A4,
        sec(vec![ordered_list(
            Numbering::Decimal,
            1,
            vec![
                plain_item(vec![para("one")]),
                marked_item("(c)", vec![para("custom")]),
                plain_item(vec![para("two")]),
            ],
        )]),
    )
    .unwrap()
    .pages();
    assert_eq!(
        texts(&pages[0]),
        ["1.", "one", "(c)", "custom", "2.", "two"]
    );
}

#[test]
fn ordered_list_start_numbers_from_the_given_value() {
    if skip_without_font() {
        return;
    }
    let pages = font_pages(
        PageSize::A4,
        sec(vec![ordered_list(
            Numbering::Decimal,
            4,
            vec![
                plain_item(vec![para("four")]),
                plain_item(vec![para("five")]),
            ],
        )]),
    )
    .unwrap()
    .pages();
    assert_eq!(texts(&pages[0]), ["4.", "four", "5.", "five"]);
}

#[test]
fn custom_marker_wider_than_marker_width_errors() {
    if skip_without_font() {
        return;
    }
    let result = font_pages(
        PageSize::A4,
        sec(vec![Block::List(ListSpec {
            marker_width: pos(10.0),
            items: vec![marked_item("(1000)", vec![para("A")])],
            ..ListSpec::default()
        })]),
    );
    assert!(
        matches!(result.map_err(|error| error.first().kind.clone()), Err(ErrorKind::UnbreakableWiderThanFrame { avail, .. }) if pt_near(avail, 10.0))
    );
}

#[test]
fn nest_indent_shifts_the_inner_list() {
    if skip_without_font() {
        return;
    }
    let pages = font_pages(
        PageSize::A4,
        sec(vec![Block::List(ListSpec {
            nest_indent: Some(pos(30.0)),
            items: vec![plain_item(vec![
                para("outer"),
                bullet_list(vec![plain_item(vec![para("child")])]),
            ])],
            ..ListSpec::default()
        })]),
    )
    .unwrap()
    .pages();
    assert!(pt_near(glyph(&pages[0], "outer").x, 30.0));
    assert!(pt_near(glyph(&pages[0], "child").x, 48.0));
}

#[test]
fn table_spans_and_repeat_header_switch() {
    if skip_without_font() {
        return;
    }
    let build = |repeat: bool| {
        let mut body = Vec::new();
        for n in 0..80 {
            body.push(vec![
                text_cell(format!("row {n}")),
                text_cell(n.to_string()),
            ]);
        }
        body.push(vec![span_cell("Total", 2)]);
        font_pages(
            PageSize::A4,
            sec(vec![Block::Table(TableSpec {
                header: vec![text_row(&["Item", "Amount"])],
                body,
                repeat_header: repeat,
                ..TableSpec::default()
            })]),
        )
        .unwrap()
        .pages()
    };
    let on = build(true);
    let off = build(false);
    assert_eq!(on.len(), 2);
    assert_eq!(texts(&on[1])[..3], ["Item", "Amount", "row 47"]);
    assert_eq!(texts(&off[1])[..2], ["row 47", "47"]);
    let total = runs(on.last().unwrap()).pop().unwrap();
    assert_eq!(total.text, "Total");
    assert!(pt_near(total.x, 0.0));
}

#[test]
fn keep_together_spacer_and_page_break() {
    if skip_without_font() {
        return;
    }
    let pages = font_pages(
        PageSize::A4,
        sec(vec![
            para("A"),
            Block::Spacer(nn(A4.height().as_pt() - 20.0)),
            Block::KeepTogether(vec![para("B1"), para("B2")]),
            Block::PageBreak,
            para("C"),
        ]),
    )
    .unwrap()
    .pages();
    assert_eq!(
        pages.iter().map(texts).collect::<Vec<_>>(),
        [vec!["A"], vec!["B1", "B2"], vec!["C"]]
    );
    assert!(pt_near(runs(&pages[1])[0].y, baseline()));
}

#[test]
fn a_paragraph_wraps_at_the_width_of_its_column() {
    if skip_without_font() {
        return;
    }
    let page = Size::try_pt(200.0, 400.0).unwrap();
    let words = "AA AA AA AA AA AA";
    let lines = |in_column: bool| {
        let block = if in_column {
            gap_columns(
                12.0,
                vec![
                    ratio_col(1.0, vec![para(words)]),
                    ratio_col(1.0, vec![para("B")]),
                ],
            )
        } else {
            para(words)
        };
        runs(
            &font_pages(PageSize::custom(page), sec(vec![block]))
                .unwrap()
                .pages()[0],
        )
    };
    let col_w = (200.0 - 12.0) / 2.0;
    let column = lines(true);
    let wrapped: Vec<_> = column.iter().filter(|run| run.text != "B").collect();
    assert!(wrapped.len() > 1);
    assert!(wrapped.iter().all(|run| run.x + run.width <= col_w));
    assert_eq!(lines(false).len(), 1);
}

#[test]
fn nesting_rules_come_from_layout() {
    if skip_without_font() {
        return;
    }
    let result = font_pages(
        PageSize::A4,
        sec(vec![gap_columns(
            12.0,
            vec![
                ratio_col(1.0, vec![Block::PageBreak]),
                ratio_col(1.0, vec![para("x")]),
            ],
        )]),
    );
    assert_eq!(
        result.err().map(|error| error.first().kind.clone()),
        Some(ErrorKind::InvalidCellContent {
            container: "column",
            node: "page break"
        })
    );
}

#[test]
fn assembly_errors_follow_discovery_order() {
    if skip_without_font() {
        return;
    }
    let errors = font_pages(
        PageSize::A4,
        sec(vec![
            gap_columns(12.0, vec![ratio_col(1.0, vec![para("only one")])]),
            grid(vec![], vec![text_row(&["a", "b"]), text_row(&["c"])]),
        ]),
    )
    .err()
    .unwrap();
    assert_eq!(
        errors
            .as_slice()
            .iter()
            .map(|error| error.kind.clone())
            .collect::<Vec<_>>(),
        vec![
            ErrorKind::ColumnWidths {
                reason: ColumnWidthsReason::ColumnCount,
            },
            ErrorKind::InvalidCellSpan(CellSpanReason::EmptySlot),
        ]
    );
    assert_eq!(
        location(&errors.as_slice()[1]),
        (Some(0), None, None, Some((1, 1)))
    );
    assert!(!errors.truncated());
}

#[test]
fn a_cell_error_is_kept_when_the_table_also_fails() {
    if skip_without_font() {
        return;
    }
    let errors = font_pages(
        PageSize::A4,
        sec(vec![Block::Table(TableSpec {
            tracks: Some(vec![Track::Ratio(Ratio::new(1.0).unwrap())]),
            body: vec![vec![
                cell(vec![CellBlock::Paragraph(ParagraphSpec {
                    runs: vec![link_run("", vec![text_run("x")])],
                    ..ParagraphSpec::default()
                })]),
                text_cell("b"),
            ]],
            ..TableSpec::default()
        })]),
    )
    .err()
    .unwrap();
    assert_eq!(
        errors
            .as_slice()
            .iter()
            .map(|error| error.kind.clone())
            .collect::<Vec<_>>(),
        vec![
            ErrorKind::EmptyLink,
            ErrorKind::ColumnWidths {
                reason: ColumnWidthsReason::ColumnCount,
            },
        ]
    );
}

#[test]
fn missing_glyphs_in_later_paragraphs_are_all_reported() {
    if skip_without_font() {
        return;
    }
    let errors = font_pages(
        PageSize::A4,
        sec(vec![para("A\u{FFFF}"), para("B\u{FFFE}")]),
    )
    .err()
    .unwrap();
    assert_eq!(
        errors
            .as_slice()
            .iter()
            .map(|error| error.kind.clone())
            .collect::<Vec<_>>(),
        vec![
            ErrorKind::MissingGlyph {
                font: 0,
                scalar: '\u{FFFF}',
            },
            ErrorKind::MissingGlyph {
                font: 0,
                scalar: '\u{FFFE}',
            },
        ]
    );
}

#[test]
fn assembly_keeps_sixty_four_errors_and_marks_truncation() {
    if skip_without_font() {
        return;
    }
    let body = (0..65).map(|_| para("\u{FFFF}")).collect();
    let errors = font_pages(PageSize::A4, sec(body)).err().unwrap();
    assert_eq!(errors.as_slice().len(), 64);
    assert!(errors.truncated());
    assert!(errors.as_slice().iter().all(|error| {
        matches!(
            &error.kind,
            ErrorKind::MissingGlyph {
                font: 0,
                scalar: '\u{FFFF}',
            }
        )
    }));
    assert!(errors.to_string().ends_with("further errors omitted"));
}

#[test]
fn text_without_a_font_fails() {
    let result = render(
        PageSize::A4,
        Resources::default(),
        vec![sec(vec![para("A")])],
    );
    assert_eq!(
        result.err().map(|error| error.first().kind.clone()),
        Some(ErrorKind::UnregisteredFont {
            key: "()".to_owned(),
        })
    );
}

#[test]
fn mixed_body_and_bold_runs_keep_fonts_and_origins() {
    if skip_without_font() {
        return;
    }
    let page = &bold_pages(
        PageSize::A4,
        sec(vec![runs_para(vec![
            text_run("A"),
            bold_run("B"),
            text_run("C"),
        ])]),
    )
    .unwrap()
    .pages()[0];
    let recorded = runs(page);
    assert_eq!(
        recorded
            .iter()
            .map(|run| (run.text.as_str(), run.font))
            .collect::<Vec<_>>(),
        [("A", 0), ("B", 1), ("C", 0)]
    );
    assert!(pt_near(recorded[1].x, recorded[0].x + recorded[0].width));
    assert!(pt_near(recorded[2].x, recorded[1].x + recorded[1].width));
    assert!(recorded.iter().all(|run| pt_near(run.y, recorded[0].y)));
}

#[test]
fn each_font_key_keeps_its_own_face() {
    #[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
    enum DocFont {
        Latin,
        Cjk,
    }

    if skip_without_font() {
        return;
    }

    let bytes = font_bytes();
    let mut fonts = FontRegistry::new();
    let family = |bytes: Vec<u8>| FontFamily {
        regular: FontFace::new(bytes, 0),
        bold: None,
        italic: None,
        bold_italic: None,
    };
    fonts.register(DocFont::Latin, family(bytes.clone()));
    fonts.register(DocFont::Cjk, family(bytes));
    let rendered = layout_spec(DocumentSpec {
        page_size: PageSize::A4,
        resources: Resources::with_fonts(fonts),
        limits: pdfcraft_pdf::Limits::default(),
        info: pdfcraft_pdf::spec::DocumentInfo::default(),
        sections: vec![SectionSpec {
            body: vec![Block::Paragraph(ParagraphSpec {
                runs: vec![
                    InlineRun::Text {
                        text: "A".into(),
                        font: DocFont::Latin,
                        style: RunStyle::default(),
                    },
                    InlineRun::Text {
                        text: "B".into(),
                        font: DocFont::Cjk,
                        style: RunStyle::default(),
                    },
                ],
                ..ParagraphSpec::default()
            })],
            ..SectionSpec::default()
        }],
    })
    .unwrap();
    let page = &rendered.pages()[0];
    assert_eq!(
        runs(page)
            .iter()
            .map(|run| (run.text.as_str(), run.font))
            .collect::<Vec<_>>(),
        [("A", 0), ("B", 1)]
    );
}

#[test]
fn first_line_indent_wraps_inside_the_inset_and_does_not_repeat_after_pagination() {
    if skip_without_font() {
        return;
    }
    let rendered = font_pages(
        PageSize::custom(Size::try_pt(24.0, line()).unwrap()),
        sec(vec![paragraph(
            vec![text_run("第第第")],
            12.0,
            Alignment::Start,
            None,
            None,
        )]),
    )
    .unwrap();
    let pages = rendered.pages();
    assert_eq!(pages.len(), 2);
    let first = runs(&pages[0]);
    let second = runs(&pages[1]);
    assert_eq!(first[0].text, "第");
    assert!(pt_near(first[0].x, 12.0));
    assert_eq!(second[0].text, "第第");
    assert!(pt_near(second[0].x, 0.0));
}

#[test]
fn bold_and_indent_configuration_errors_are_specific() {
    if skip_without_font() {
        return;
    }
    let missing_bold = font_pages(PageSize::A4, sec(vec![runs_para(vec![bold_run("A")])]))
        .err()
        .unwrap();
    assert_eq!(
        missing_bold.first().kind.clone(),
        ErrorKind::BoldFaceRequired
    );

    let invalid_indent = font_pages(
        PageSize::custom(Size::try_pt(24.0, 40.0).unwrap()),
        sec(vec![paragraph(
            vec![text_run("A")],
            24.0,
            Alignment::Start,
            None,
            None,
        )]),
    )
    .err()
    .unwrap();
    assert_eq!(
        invalid_indent.first().kind.clone(),
        ErrorKind::InvalidGeometry(GeometryKind::FirstLineIndent {
            indent: 24.0,
            width: 24.0,
        })
    );

    let overwide = font_pages(
        PageSize::custom(Size::try_pt(24.0, 40.0).unwrap()),
        sec(vec![paragraph(
            vec![text_run("AA")],
            12.0,
            Alignment::Start,
            None,
            None,
        )]),
    )
    .err()
    .unwrap();
    assert!(
        matches!(overwide.first().kind.clone(), ErrorKind::UnbreakableWiderThanFrame { avail, .. } if pt_near(avail, 12.0)
        ),
        "{overwide:?}"
    );
}

#[cfg(feature = "backend-krilla")]
#[test]
fn mixed_bold_and_indent_write_a_pdf() {
    if skip_without_font() {
        return;
    }
    let rendered = bold_pages(
        PageSize::A4,
        sec(vec![paragraph(
            vec![text_run("Contract "), bold_run("A")],
            24.0,
            Alignment::Start,
            None,
            None,
        )]),
    )
    .unwrap();
    let pdf = to_pdf(rendered).unwrap();
    assert!(pdf.starts_with(b"%PDF"));
}

#[cfg(not(feature = "backend-krilla"))]
#[test]
fn load_image_without_decoder_is_an_image_error() {
    let mut resources = Resources::default();
    let _ = resources.add_image(ONE_PIXEL.to_vec(), ImageFormat::Png);
    let err = render(PageSize::A4, resources, vec![SectionSpec::default()])
        .err()
        .unwrap();
    assert!(matches!(err.first().kind.clone(), ErrorKind::Image { .. }));
}

#[cfg(feature = "backend-krilla")]
fn image_rects(page: &RecordedPage) -> Vec<(u32, u32, u32, Rect)> {
    page.ops()
        .iter()
        .filter_map(|op| match op {
            CanvasCommand::DrawImage { image, rect } => Some((
                image.id.as_u32(),
                image.pixel_width,
                image.pixel_height,
                *rect,
            )),
            _ => None,
        })
        .collect()
}

#[cfg(feature = "backend-krilla")]
fn rect_bits(rect: Rect) -> (u32, u32, u32, u32) {
    (
        rect.origin().x().as_pt().to_bits(),
        rect.origin().y().as_pt().to_bits(),
        rect.size().width().as_pt().to_bits(),
        rect.size().height().as_pt().to_bits(),
    )
}

#[cfg(feature = "backend-krilla")]
#[test]
fn image_only_document_needs_no_font() {
    let mut resources = Resources::default();
    let logo = resources.add_image(ONE_PIXEL.to_vec(), ImageFormat::Png);
    let rendered = render(
        PageSize::custom(Size::try_pt(200.0, 400.0).unwrap()),
        resources,
        vec![sec(vec![image_block(logo, Some(40.0), Alignment::Center)])],
    )
    .unwrap();
    let images = image_rects(&rendered.pages()[0]);
    assert_eq!(images.len(), 1);
    assert_eq!(images[0].0, 0);
    assert_eq!(images[0].1, 1);
    assert_eq!(images[0].2, 1);
    assert_eq!(
        rect_bits(images[0].3),
        rect_bits(Rect::try_pt(80.0, 0.0, 40.0, 40.0).unwrap())
    );
    assert!(to_pdf(rendered).unwrap().starts_with(b"%PDF"));
}

#[cfg(feature = "backend-krilla")]
#[test]
fn cell_image_and_last_page_seal_share_coordinates() {
    if skip_without_font() {
        return;
    }
    let mut resources = font_resources();
    let seal = resources.add_image(ONE_PIXEL.to_vec(), ImageFormat::Png);
    let rendered = render(
        PageSize::custom(Size::try_pt(200.0, 80.0).unwrap()),
        resources,
        vec![SectionSpec {
            body: vec![
                Block::Table(TableSpec {
                    body: vec![vec![
                        text_cell("A"),
                        cell(vec![CellBlock::Image(ImageSpec {
                            image: seal,
                            width: Some(pos(10.0)),
                            height: None,
                            align: Alignment::Start,
                        })]),
                    ]],
                    ..TableSpec::default()
                }),
                Block::PageBreak,
                para("B"),
            ],
            front: vec![LayerMark::Image {
                when: PageWhen::Last,
                image: seal,
                rect: Rect::try_pt(150.0, 40.0, 20.0, 20.0).unwrap(),
            }],
            ..SectionSpec::default()
        }],
    )
    .unwrap();
    let pages = rendered.pages();
    assert_eq!(pages.len(), 2);
    let cell_image = image_rects(&pages[0]);
    assert_eq!(cell_image.len(), 1);
    assert_eq!(
        rect_bits(cell_image[0].3),
        rect_bits(Rect::try_pt(100.0, 0.0, 10.0, 10.0).unwrap())
    );
    let placed = image_rects(&pages[1]);
    assert_eq!(placed.len(), 1);
    assert_eq!(
        rect_bits(placed[0].3),
        rect_bits(Rect::try_pt(150.0, 40.0, 20.0, 20.0).unwrap())
    );
    assert_eq!(placed[0].0, cell_image[0].0);
}

#[cfg(feature = "backend-krilla")]
#[test]
fn three_pages_of_one_image_share_one_xobject() {
    let mut resources = Resources::default();
    let seal = resources.add_image(ONE_PIXEL.to_vec(), ImageFormat::Png);
    let image = || image_block(seal, None, Alignment::Start);
    let rendered = render(
        PageSize::custom(Size::try_pt(120.0, 40.0).unwrap()),
        resources,
        vec![sec(vec![
            image(),
            Block::PageBreak,
            image(),
            Block::PageBreak,
            image(),
        ])],
    )
    .unwrap();
    let pdf = to_pdf(rendered).unwrap();
    assert!(pdf.starts_with(b"%PDF"));
    let text = String::from_utf8_lossy(&pdf);
    let count = text.matches("/Subtype /Image").count() + text.matches("/Subtype/Image").count();
    assert_eq!(count, 1, "{text}");
}

#[cfg(feature = "backend-krilla")]
#[test]
fn garbage_empty_and_foreign_handles_are_image_errors() {
    let mut garbage = Resources::default();
    let _ = garbage.add_image(b"not a png".to_vec(), ImageFormat::Png);
    let garbage = render(PageSize::A4, garbage, vec![SectionSpec::default()])
        .err()
        .unwrap();
    assert!(matches!(
        garbage.first().kind.clone(),
        ErrorKind::Image { .. }
    ));

    let mut empty = Resources::default();
    let _ = empty.add_image(Vec::new(), ImageFormat::Png);
    let empty = render(PageSize::A4, empty, vec![SectionSpec::default()])
        .err()
        .unwrap();
    assert!(
        matches!(empty.first().kind.clone(), ErrorKind::Image { message } if message == "bytes are empty")
    );

    let stolen = {
        let mut other = Resources::<()>::default();
        other.add_image(ONE_PIXEL.to_vec(), ImageFormat::Png)
    };
    let foreign = render(
        PageSize::A4,
        Resources::default(),
        vec![sec(vec![image_block(stolen, None, Alignment::Start)])],
    )
    .err()
    .unwrap();
    assert!(matches!(
        foreign.first().kind.clone(),
        ErrorKind::UnknownImage { id: 0 }
    ));
}

#[cfg(feature = "backend-krilla")]
#[test]
fn list_item_and_overwide_image_fail() {
    let mut listed_resources = Resources::default();
    let logo = listed_resources.add_image(ONE_PIXEL.to_vec(), ImageFormat::Png);
    let listed = render(
        PageSize::A4,
        listed_resources,
        vec![sec(vec![bullet_list(vec![plain_item(vec![image_block(
            logo,
            None,
            Alignment::Start,
        )])])])],
    )
    .err()
    .unwrap();
    assert_eq!(
        listed.first().kind.clone(),
        ErrorKind::InvalidCellContent {
            container: "list item",
            node: "image"
        }
    );

    let mut wide_resources = Resources::default();
    let logo = wide_resources.add_image(ONE_PIXEL.to_vec(), ImageFormat::Png);
    let wide = render(
        PageSize::custom(Size::try_pt(100.0, 100.0).unwrap()),
        wide_resources,
        vec![sec(vec![image_block(logo, Some(101.0), Alignment::Start)])],
    )
    .err()
    .unwrap();
    assert_eq!(
        wide.first().kind.clone(),
        ErrorKind::UnbreakableWiderThanFrame {
            text: None,
            width: 101.0,
            avail: 100.0,
        }
    );
}

#[cfg(feature = "backend-krilla")]
#[test]
fn damaged_png_pixels_fail_at_load() {
    let mut resources = Resources::default();
    let _ = resources.add_image(BAD_IDAT.to_vec(), ImageFormat::Png);
    let err = render(PageSize::A4, resources, vec![SectionSpec::default()])
        .err()
        .unwrap();
    let ErrorKind::Image { message } = &err.first().kind else {
        panic!("{err:?}");
    };
    assert!(message.starts_with("Png decode failed:"), "{message}");
}

#[cfg(feature = "backend-krilla")]
#[test]
fn to_pdf_writes_a_pdf() {
    if skip_without_font() {
        return;
    }
    let pdf = to_pdf(full_template(&contract(Kind::A), "ACME").unwrap()).unwrap();
    assert!(pdf.starts_with(b"%PDF"));
}

fn counter_footer() -> BandSpec<()> {
    BandSpec {
        left: Some(slot_text(vec![
            TextRun::PageNumber,
            TextRun::Literal("/".into()),
            TextRun::PageCount,
        ])),
        right: Some(slot_text(vec![
            TextRun::SectionNumber,
            TextRun::Literal("/".into()),
            TextRun::SectionCount,
        ])),
        ..BandSpec::default()
    }
}

#[test]
fn two_sections_number_document_pages_and_section_pages() {
    if skip_without_font() {
        return;
    }
    let margin = Margins::uniform(nn(20.0));
    let pages = render(
        PageSize::A4,
        font_resources(),
        vec![
            SectionSpec {
                margins: margin,
                footer: Some(counter_footer()),
                body: vec![para("A"), Block::PageBreak, para("B")],
                ..SectionSpec::default()
            },
            SectionSpec {
                margins: margin,
                footer: Some(counter_footer()),
                body: vec![para("C")],
                ..SectionSpec::default()
            },
        ],
    )
    .unwrap()
    .pages();
    assert_eq!(pages.len(), 3);
    assert_eq!(texts(&pages[0]), ["A", "1/3", "1/2"]);
    assert_eq!(texts(&pages[1]), ["B", "2/3", "2/2"]);
    assert_eq!(texts(&pages[2]), ["C", "3/3", "1/1"]);
}

#[test]
fn second_section_header_stays_at_page_edge_until_caller_insets() {
    if skip_without_font() {
        return;
    }
    let pages = render(
        PageSize::A4,
        font_resources(),
        vec![
            SectionSpec {
                margins: Margins::uniform(nn(20.0)),
                header: Some(left_band("One")),
                body: vec![para("A")],
                ..SectionSpec::default()
            },
            SectionSpec {
                margins: Margins::new(nn(20.0), nn(10.0), nn(20.0), nn(48.0)),
                header: Some(BandSpec {
                    inset: BandInset::PageMargins,
                    left: Some(slot_text(vec![TextRun::Literal("Two".into())])),
                    right: None,
                }),
                body: vec![para("B")],
                ..SectionSpec::default()
            },
        ],
    )
    .unwrap()
    .pages();
    assert_eq!(pages.len(), 2);
    assert!(pt_near(runs(&pages[0])[0].x, 0.0));
    assert_eq!(runs(&pages[0])[0].text, "One");
    assert!(pt_near(runs(&pages[1])[0].x, 48.0));
    assert_eq!(runs(&pages[1])[0].text, "Two");
}

#[test]
fn empty_section_is_one_page_and_still_draws_its_footer() {
    if skip_without_font() {
        return;
    }
    let pages = font_pages(
        PageSize::A4,
        SectionSpec {
            margins: Margins::uniform(nn(20.0)),
            footer: Some(left_band("foot")),
            ..SectionSpec::default()
        },
    )
    .unwrap()
    .pages();
    assert_eq!(pages.len(), 1);
    assert_eq!(texts(&pages[0]), ["foot"]);
}

#[test]
fn trailing_page_break_inside_a_section_does_not_add_a_page() {
    if skip_without_font() {
        return;
    }
    let pages = font_pages(PageSize::A4, sec(vec![para("A"), Block::PageBreak]))
        .unwrap()
        .pages();
    assert_eq!(pages.len(), 1);
    assert_eq!(texts(&pages[0]), ["A"]);
}

#[cfg(feature = "backend-krilla")]
#[test]
fn two_sections_on_different_paper_write_a_pdf() {
    if skip_without_font() {
        return;
    }
    let rendered = render(
        PageSize::A4,
        font_resources(),
        vec![
            sec(vec![para("A")]),
            SectionSpec {
                page_size: Some(Paper::Letter.landscape()),
                body: vec![para("B")],
                ..SectionSpec::default()
            },
        ],
    )
    .unwrap();
    let pages = rendered.pages();
    assert_eq!(pages.len(), 2);
    assert_eq!(pages[0].size(), A4);
    assert_eq!(pages[1].size(), Paper::Letter.landscape().resolve());
    assert!(to_pdf(rendered).unwrap().starts_with(b"%PDF"));
}

fn ellipse_center_radii(path: &Path) -> Option<(f32, f32, f32, f32)> {
    let [PathVerb::MoveTo(right), PathVerb::CubicTo { to: bottom, .. }, PathVerb::CubicTo { to: left, .. }, PathVerb::CubicTo { to: top, .. }, PathVerb::CubicTo { .. }, PathVerb::Close] =
        path.verbs()
    else {
        return None;
    };
    Some((
        f32::midpoint(right.x().as_pt(), left.x().as_pt()),
        f32::midpoint(bottom.y().as_pt(), top.y().as_pt()),
        (right.x().as_pt() - left.x().as_pt()) * 0.5,
        (bottom.y().as_pt() - top.y().as_pt()) * 0.5,
    ))
}

fn painted_layer_paths() -> Rendered {
    font_pages(
        PageSize::custom(Size::try_pt(200.0, 160.0).unwrap()),
        SectionSpec {
            margins: Margins::uniform(nn(20.0)),
            header: Some(left_band("H")),
            body: vec![para("A")],
            behind: vec![
                LayerMark::Line {
                    when: PageWhen::All,
                    from: Point::try_pt(0.0, 0.0).unwrap(),
                    to: Point::try_pt(40.0, 40.0).unwrap(),
                    pen: Pen::new(Color::rgba(200, 0, 0, 64), 0.8).unwrap(),
                },
                LayerMark::Rect {
                    when: PageWhen::All,
                    rect: Rect::try_pt(4.0, 6.0, 10.0, 12.0).unwrap(),
                    ink: Ink::Stroke(Pen::new(Color::rgb(0, 0, 0), 0.5).unwrap()),
                },
            ],
            front: vec![
                mark_text(PageWhen::All, "SEAL", 12.0, 30.0, RunStyle::default()),
                LayerMark::Ellipse {
                    when: PageWhen::All,
                    rect: Rect::try_pt(40.0, 50.0, 20.0, 10.0).unwrap(),
                    ink: Ink::FillStroke(
                        Color::rgba(200, 0, 0, 40),
                        Pen::new(Color::rgb(200, 0, 0), 2.0).unwrap(),
                    ),
                },
                LayerMark::Rotated {
                    when: PageWhen::All,
                    degrees: 90.0,
                    pivot: Point::try_pt(10.0, 12.0).unwrap(),
                    shapes: vec![LayerShape::Rect {
                        rect: Rect::try_pt(10.0, 12.0, 20.0, 8.0).unwrap(),
                        ink: Ink::Fill(Color::rgba(0, 0, 255, 128)),
                    }],
                },
                LayerMark::Rotated {
                    when: PageWhen::All,
                    degrees: 0.0,
                    pivot: Point::ZERO,
                    shapes: vec![LayerShape::Line {
                        from: Point::try_pt(1.0, 2.0).unwrap(),
                        to: Point::try_pt(8.0, 9.0).unwrap(),
                        pen: Pen::new(Color::rgb(0, 0, 0), 1.0).unwrap(),
                    }],
                },
            ],
            ..SectionSpec::default()
        },
    )
    .unwrap()
}

#[test]
fn layer_paths_keep_stack_order_and_rotate_via_transform() {
    if skip_without_font() {
        return;
    }
    let rendered = painted_layer_paths();
    let page = &rendered.pages()[0];
    let ops = page.ops();
    assert!(matches!(ops[0], CanvasCommand::StrokePath { .. }));
    assert!(matches!(ops[1], CanvasCommand::StrokePath { .. }));
    assert!(matches!(ops[2], CanvasCommand::DrawGlyphRun { .. }));
    assert!(matches!(ops[3], CanvasCommand::DrawGlyphRun { .. }));
    assert!(matches!(ops[4], CanvasCommand::DrawGlyphRun { .. }));
    assert!(matches!(ops[5], CanvasCommand::FillPath { .. }));
    assert!(matches!(ops[6], CanvasCommand::StrokePath { .. }));
    assert!(matches!(ops[7], CanvasCommand::PushTransform { .. }));
    assert!(matches!(ops[8], CanvasCommand::FillPath { .. }));
    assert!(matches!(ops[9], CanvasCommand::PopTransform));
    assert!(matches!(ops[10], CanvasCommand::StrokePath { .. }));
    assert_eq!(texts(page), ["H", "A", "SEAL"]);
    assert_behind_paths(ops);
    assert_front_paths(page, ops);
}

fn assert_behind_paths(ops: &[CanvasCommand]) {
    let CanvasCommand::StrokePath {
        path,
        paint,
        stroke,
    } = &ops[0]
    else {
        unreachable!();
    };
    assert_eq!(paint.color(), Color::rgba(200, 0, 0, 64));
    assert_eq!(stroke.width().as_pt().to_bits(), 0.8f32.to_bits());
    assert_eq!(
        path.verbs(),
        &[
            PathVerb::MoveTo(Point::try_pt(0.0, 0.0).unwrap()),
            PathVerb::LineTo(Point::try_pt(40.0, 40.0).unwrap()),
        ]
    );
    let CanvasCommand::StrokePath {
        path: frame,
        paint: frame_paint,
        stroke: frame_stroke,
    } = &ops[1]
    else {
        unreachable!();
    };
    assert_eq!(frame_paint.color(), Color::rgb(0, 0, 0));
    assert_eq!(frame_stroke.width().as_pt().to_bits(), 0.5f32.to_bits());
    assert_eq!(
        frame.verbs(),
        &[
            PathVerb::MoveTo(Point::try_pt(4.0, 6.0).unwrap()),
            PathVerb::LineTo(Point::try_pt(14.0, 6.0).unwrap()),
            PathVerb::LineTo(Point::try_pt(14.0, 18.0).unwrap()),
            PathVerb::LineTo(Point::try_pt(4.0, 18.0).unwrap()),
            PathVerb::Close,
        ]
    );
}

fn assert_front_paths(page: &RecordedPage, ops: &[CanvasCommand]) {
    let CanvasCommand::FillPath {
        path: filled,
        paint: fill_paint,
    } = &ops[5]
    else {
        unreachable!();
    };
    let CanvasCommand::StrokePath {
        path: stroked,
        paint: stroke_paint,
        stroke: ellipse_stroke,
    } = &ops[6]
    else {
        unreachable!();
    };
    assert_eq!(filled, stroked);
    assert_eq!(fill_paint.color(), Color::rgba(200, 0, 0, 40));
    assert_eq!(stroke_paint.color(), Color::rgb(200, 0, 0));
    assert_eq!(ellipse_stroke.width().as_pt().to_bits(), 2.0f32.to_bits());
    let (cx, cy, rx, ry) = ellipse_center_radii(filled).unwrap();
    assert_eq!(cx.to_bits(), 50.0f32.to_bits());
    assert_eq!(cy.to_bits(), 55.0f32.to_bits());
    assert_eq!(rx.to_bits(), 10.0f32.to_bits());
    assert_eq!(ry.to_bits(), 5.0f32.to_bits());

    let CanvasCommand::PushTransform { affine } = &ops[7] else {
        unreachable!();
    };
    assert!(pt_near(affine.a, 0.0));
    assert!(pt_near(affine.b, -1.0));
    assert!(pt_near(affine.c, 1.0));
    assert!(pt_near(affine.d, 0.0));
    assert!(pt_near(affine.tx, 22.0));
    assert!(pt_near(affine.ty, 2.0));
    let CanvasCommand::FillPath {
        path: unrotated,
        paint: turned_paint,
    } = &ops[8]
    else {
        unreachable!();
    };
    assert_eq!(turned_paint.color(), Color::rgba(0, 0, 255, 128));
    assert_eq!(
        unrotated.verbs(),
        &[
            PathVerb::MoveTo(Point::try_pt(10.0, 12.0).unwrap()),
            PathVerb::LineTo(Point::try_pt(30.0, 12.0).unwrap()),
            PathVerb::LineTo(Point::try_pt(30.0, 20.0).unwrap()),
            PathVerb::LineTo(Point::try_pt(10.0, 20.0).unwrap()),
            PathVerb::Close,
        ]
    );
    assert!(matches!(ops[9], CanvasCommand::PopTransform));

    let CanvasCommand::StrokePath { path: flat, .. } = &ops[10] else {
        unreachable!();
    };
    assert_eq!(
        flat.verbs(),
        &[
            PathVerb::MoveTo(Point::try_pt(1.0, 2.0).unwrap()),
            PathVerb::LineTo(Point::try_pt(8.0, 9.0).unwrap()),
        ]
    );
    let seal = runs(page)
        .into_iter()
        .find(|run| run.text == "SEAL")
        .unwrap();
    assert!(pt_near(seal.x, 12.0));
    assert!(pt_near(seal.y, 30.0 + baseline()));
}

#[cfg(feature = "backend-krilla")]
#[test]
fn layer_paths_write_a_pdf() {
    let rendered = render(
        PageSize::custom(Size::try_pt(80.0, 80.0).unwrap()),
        Resources::default(),
        vec![SectionSpec {
            behind: vec![
                LayerMark::Rect {
                    when: PageWhen::All,
                    rect: Rect::try_pt(10.0, 10.0, 20.0, 20.0).unwrap(),
                    ink: Ink::Fill(Color::rgba(0, 0, 0, 128)),
                },
                LayerMark::Ellipse {
                    when: PageWhen::All,
                    rect: Rect::try_pt(30.0, 30.0, 16.0, 12.0).unwrap(),
                    ink: Ink::Stroke(Pen::new(Color::rgb(0, 0, 0), 1.0).unwrap()),
                },
            ],
            ..SectionSpec::default()
        }],
    )
    .unwrap();
    assert!(to_pdf(rendered).unwrap().starts_with(b"%PDF"));
}

#[test]
fn rotated_layer_text_pushes_transform_on_every_page() {
    if skip_without_font() {
        return;
    }
    let rendered = font_pages(
        PageSize::custom(Size::try_pt(120.0, 80.0).unwrap()),
        SectionSpec {
            body: vec![para("one"), Block::PageBreak, para("two")],
            behind: vec![LayerMark::Rotated {
                when: PageWhen::All,
                degrees: 45.0,
                pivot: Point::try_pt(60.0, 40.0).unwrap(),
                shapes: vec![LayerShape::Text {
                    text: vec![TextRun::Literal("WM".into())],
                    font: (),
                    at: Point::try_pt(50.0, 35.0).unwrap(),
                    style: RunStyle {
                        font_size: Some(pos(12.0)),
                        color: Some(Color::rgba(0, 0, 0, 64)),
                        ..RunStyle::default()
                    },
                }],
            }],
            ..SectionSpec::default()
        },
    )
    .unwrap();
    assert_eq!(rendered.pages().len(), 2);
    for page in rendered.pages() {
        let ops = page.ops();
        let push = ops
            .iter()
            .position(|op| matches!(op, CanvasCommand::PushTransform { .. }))
            .expect("push");
        assert!(matches!(ops[push + 1], CanvasCommand::DrawGlyphRun { .. }));
        assert!(matches!(ops[push + 2], CanvasCommand::PopTransform));
        let CanvasCommand::DrawGlyphRun { run } = &ops[push + 1] else {
            unreachable!();
        };
        assert_eq!(run.text, "WM");
        assert!(pt_near(run.origin.x().as_pt(), 50.0));
        assert!(pt_near(run.origin.y().as_pt(), 35.0 + baseline()));
    }
}

#[test]
fn rotated_layer_image_keeps_pre_rotation_rect() {
    if skip_without_font() {
        return;
    }
    let mut resources = font_resources();
    let image = resources.add_image(ONE_PIXEL.to_vec(), ImageFormat::Png);
    let rendered = render(
        PageSize::custom(Size::try_pt(100.0, 100.0).unwrap()),
        resources,
        vec![SectionSpec {
            front: vec![LayerMark::Rotated {
                when: PageWhen::All,
                degrees: 90.0,
                pivot: Point::try_pt(20.0, 20.0).unwrap(),
                shapes: vec![LayerShape::Image {
                    image,
                    rect: Rect::try_pt(10.0, 10.0, 20.0, 15.0).unwrap(),
                }],
            }],
            ..SectionSpec::default()
        }],
    )
    .unwrap();
    let pages = rendered.pages();
    let ops = pages[0].ops();
    assert!(matches!(ops[0], CanvasCommand::PushTransform { .. }));
    let CanvasCommand::DrawImage { rect, .. } = &ops[1] else {
        panic!("expected image inside transform, got {:?}", ops[1]);
    };
    assert_eq!(rect, &Rect::try_pt(10.0, 10.0, 20.0, 15.0).unwrap());
    assert!(matches!(ops[2], CanvasCommand::PopTransform));
}

#[test]
fn rotated_mixed_path_and_text_keep_shapes_order() {
    if skip_without_font() {
        return;
    }
    let rendered = font_pages(
        PageSize::custom(Size::try_pt(100.0, 80.0).unwrap()),
        SectionSpec {
            behind: vec![LayerMark::Rotated {
                when: PageWhen::All,
                degrees: 30.0,
                pivot: Point::try_pt(40.0, 40.0).unwrap(),
                shapes: vec![
                    LayerShape::Line {
                        from: Point::try_pt(0.0, 0.0).unwrap(),
                        to: Point::try_pt(10.0, 0.0).unwrap(),
                        pen: Pen::new(Color::rgb(0, 0, 0), 1.0).unwrap(),
                    },
                    LayerShape::Text {
                        text: vec![TextRun::Literal("T".into())],
                        font: (),
                        at: Point::try_pt(2.0, 4.0).unwrap(),
                        style: RunStyle::default(),
                    },
                ],
            }],
            ..SectionSpec::default()
        },
    )
    .unwrap();
    let pages = rendered.pages();
    let ops = pages[0].ops();
    assert!(matches!(ops[0], CanvasCommand::PushTransform { .. }));
    assert!(matches!(ops[1], CanvasCommand::StrokePath { .. }));
    assert!(matches!(ops[2], CanvasCommand::DrawGlyphRun { .. }));
    assert!(matches!(ops[3], CanvasCommand::PopTransform));
    let CanvasCommand::StrokePath { path, .. } = &ops[1] else {
        unreachable!();
    };
    assert_eq!(
        path.verbs(),
        &[
            PathVerb::MoveTo(Point::try_pt(0.0, 0.0).unwrap()),
            PathVerb::LineTo(Point::try_pt(10.0, 0.0).unwrap()),
        ]
    );
}

#[test]
fn empty_rotated_shapes_emit_nothing() {
    let rendered = render(
        PageSize::custom(Size::try_pt(80.0, 80.0).unwrap()),
        Resources::default(),
        vec![SectionSpec {
            behind: vec![LayerMark::Rotated {
                when: PageWhen::All,
                degrees: 45.0,
                pivot: Point::ZERO,
                shapes: vec![],
            }],
            ..SectionSpec::default()
        }],
    )
    .unwrap();
    assert!(rendered.pages()[0].ops().is_empty());
}

#[test]
fn rotated_nan_degrees_fails() {
    let error = layout_spec(DocumentSpec {
        page_size: PageSize::custom(Size::try_pt(80.0, 80.0).unwrap()),
        resources: Resources::<()>::default(),
        limits: pdfcraft_pdf::Limits::default(),
        info: pdfcraft_pdf::spec::DocumentInfo::default(),
        sections: vec![SectionSpec {
            behind: vec![LayerMark::Rotated {
                when: PageWhen::All,
                degrees: f32::NAN,
                pivot: Point::ZERO,
                shapes: vec![LayerShape::Line {
                    from: Point::try_pt(0.0, 0.0).unwrap(),
                    to: Point::try_pt(1.0, 0.0).unwrap(),
                    pen: Pen::new(Color::rgb(0, 0, 0), 1.0).unwrap(),
                }],
            }],
            ..SectionSpec::default()
        }],
    })
    .err()
    .unwrap();
    assert!(matches!(
        error.first().kind,
        ErrorKind::InvalidGeometry(GeometryKind::NonFiniteRotation { degrees }) if degrees.is_nan()
    ));
}

#[cfg(feature = "backend-krilla")]
#[test]
fn rotated_text_writes_a_pdf() {
    if skip_without_font() {
        return;
    }
    let rendered = font_pages(
        PageSize::custom(Size::try_pt(120.0, 80.0).unwrap()),
        SectionSpec {
            behind: vec![LayerMark::Rotated {
                when: PageWhen::All,
                degrees: 45.0,
                pivot: Point::try_pt(60.0, 40.0).unwrap(),
                shapes: vec![LayerShape::Text {
                    text: vec![TextRun::Literal("X".into())],
                    font: (),
                    at: Point::try_pt(50.0, 35.0).unwrap(),
                    style: RunStyle::default(),
                }],
            }],
            ..SectionSpec::default()
        },
    )
    .unwrap();
    assert!(to_pdf(rendered).unwrap().starts_with(b"%PDF"));
}

#[test]
fn hline_moves_whole_and_pins_stroke() {
    let pages = render(
        PageSize::custom(Size::try_pt(100.0, 40.0).unwrap()),
        Resources::default(),
        vec![sec(vec![
            Block::Spacer(nn(31.0)),
            rule(HLineSpec {
                thickness: pos(2.0),
                space_before: nn(4.0),
                space_after: nn(4.0),
                ..HLineSpec::default()
            }),
        ])],
    )
    .unwrap()
    .pages();
    assert_eq!(pages.len(), 2);
    assert!(pages[0]
        .ops()
        .iter()
        .all(|op| !matches!(op, CanvasCommand::StrokePath { .. })));
    let CanvasCommand::StrokePath {
        path,
        paint,
        stroke,
    } = &pages[1].ops()[0]
    else {
        panic!("expected stroke on page 2");
    };
    assert_eq!(paint.color(), Color::rgb(0, 0, 0));
    assert_eq!(*stroke, Stroke::new(pos(2.0)));
    assert_eq!(
        path.verbs(),
        &[
            PathVerb::MoveTo(Point::try_pt(0.0, 5.0).unwrap()),
            PathVerb::LineTo(Point::try_pt(100.0, 5.0).unwrap()),
        ]
    );
}

#[test]
fn hline_taller_than_body_fails_before_any_page() {
    let error = render(
        PageSize::custom(Size::try_pt(100.0, 20.0).unwrap()),
        Resources::default(),
        vec![sec(vec![rule(HLineSpec {
            space_before: nn(12.0),
            space_after: nn(12.0),
            ..HLineSpec::default()
        })])],
    )
    .err()
    .unwrap();
    assert_eq!(
        error.first().kind.clone(),
        ErrorKind::UnsplittableTallerThanFrame {
            height: 25.0,
            avail: 20.0,
        }
    );
    assert_eq!(
        location(error.first()),
        (Some(0), Some(1), Some(Region::Body), None)
    );
}

#[test]
fn hline_in_list_item_and_cell() {
    if skip_without_font() {
        return;
    }
    let page = &font_pages(
        PageSize::custom(Size::try_pt(200.0, 200.0).unwrap()),
        sec(vec![
            bullet_list(vec![plain_item(vec![rule(HLineSpec {
                width: Some(pos(40.0)),
                align: Alignment::Start,
                ..HLineSpec::default()
            })])]),
            grid(
                vec![],
                vec![vec![cell(vec![CellBlock::HLine(HLineSpec {
                    color: Color::rgb(9, 9, 9),
                    ..HLineSpec::default()
                })])]],
            ),
        ]),
    )
    .unwrap()
    .pages()[0];
    let strokes: Vec<_> = page
        .ops()
        .iter()
        .filter(|op| matches!(op, CanvasCommand::StrokePath { .. }))
        .collect();
    assert!(strokes.len() >= 2);
    let CanvasCommand::StrokePath { path, .. } = strokes[0] else {
        panic!("list hline");
    };
    assert_eq!(
        path.verbs(),
        &[
            PathVerb::MoveTo(Point::try_pt(18.0, 0.5).unwrap()),
            PathVerb::LineTo(Point::try_pt(58.0, 0.5).unwrap()),
        ]
    );
}

#[cfg(feature = "backend-krilla")]
#[test]
fn hline_writes_a_pdf() {
    let rendered = render(
        PageSize::A4,
        Resources::default(),
        vec![sec(vec![rule(HLineSpec::default())])],
    )
    .unwrap();
    assert!(to_pdf(rendered).unwrap().starts_with(b"%PDF"));
}

#[test]
fn cell_spacer_shifts_the_following_row() {
    if skip_without_font() {
        return;
    }
    let y_of_b = |height: f32| {
        let pages = font_pages(
            PageSize::A4,
            sec(vec![grid(
                vec![],
                vec![
                    vec![cell(vec![CellBlock::Spacer(nn(height))])],
                    text_row(&["B"]),
                ],
            )]),
        )
        .unwrap()
        .pages();
        assert_eq!(pages.len(), 1);
        assert!(pages[0]
            .ops()
            .iter()
            .all(|op| matches!(op, CanvasCommand::DrawGlyphRun { .. })));
        assert_eq!(texts(&pages[0]), ["B"]);
        runs(&pages[0])[0].y
    };
    assert!(pt_near(y_of_b(30.0) - y_of_b(0.0), 30.0));
}

#[test]
fn cell_spacer_alone_records_no_ops() {
    let pages = render(
        PageSize::A4,
        Resources::default(),
        vec![sec(vec![grid(
            vec![],
            vec![vec![cell(vec![CellBlock::Spacer(nn(24.0))])]],
        )])],
    )
    .unwrap()
    .pages();
    assert_eq!(pages.len(), 1);
    assert!(pages[0].ops().is_empty());
}

#[test]
fn cell_spacer_taller_than_body_fails_before_any_page() {
    let error = render(
        PageSize::custom(Size::try_pt(100.0, 20.0).unwrap()),
        Resources::default(),
        vec![sec(vec![grid(
            vec![],
            vec![vec![cell(vec![CellBlock::Spacer(nn(21.0))])]],
        )])],
    )
    .err()
    .unwrap();
    assert_eq!(
        error.first().kind.clone(),
        ErrorKind::UnsplittableTallerThanFrame {
            height: 21.0,
            avail: 20.0,
        }
    );
    assert_eq!(
        location(error.first()),
        (Some(0), Some(1), Some(Region::Body), Some((0, 0)))
    );
}

#[test]
fn short_cell_spacer_does_not_grow_a_text_row() {
    if skip_without_font() {
        return;
    }
    let pages = font_pages(
        PageSize::A4,
        sec(vec![grid(
            vec![],
            vec![
                vec![text_cell("A"), cell(vec![CellBlock::Spacer(nn(8.0))])],
                text_row(&["B", "C"]),
            ],
        )]),
    )
    .unwrap()
    .pages();
    assert_eq!(pages.len(), 1);
    assert!(pt_near(glyph(&pages[0], "B").y, line() + baseline()));
}
struct StrokeSample {
    x0: f32,
    y0: f32,
    x1: f32,
    y1: f32,
    width: f32,
    color: Color,
}

fn stroke_samples(page: &RecordedPage) -> Vec<StrokeSample> {
    page.ops()
        .iter()
        .filter_map(|op| {
            let CanvasCommand::StrokePath {
                path,
                paint,
                stroke,
            } = op
            else {
                return None;
            };
            let [PathVerb::MoveTo(from), PathVerb::LineTo(to)] = path.verbs() else {
                return None;
            };
            Some(StrokeSample {
                x0: from.x().as_pt(),
                y0: from.y().as_pt(),
                x1: to.x().as_pt(),
                y1: to.y().as_pt(),
                width: stroke.width().as_pt(),
                color: paint.color(),
            })
        })
        .collect()
}

fn glyph(page: &RecordedPage, text: &str) -> GlyphSample {
    runs(page)
        .into_iter()
        .find(|run| run.text == text)
        .unwrap_or_else(|| panic!("missing glyph run {text}"))
}

struct LinkHit {
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    target: String,
}

fn links(page: &RecordedPage) -> Vec<LinkHit> {
    page.ops()
        .iter()
        .filter_map(|op| match op {
            CanvasCommand::AnnotateLink { rect, target } => Some(LinkHit {
                x: rect.origin().x().as_pt(),
                y: rect.origin().y().as_pt(),
                width: rect.size().width().as_pt(),
                height: rect.size().height().as_pt(),
                target: target.uri_str().unwrap_or("").to_owned(),
            }),
            _ => None,
        })
        .collect()
}

fn commands_without_links(page: &RecordedPage) -> Vec<CanvasCommand> {
    page.ops()
        .iter()
        .filter(|op| !matches!(op, CanvasCommand::AnnotateLink { .. }))
        .cloned()
        .collect()
}

#[test]
fn middle_blank_shifts_the_next_glyph_and_strokes_the_gap() {
    if skip_without_font() {
        return;
    }
    let pages = font_pages(
        PageSize::custom(Size::try_pt(200.0, 400.0).unwrap()),
        sec(vec![runs_para(vec![
            text_run("AB"),
            blank_run(BlankWidth::Fixed(nn(30.0)), Some(2.0), None, Some(4.0)),
            text_run("C"),
        ])]),
    )
    .unwrap()
    .pages();
    let ab = glyph(&pages[0], "AB");
    let c = glyph(&pages[0], "C");
    assert!(pt_near(c.x, ab.x + ab.width + 30.0), "c.x {}", c.x);
    let stroke = &stroke_samples(&pages[0])[0];
    assert!(pt_near(stroke.x0, ab.x + ab.width));
    assert!(pt_near(stroke.x1, c.x));
    assert!(pt_near(stroke.y0, ab.y + 4.0));
    assert!(pt_near(stroke.y1, stroke.y0));
    assert!(pt_near(stroke.width, 2.0));
    assert_eq!(stroke.color, Color::rgb(0, 0, 0));
}

#[test]
fn blank_does_not_change_line_height() {
    if skip_without_font() {
        return;
    }
    let y_of_b = |with_blank: bool| {
        let runs = if with_blank {
            vec![
                text_run("A"),
                blank_run(BlankWidth::Fixed(nn(20.0)), None, None, None),
                text_run("\nB"),
            ]
        } else {
            vec![text_run("A\nB")]
        };
        let pages = font_pages(PageSize::A4, sec(vec![runs_para(runs)]))
            .unwrap()
            .pages();
        glyph(&pages[0], "B").y
    };
    assert!(pt_near(y_of_b(true), y_of_b(false)));
}

#[test]
fn fill_blank_reaches_the_frame_edge_with_the_default_stroke() {
    if skip_without_font() {
        return;
    }
    let pages = font_pages(
        PageSize::custom(Size::try_pt(120.0, 400.0).unwrap()),
        sec(vec![runs_para(vec![
            text_run("A"),
            blank_run(BlankWidth::ToEndOfLine, None, None, None),
        ])]),
    )
    .unwrap()
    .pages();
    let a = glyph(&pages[0], "A");
    let stroke = &stroke_samples(&pages[0])[0];
    assert!(pt_near(stroke.x0, a.x + a.width));
    assert!(pt_near(stroke.x1, 120.0));
    assert!(pt_near(stroke.y0, a.y + 1.0));
    assert!(pt_near(stroke.width, 1.0));
    assert_eq!(stroke.color, Color::rgb(0, 0, 0));
}

#[test]
fn blank_em_is_font_size_times_count_and_needs_no_bold_face() {
    if skip_without_font() {
        return;
    }
    let pages = font_pages(
        PageSize::custom(Size::try_pt(200.0, 400.0).unwrap()),
        sec(vec![runs_para(vec![blank_run(
            BlankWidth::Em(2),
            None,
            None,
            None,
        )])]),
    )
    .unwrap()
    .pages();
    let stroke = &stroke_samples(&pages[0])[0];
    assert!(
        pt_near(stroke.x1 - stroke.x0, 24.0),
        "{}",
        stroke.x1 - stroke.x0
    );
    assert!(
        runs(&pages[0]).is_empty(),
        "a blank-only paragraph does not emit a glyph run"
    );
}

#[test]
fn zero_width_blank_draws_no_stroke_and_does_not_shift() {
    if skip_without_font() {
        return;
    }
    let pages = font_pages(
        PageSize::custom(Size::try_pt(200.0, 400.0).unwrap()),
        sec(vec![runs_para(vec![
            text_run("A"),
            blank_run(BlankWidth::Fixed(NonNegative::ZERO), None, None, None),
            blank_run(BlankWidth::Em(0), None, None, None),
            text_run("B"),
        ])]),
    )
    .unwrap()
    .pages();
    assert!(stroke_samples(&pages[0]).is_empty());
    let a = glyph(&pages[0], "A");
    let b = glyph(&pages[0], "B");
    assert!(pt_near(b.x, a.x + a.width));
}

#[test]
fn list_fill_blank_starts_after_the_marker_column() {
    if skip_without_font() {
        return;
    }
    let pages = font_pages(
        PageSize::custom(Size::try_pt(200.0, 400.0).unwrap()),
        sec(vec![bullet_list(vec![plain_item(vec![runs_para(vec![
            text_run("A"),
            blank_run(BlankWidth::ToEndOfLine, None, None, None),
        ])])])]),
    )
    .unwrap()
    .pages();
    let a = glyph(&pages[0], "A");
    assert!(pt_near(a.x, 18.0), "{}", a.x);
    let stroke = &stroke_samples(&pages[0])[0];
    assert!(pt_near(stroke.x0, a.x + a.width));
    assert!(pt_near(stroke.x1, 200.0));
}

#[test]
fn column_fill_blank_is_shifted_with_its_column() {
    if skip_without_font() {
        return;
    }
    let pages = font_pages(
        PageSize::custom(Size::try_pt(200.0, 400.0).unwrap()),
        sec(vec![gap_columns(
            12.0,
            vec![
                ratio_col(1.0, vec![para("L")]),
                ratio_col(
                    1.0,
                    vec![runs_para(vec![
                        text_run("A"),
                        blank_run(BlankWidth::ToEndOfLine, None, None, None),
                    ])],
                ),
            ],
        )]),
    )
    .unwrap()
    .pages();
    let a = glyph(&pages[0], "A");
    let col = (200.0 - 12.0) / 2.0;
    assert!(pt_near(a.x, col + 12.0), "{}", a.x);
    let stroke = &stroke_samples(&pages[0])[0];
    assert!(pt_near(stroke.x0, a.x + a.width));
    assert!(pt_near(stroke.x1, 200.0), "{}", stroke.x1);
}

#[test]
fn blank_stays_on_its_line_when_the_paragraph_splits() {
    if skip_without_font() {
        return;
    }
    let pages = font_pages(
        PageSize::custom(Size::try_pt(200.0, line()).unwrap()),
        sec(vec![runs_para(vec![
            text_run("A"),
            blank_run(BlankWidth::Fixed(nn(20.0)), None, None, None),
            text_run("\nB"),
        ])]),
    )
    .unwrap()
    .pages();
    assert_eq!(pages.len(), 2);
    assert_eq!(stroke_samples(&pages[0]).len(), 1);
    assert!(stroke_samples(&pages[1]).is_empty());
    let b = glyph(&pages[1], "B");
    assert!(pt_near(b.x, 0.0));
    assert!(pt_near(b.y, baseline()));
}

#[cfg(feature = "backend-krilla")]
#[test]
fn inline_blank_writes_a_pdf() {
    if skip_without_font() {
        return;
    }
    let rendered = font_pages(
        PageSize::A4,
        sec(vec![runs_para(vec![
            text_run("A"),
            blank_run(BlankWidth::Fixed(nn(12.0)), None, None, None),
        ])]),
    )
    .unwrap();
    assert!(to_pdf(rendered).unwrap().starts_with(b"%PDF"));
}

#[test]
fn partial_link_rect_matches_the_linked_glyphs() {
    if skip_without_font() {
        return;
    }
    let pages = font_pages(
        PageSize::A4,
        sec(vec![runs_para(vec![
            text_run("A"),
            link_run("https://example.com/bc", vec![text_run("BC")]),
            text_run("D"),
        ])]),
    )
    .unwrap()
    .pages();
    let page = &pages[0];
    let bc = glyph(page, "BC");
    let hits = links(page);
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].target, "https://example.com/bc");
    assert!(pt_near(hits[0].x, bc.x));
    assert!(pt_near(hits[0].width, bc.width));
    assert!(pt_near(hits[0].y, 0.0));
    assert!(pt_near(hits[0].height, line()));
    assert!(pt_near(bc.x, glyph(page, "A").width));
}

#[test]
fn wrap_emits_one_link_rect_per_visual_line() {
    if skip_without_font() {
        return;
    }
    // One em-width of 'A' so "A A" wraps to two visual lines.
    let width = common::advance_pt(&font_bytes(), 'A', 12.0);
    let pages = font_pages(
        PageSize::custom(Size::try_pt(width, 2.0 * line() + 1.0).unwrap()),
        sec(vec![runs_para(vec![link_run(
            "https://example.com/wrap",
            vec![text_run("A A")],
        )])]),
    )
    .unwrap()
    .pages();
    let hits = links(&pages[0]);
    let glyphs = runs(&pages[0]);
    assert_eq!(pages.len(), 1);
    assert_eq!(
        hits.len(),
        2,
        "{:?}",
        glyphs.iter().map(|run| &run.text).collect::<Vec<_>>()
    );
    assert_eq!(glyphs.len(), 2);
    assert!(hits
        .iter()
        .all(|hit| hit.target == "https://example.com/wrap"));
    for (hit, run) in hits.iter().zip(glyphs.iter()) {
        assert!(pt_near(hit.x, run.x));
        assert!(pt_near(hit.width, run.width));
        assert!(pt_near(hit.height, line()));
    }
    assert!(pt_near(hits[0].y, 0.0));
    assert!(pt_near(hits[1].y, line()));
}

#[test]
fn body_and_bold_share_one_link_rect() {
    if skip_without_font() {
        return;
    }
    let pages = bold_pages(
        PageSize::A4,
        sec(vec![runs_para(vec![link_run(
            "https://example.com/mix",
            vec![text_run("A"), bold_run("B"), text_run("C")],
        )])]),
    )
    .unwrap()
    .pages();
    let page = &pages[0];
    let glyphs = runs(page);
    assert_eq!(
        glyphs
            .iter()
            .map(|run| run.text.as_str())
            .collect::<Vec<_>>(),
        ["A", "B", "C"]
    );
    let hits = links(page);
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].target, "https://example.com/mix");
    assert!(pt_near(hits[0].x, glyphs[0].x));
    assert!(pt_near(
        hits[0].width,
        glyphs.iter().map(|run| run.width).sum()
    ));
    assert!(pt_near(hits[0].y, 0.0));
    assert!(pt_near(hits[0].height, line()));
}

#[test]
fn uri_boundaries_stay_distinct() {
    if skip_without_font() {
        return;
    }
    let pages = font_pages(
        PageSize::A4,
        sec(vec![runs_para(vec![
            link_run("https://a.example", vec![text_run("A")]),
            link_run("https://b.example", vec![text_run("B")]),
            text_run("C"),
            link_run("https://a.example", vec![text_run("D")]),
        ])]),
    )
    .unwrap()
    .pages();
    let page = &pages[0];
    let hits = links(page);
    assert_eq!(
        hits.iter()
            .map(|hit| hit.target.as_str())
            .collect::<Vec<_>>(),
        [
            "https://a.example",
            "https://b.example",
            "https://a.example"
        ]
    );
    for (hit, text) in hits.iter().zip(["A", "B", "D"]) {
        let run = glyph(page, text);
        assert!(pt_near(hit.x, run.x));
        assert!(pt_near(hit.width, run.width));
    }
    assert!(hits[0].x + hits[0].width <= hits[1].x + f32::EPSILON);
    assert!(hits[1].x + hits[1].width < hits[2].x);
}

#[test]
fn repeated_same_uri_merges_across_calls_and_faces() {
    if skip_without_font() {
        return;
    }
    let pages = bold_pages(
        PageSize::A4,
        sec(vec![
            runs_para(vec![
                link_run("https://example.com/u", vec![text_run("A")]),
                link_run("https://example.com/u", vec![text_run("B")]),
            ]),
            runs_para(vec![link_run(
                "https://example.com/u",
                vec![text_run("C"), bold_run("D")],
            )]),
        ]),
    )
    .unwrap()
    .pages();
    let page = &pages[0];
    let hits = links(page);
    assert_eq!(hits.len(), 2);
    assert!(hits.iter().all(|hit| hit.target == "https://example.com/u"));
    let ab = glyph(page, "AB");
    assert!(pt_near(hits[0].x, ab.x));
    assert!(pt_near(hits[0].width, ab.width));
    assert!(pt_near(hits[0].y, 0.0));
    let c = glyph(page, "C");
    let d = glyph(page, "D");
    assert!(pt_near(hits[1].x, c.x));
    assert!(pt_near(hits[1].width, c.width + d.width));
    assert!(pt_near(hits[1].y, line()));
    assert!(pt_near(hits[1].height, line()));
}

#[test]
fn blank_inside_a_link_is_not_part_of_either_rect() {
    if skip_without_font() {
        return;
    }
    let pages = font_pages(
        PageSize::A4,
        sec(vec![runs_para(vec![link_run(
            "https://example.com/gap",
            vec![
                text_run("A"),
                blank_run(BlankWidth::Fixed(nn(30.0)), None, None, None),
                text_run("B"),
            ],
        )])]),
    )
    .unwrap()
    .pages();
    let page = &pages[0];
    let hits = links(page);
    assert_eq!(hits.len(), 2);
    assert_eq!(stroke_samples(page).len(), 1);
    assert!(pt_near(hits[1].x - (hits[0].x + hits[0].width), 30.0));
    assert!(pt_near(hits[0].x, glyph(page, "A").x));
    assert!(pt_near(hits[1].x, glyph(page, "B").x));
    assert_eq!(hits[0].target, "https://example.com/gap");
    assert_eq!(hits[1].target, hits[0].target);
}

#[test]
fn link_rect_includes_indent_column_and_list_offset() {
    if skip_without_font() {
        return;
    }
    let indented = font_pages(
        PageSize::A4,
        sec(vec![paragraph(
            vec![link_run("https://example.com/indent", vec![text_run("A")])],
            10.0,
            Alignment::Start,
            None,
            None,
        )]),
    )
    .unwrap()
    .pages();
    let indent_hit = &links(&indented[0])[0];
    assert!(pt_near(indent_hit.x, 10.0));
    assert!(pt_near(indent_hit.x, glyph(&indented[0], "A").x));

    let columns = font_pages(
        PageSize::A4,
        sec(vec![gap_columns(
            12.0,
            vec![
                ratio_col(1.0, vec![para("L")]),
                ratio_col(
                    1.0,
                    vec![runs_para(vec![link_run(
                        "https://example.com/col",
                        vec![text_run("R")],
                    )])],
                ),
            ],
        )]),
    )
    .unwrap()
    .pages();
    let right = glyph(&columns[0], "R");
    let col_hit = &links(&columns[0])[0];
    assert!(right.x > 0.0);
    assert!(pt_near(col_hit.x, right.x));
    assert!(pt_near(col_hit.width, right.width));
    assert_eq!(col_hit.target, "https://example.com/col");

    let listed = font_pages(
        PageSize::A4,
        sec(vec![bullet_list(vec![plain_item(vec![runs_para(vec![
            link_run("https://example.com/item", vec![text_run("A")]),
        ])])])]),
    )
    .unwrap()
    .pages();
    let item = glyph(&listed[0], "A");
    let list_hit = &links(&listed[0])[0];
    assert!(pt_near(item.x, 18.0));
    assert!(pt_near(list_hit.x, item.x));
    assert!(pt_near(list_hit.width, item.width));
    assert_eq!(links(&listed[0]).len(), 1);
}

#[test]
fn link_across_pages_restarts_y_at_the_body_top() {
    if skip_without_font() {
        return;
    }
    let top = 20.0;
    let pages = font_pages(
        PageSize::custom(Size::try_pt(200.0, top + line()).unwrap()),
        SectionSpec {
            margins: Margins::new(
                nn(top),
                NonNegative::ZERO,
                NonNegative::ZERO,
                NonNegative::ZERO,
            ),
            body: vec![runs_para(vec![link_run(
                "https://example.com/page",
                vec![text_run("A\nA")],
            )])],
            ..SectionSpec::default()
        },
    )
    .unwrap()
    .pages();
    assert_eq!(pages.len(), 2);
    for page in &pages {
        let hits = links(page);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].target, "https://example.com/page");
        assert!(pt_near(hits[0].y, top));
        assert!(pt_near(hits[0].height, line()));
        assert!(pt_near(glyph(page, "A").y, top + baseline()));
    }
}

#[test]
fn header_and_footer_slots_annotate_at_the_band_origin() {
    if skip_without_font() {
        return;
    }
    let margin = 36.0;
    let pages = font_pages(
        PageSize::A4,
        SectionSpec {
            margins: Margins::uniform(nn(margin)),
            header: Some(BandSpec {
                left: Some(slot_link(
                    vec![TextRun::Literal("https://example.com/left".into())],
                    "Left",
                )),
                right: Some(slot_link(
                    vec![TextRun::Literal("https://example.com/right".into())],
                    "Right",
                )),
                ..BandSpec::default()
            }),
            footer: Some(BandSpec {
                left: Some(slot_link(
                    vec![TextRun::Literal("https://example.com/foot".into())],
                    "Foot",
                )),
                ..BandSpec::default()
            }),
            body: vec![para("Body")],
            ..SectionSpec::default()
        },
    )
    .unwrap()
    .pages();
    let page = &pages[0];
    let hits = links(page);
    assert_eq!(hits.len(), 3);
    let left = glyph(page, "Left");
    let right = glyph(page, "Right");
    let foot = glyph(page, "Foot");
    let left_hit = hits
        .iter()
        .find(|hit| hit.target == "https://example.com/left")
        .unwrap();
    let right_hit = hits
        .iter()
        .find(|hit| hit.target == "https://example.com/right")
        .unwrap();
    let foot_hit = hits
        .iter()
        .find(|hit| hit.target == "https://example.com/foot")
        .unwrap();
    assert!(pt_near(left.x, 0.0));
    assert!(pt_near(left_hit.x, left.x));
    assert!(pt_near(left_hit.width, left.width));
    assert!(pt_near(left_hit.y, 0.0));
    assert!(pt_near(left_hit.height, line()));
    assert!(pt_near(right.x + right.width, A4.width().as_pt()));
    assert!(pt_near(right_hit.x, right.x));
    assert!(pt_near(right_hit.width, right.width));
    assert!(pt_near(right_hit.y, 0.0));
    assert!(pt_near(foot.x, 0.0));
    assert!(pt_near(foot.y, A4.height().as_pt() - margin + baseline()));
    assert!(pt_near(foot_hit.x, foot.x));
    assert!(pt_near(foot_hit.y, A4.height().as_pt() - margin));
    assert!(pt_near(foot_hit.height, line()));
}

#[test]
fn header_uri_can_differ_per_page() {
    if skip_without_font() {
        return;
    }
    let pages = font_pages(
        PageSize::A4,
        SectionSpec {
            margins: Margins::uniform(nn(36.0)),
            header: Some(BandSpec {
                left: Some(BandSlot::Link {
                    font: (),
                    uri: vec![
                        TextRun::Literal("https://example.com/".into()),
                        TextRun::PageNumber,
                    ],
                    text: vec![TextRun::Literal("Home".into())],
                    style: RunStyle::default(),
                }),
                ..BandSpec::default()
            }),
            body: vec![para("A"), Block::PageBreak, para("B")],
            ..SectionSpec::default()
        },
    )
    .unwrap()
    .pages();
    assert_eq!(pages.len(), 2);
    assert_eq!(links(&pages[0]).len(), 1);
    assert_eq!(links(&pages[1]).len(), 1);
    assert_eq!(links(&pages[0])[0].target, "https://example.com/1");
    assert_eq!(links(&pages[1])[0].target, "https://example.com/2");
}

#[test]
fn empty_uri_fails_the_document() {
    if skip_without_font() {
        return;
    }
    let body = font_pages(
        PageSize::A4,
        sec(vec![runs_para(vec![link_run("", vec![text_run("x")])])]),
    );
    assert!(matches!(
        body.map_err(|error| error.first().kind.clone()),
        Err(ErrorKind::EmptyLink)
    ));

    let band = font_pages(
        PageSize::A4,
        SectionSpec {
            margins: Margins::uniform(nn(36.0)),
            header: Some(BandSpec {
                right: Some(slot_link(vec![TextRun::Literal(String::new())], "Home")),
                ..BandSpec::default()
            }),
            body: vec![para("A")],
            ..SectionSpec::default()
        },
    );
    assert!(matches!(
        band.map_err(|error| error.first().kind.clone()),
        Err(ErrorKind::EmptyLink)
    ));
}

#[test]
fn empty_linked_text_draws_no_annotation() {
    if skip_without_font() {
        return;
    }
    let pages = font_pages(
        PageSize::A4,
        sec(vec![runs_para(vec![
            link_run("https://example.com", vec![text_run("")]),
            text_run("A"),
        ])]),
    )
    .unwrap()
    .pages();
    assert!(links(&pages[0]).is_empty());
    assert_eq!(texts(&pages[0]), ["A"]);
}

#[test]
fn link_does_not_change_glyphs_or_add_a_stroke() {
    if skip_without_font() {
        return;
    }
    let paint = |linked: bool| {
        let runs = if linked {
            vec![link_run("https://example.com/same", vec![text_run("ABC")])]
        } else {
            vec![text_run("ABC")]
        };
        font_pages(PageSize::A4, sec(vec![runs_para(runs)]))
            .unwrap()
            .pages()
    };
    let linked = paint(true);
    let plain = paint(false);
    assert_eq!(commands_without_links(&linked[0]), plain[0].ops().to_vec());
    assert_eq!(links(&linked[0]).len(), 1);
    assert!(links(&plain[0]).is_empty());
    let run = glyph(&linked[0], "ABC");
    assert_eq!(run.font, glyph(&plain[0], "ABC").font);
    assert_eq!(run.text, "ABC");
}

#[test]
fn uri_text_is_stored_as_given() {
    if skip_without_font() {
        return;
    }
    let pages = font_pages(
        PageSize::A4,
        sec(vec![runs_para(vec![
            link_run(" ", vec![text_run("A")]),
            link_run("mailto:a@b.c", vec![text_run("B")]),
            link_run("not a uri", vec![text_run("C")]),
        ])]),
    )
    .unwrap()
    .pages();
    let hits = links(&pages[0]);
    assert_eq!(
        hits.iter()
            .map(|hit| hit.target.as_str())
            .collect::<Vec<_>>(),
        [" ", "mailto:a@b.c", "not a uri"]
    );
}

#[test]
fn cell_and_layer_text_do_not_annotate() {
    if skip_without_font() {
        return;
    }
    let pages = font_pages(
        PageSize::A4,
        SectionSpec {
            body: vec![
                grid(vec![], vec![text_row(&["cell"])]),
                runs_para(vec![link_run(
                    "https://example.com/only",
                    vec![text_run("Go")],
                )]),
            ],
            behind: vec![mark_text(
                PageWhen::All,
                "mark",
                10.0,
                10.0,
                RunStyle::default(),
            )],
            ..SectionSpec::default()
        },
    )
    .unwrap()
    .pages();
    let hits = links(&pages[0]);
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].target, "https://example.com/only");
    assert_eq!(texts(&pages[0]), ["mark", "cell", "Go"]);
}

#[cfg(feature = "backend-krilla")]
#[test]
fn text_link_writes_a_pdf() {
    if skip_without_font() {
        return;
    }
    let rendered = font_pages(
        PageSize::A4,
        sec(vec![runs_para(vec![link_run(
            "https://example.com",
            vec![text_run("Go")],
        )])]),
    )
    .unwrap();
    assert!(to_pdf(rendered).unwrap().starts_with(b"%PDF"));
}

#[test]
fn cell_paragraph_pins_bold_blank_and_link() {
    if skip_without_font() {
        return;
    }
    let pages = bold_pages(
        PageSize::custom(Size::try_pt(200.0, 400.0).unwrap()),
        sec(vec![grid(
            vec![],
            vec![vec![
                cell(vec![CellBlock::Paragraph(ParagraphSpec {
                    runs: vec![
                        text_run("A"),
                        bold_run("B"),
                        blank_run(BlankWidth::ToEndOfLine, None, None, None),
                    ],
                    ..ParagraphSpec::default()
                })]),
                cell(vec![CellBlock::Paragraph(ParagraphSpec {
                    runs: vec![link_run("https://example.test/a", vec![text_run("Go")])],
                    ..ParagraphSpec::default()
                })]),
            ]],
        )]),
    )
    .unwrap()
    .pages();
    let page = &pages[0];
    let a = glyph(page, "A");
    let b = glyph(page, "B");
    assert_eq!((a.text.as_str(), a.font), ("A", 0));
    assert_eq!((b.text.as_str(), b.font), ("B", 1));
    let stroke = &stroke_samples(page)[0];
    assert!(pt_near(stroke.x0, b.x + b.width), "{}", stroke.x0);
    assert!(pt_near(stroke.x1, 100.0), "{}", stroke.x1);
    let hits = links(page);
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].target, "https://example.test/a");
    assert!(pt_near(hits[0].x, 100.0), "{}", hits[0].x);
    assert!(pt_near(hits[0].y, 0.0));
    assert!(pt_near(hits[0].height, line()));
}

#[test]
fn cell_paragraph_errors_record_no_pages() {
    if skip_without_font() {
        return;
    }
    let empty_link = font_pages(
        PageSize::A4,
        sec(vec![grid(
            vec![],
            vec![vec![cell(vec![CellBlock::Paragraph(ParagraphSpec {
                runs: vec![link_run("", vec![text_run("x")])],
                ..ParagraphSpec::default()
            })])]],
        )]),
    );
    assert_eq!(
        empty_link.err().map(|error| error.first().kind.clone()),
        Some(ErrorKind::EmptyLink)
    );

    let after_fill = font_pages(
        PageSize::A4,
        sec(vec![grid(
            vec![],
            vec![vec![cell(vec![CellBlock::Paragraph(ParagraphSpec {
                runs: vec![
                    blank_run(BlankWidth::ToEndOfLine, None, None, None),
                    text_run("x"),
                ],
                ..ParagraphSpec::default()
            })])]],
        )]),
    );
    assert_eq!(
        after_fill.err().map(|error| error.first().kind.clone()),
        Some(ErrorKind::RunAfterFillBlank)
    );
}

#[test]
fn cell_paragraph_indent_uses_cell_width() {
    if skip_without_font() {
        return;
    }
    let error = font_pages(
        PageSize::custom(Size::try_pt(40.0, 400.0).unwrap()),
        sec(vec![grid(
            vec![],
            vec![vec![
                cell(vec![CellBlock::Paragraph(ParagraphSpec {
                    runs: vec![text_run("A")],
                    first_line_indent: nn(24.0),
                    ..ParagraphSpec::default()
                })]),
                text_cell("B"),
            ]],
        )]),
    )
    .err()
    .unwrap();
    assert_eq!(
        error.first().kind.clone(),
        ErrorKind::InvalidGeometry(GeometryKind::FirstLineIndent {
            indent: 24.0,
            width: 20.0,
        })
    );
    assert_eq!(
        location(error.first()),
        (Some(0), Some(1), Some(Region::Body), Some((0, 0)))
    );
}

#[test]
fn cell_rich_taller_than_frame_does_not_continue() {
    if skip_without_font() {
        return;
    }
    let page = PageSize::custom(Size::try_pt(200.0, line()).unwrap());
    let paragraph = font_pages(
        page,
        sec(vec![grid(
            vec![],
            vec![vec![cell(vec![CellBlock::Paragraph(ParagraphSpec::text(
                (),
                "A\nB",
            ))])]],
        )]),
    );
    let error = paragraph.err().unwrap();
    assert!(
        matches!(
            error.first().kind.clone(),
            ErrorKind::UnsplittableTallerThanFrame { height, avail }
                if pt_near(height, line() * 2.0) && pt_near(avail, line())
        ),
        "{error:?}"
    );
    assert_eq!(
        location(error.first()),
        (Some(0), Some(1), Some(Region::Body), Some((0, 0)))
    );

    let list = font_pages(
        page,
        sec(vec![grid(
            vec![],
            vec![vec![cell(vec![CellBlock::List(ListSpec {
                items: vec![plain_item(vec![para("A")]), plain_item(vec![para("B")])],
                ..ListSpec::default()
            })])]],
        )]),
    );
    let error = list.err().unwrap();
    assert!(
        matches!(
            error.first().kind.clone(),
            ErrorKind::UnsplittableTallerThanFrame { height, avail }
                if pt_near(height, line() * 2.0) && pt_near(avail, line())
        ),
        "{error:?}"
    );
}

#[test]
fn cell_list_can_borrow_a_table_local() {
    if skip_without_font() {
        return;
    }
    let label = String::from("borrowed");
    let pages = font_pages(
        PageSize::A4,
        sec(vec![grid(
            vec![],
            vec![vec![cell(vec![CellBlock::List(ListSpec {
                items: vec![plain_item(vec![para(label)])],
                ..ListSpec::default()
            })])]],
        )]),
    )
    .unwrap()
    .pages();
    assert!(texts(&pages[0]).iter().any(|s| s == "borrowed"));
}

fn cell_page(content: Vec<CellBlock<()>>) -> RecordedPage {
    let pages = font_pages(
        PageSize::custom(Size::try_pt(200.0, 200.0).unwrap()),
        sec(vec![grid(vec![], vec![vec![cell(content)]])]),
    )
    .unwrap()
    .pages();
    assert_eq!(pages.len(), 1);
    pages.into_iter().next().unwrap()
}

#[test]
fn cell_add_matches_sugar() {
    if skip_without_font() {
        return;
    }
    let page = cell_page(vec![CellBlock::Paragraph(ParagraphSpec {
        runs: vec![text_run("Fee 12")],
        font_size: Some(pos(9.0)),
        ..ParagraphSpec::default()
    })]);
    let drawn = page
        .ops()
        .iter()
        .find_map(|op| match op {
            CanvasCommand::DrawGlyphRun { run } => Some(run),
            _ => None,
        })
        .unwrap();
    assert_eq!(drawn.text, "Fee 12");
    assert_eq!(drawn.font_size.as_pt().to_bits(), 9.0f32.to_bits());

    let page = cell_page(vec![CellBlock::HLine(HLineSpec {
        color: Color::rgb(9, 9, 9),
        ..HLineSpec::default()
    })]);
    assert_eq!(stroke_samples(&page)[0].color, Color::rgb(9, 9, 9));

    let page = cell_page(vec![CellBlock::Spacer(nn(12.0))]);
    assert!(page.ops().is_empty());

    let page = cell_page(vec![CellBlock::List(ListSpec {
        items: vec![plain_item(vec![para("a")])],
        ..ListSpec::default()
    })]);
    assert!(texts(&page).iter().any(|text| text == "a"));

    let page = cell_page(vec![CellBlock::List(ListSpec {
        kind: ListKind::Ordered(Numbering::UpperRoman),
        start: NonZeroU32::new(4).unwrap(),
        items: vec![plain_item(vec![para("a")])],
        ..ListSpec::default()
    })]);
    assert!(texts(&page).iter().any(|text| text == "IV."));
}

#[cfg(feature = "backend-krilla")]
#[test]
fn cell_add_image_matches_sugar() {
    let mut resources = Resources::default();
    let image = resources.add_image(ONE_PIXEL.to_vec(), ImageFormat::Png);
    let pages = render(
        PageSize::custom(Size::try_pt(200.0, 200.0).unwrap()),
        resources,
        vec![sec(vec![grid(
            vec![],
            vec![vec![cell(vec![CellBlock::Image(ImageSpec {
                image,
                width: Some(pos(10.0)),
                height: None,
                align: Alignment::Start,
            })])]],
        )])],
    )
    .unwrap()
    .pages();
    let images = image_rects(&pages[0]);
    assert_eq!(images.len(), 1);
    assert_eq!((images[0].1, images[0].2), (1, 1));
    assert_eq!(
        rect_bits(images[0].3),
        rect_bits(Rect::try_pt(0.0, 0.0, 10.0, 10.0).unwrap())
    );
}

#[cfg(feature = "backend-krilla")]
#[test]
fn table_build_cell_image_paints_when_added() {
    let mut resources = Resources::default();
    let image = resources.add_image(ONE_PIXEL.to_vec(), ImageFormat::Png);
    let rendered = render(
        PageSize::custom(Size::try_pt(200.0, 80.0).unwrap()),
        resources,
        vec![sec(vec![grid(
            vec![],
            vec![vec![cell(vec![CellBlock::Image(ImageSpec {
                image,
                width: Some(pos(10.0)),
                height: None,
                align: Alignment::Start,
            })])]],
        )])],
    )
    .unwrap();
    let images = image_rects(&rendered.pages()[0]);
    assert_eq!(images.len(), 1);
    assert_eq!((images[0].1, images[0].2), (1, 1));
    assert_eq!(
        rect_bits(images[0].3),
        rect_bits(Rect::try_pt(0.0, 0.0, 10.0, 10.0).unwrap())
    );
}

#[test]
fn cell_children_stack_in_add_order() {
    if skip_without_font() {
        return;
    }
    let page = cell_page(vec![
        CellBlock::Paragraph(ParagraphSpec::text((), "A")),
        CellBlock::HLine(HLineSpec::default()),
    ]);
    let strokes = stroke_samples(&page);
    assert_eq!(strokes.len(), 1);
    assert!(pt_near(glyph(&page, "A").y, baseline()));
    assert!(pt_near(strokes[0].y0, line() + 0.5));
}

#[test]
fn empty_cell_is_a_zero_height_slot() {
    if skip_without_font() {
        return;
    }
    let pages = font_pages(
        PageSize::A4,
        sec(vec![grid(
            vec![],
            vec![vec![CellSpec::default()], text_row(&["B"])],
        )]),
    )
    .unwrap()
    .pages();
    assert!(pt_near(glyph(&pages[0], "B").y, baseline()));
}

#[test]
fn cell_list_marker_at_cell_x() {
    if skip_without_font() {
        return;
    }
    let pages = font_pages(
        PageSize::custom(Size::try_pt(200.0, 400.0).unwrap()),
        sec(vec![grid(
            vec![],
            vec![
                vec![
                    text_cell("L"),
                    cell(vec![CellBlock::List(ListSpec {
                        items: vec![
                            plain_item(vec![para("dot")]),
                            plain_item(vec![bullet_list(vec![plain_item(vec![para("child")])])]),
                        ],
                        ..ListSpec::default()
                    })]),
                ],
                vec![
                    text_cell("M"),
                    cell(vec![CellBlock::List(ListSpec {
                        kind: ListKind::Ordered(Numbering::Decimal),
                        items: vec![
                            plain_item(vec![para("one")]),
                            marked_item("(c)", vec![para("custom")]),
                        ],
                        ..ListSpec::default()
                    })]),
                ],
            ],
        )]),
    )
    .unwrap()
    .pages();
    let page = &pages[0];
    assert_eq!(
        texts(page),
        [
            "L", "\u{2022}", "dot", "\u{2022}", "\u{2022}", "child", "M", "1.", "one", "(c)",
            "custom",
        ]
    );
    let bullet_x: Vec<f32> = runs(page)
        .into_iter()
        .filter(|run| run.text == "\u{2022}")
        .map(|run| run.x)
        .collect();
    assert!(pt_near(bullet_x[0], 100.0), "{}", bullet_x[0]);
    assert!(pt_near(bullet_x[1], 100.0), "{}", bullet_x[1]);
    assert!(pt_near(bullet_x[2], 118.0), "{}", bullet_x[2]);
    assert!(pt_near(glyph(page, "dot").x, 118.0));
    assert!(pt_near(glyph(page, "child").x, 136.0));
    assert!(pt_near(glyph(page, "1.").x, 100.0));
    assert!(pt_near(glyph(page, "one").x, 118.0));
    assert!(pt_near(glyph(page, "(c)").x, 100.0));
    assert!(pt_near(glyph(page, "custom").x, 118.0));
}

#[test]
fn cell_list_item_rejects_forbidden_kids() {
    if skip_without_font() {
        return;
    }
    let reject = |node: &'static str, block: Block<()>| {
        let result = font_pages(
            PageSize::A4,
            sec(vec![grid(
                vec![],
                vec![vec![cell(vec![CellBlock::List(ListSpec {
                    items: vec![plain_item(vec![block])],
                    ..ListSpec::default()
                })])]],
            )]),
        );
        assert_eq!(
            result.err().map(|error| error.first().kind.clone()),
            Some(ErrorKind::InvalidCellContent {
                container: "list item",
                node,
            })
        );
    };
    reject("table", grid(vec![], vec![text_row(&["a"])]));
    reject(
        "columns",
        gap_columns(
            12.0,
            vec![
                ratio_col(1.0, vec![para("a")]),
                ratio_col(1.0, vec![para("b")]),
            ],
        ),
    );
    reject("page break", Block::PageBreak);
    reject("keep together", Block::KeepTogether(vec![]));
}

#[cfg(feature = "backend-krilla")]
#[test]
fn cell_list_item_rejects_an_image() {
    if skip_without_font() {
        return;
    }
    let mut resources = font_resources();
    let logo = resources.add_image(ONE_PIXEL.to_vec(), ImageFormat::Png);
    let result = render(
        PageSize::A4,
        resources,
        vec![sec(vec![grid(
            vec![],
            vec![vec![cell(vec![CellBlock::List(ListSpec {
                items: vec![plain_item(vec![image_block(logo, None, Alignment::Start)])],
                ..ListSpec::default()
            })])]],
        )])],
    );
    assert_eq!(
        result.err().map(|error| error.first().kind.clone()),
        Some(ErrorKind::InvalidCellContent {
            container: "list item",
            node: "image",
        })
    );
}

#[test]
fn cell_list_narrower_than_marker_is_invalid_geometry() {
    if skip_without_font() {
        return;
    }
    let error = font_pages(
        PageSize::custom(Size::try_pt(18.0, 400.0).unwrap()),
        sec(vec![grid(
            vec![],
            vec![vec![cell(vec![CellBlock::List(ListSpec {
                items: vec![plain_item(vec![para("A")])],
                ..ListSpec::default()
            })])]],
        )]),
    )
    .err()
    .unwrap();
    assert_eq!(
        error.first().kind.clone(),
        ErrorKind::InvalidGeometry(GeometryKind::ListIndentWiderThanFrame {
            indent: 18.0,
            avail: 18.0,
        })
    );
    assert_eq!(
        location(error.first()),
        (Some(0), Some(1), Some(Region::Body), Some((0, 0)))
    );
}

#[test]
fn margin_presets_match_uniform_insets() {
    if skip_without_font() {
        return;
    }
    let origin = |margins: Margins| {
        let pages = font_pages(
            PageSize::A4,
            SectionSpec {
                margins,
                body: vec![para("Body")],
                ..SectionSpec::default()
            },
        )
        .unwrap()
        .pages();
        let body = glyph(&pages[0], "Body");
        (body.x, body.y)
    };
    let inch = origin(MarginPreset::Inch.into());
    let inch_uniform = origin(Margins::uniform(nn(72.0)));
    assert!(pt_near(inch.0, 72.0));
    assert!(pt_near(inch.1, 72.0 + baseline()));
    assert!(pt_near(inch.0, inch_uniform.0));
    assert!(pt_near(inch.1, inch_uniform.1));
    let half = origin(MarginPreset::HalfInch.into());
    let half_uniform = origin(Margins::uniform(nn(36.0)));
    assert!(pt_near(half.0, 36.0));
    assert!(pt_near(half.1, 36.0 + baseline()));
    assert!(pt_near(half.0, half_uniform.0));
    assert!(pt_near(half.1, half_uniform.1));
}

#[test]
fn table_tracks_fill_the_frame_and_keep_fixed_width() {
    if skip_without_font() {
        return;
    }
    let pages = font_pages(
        PageSize::custom(Size::try_pt(200.0, 400.0).unwrap()),
        sec(vec![Block::Table(TableSpec {
            tracks: Some(vec![Track::Fixed(pos(40.0)), Track::Auto]),
            body: vec![text_row(&["A", "B"])],
            ..TableSpec::default()
        })]),
    )
    .unwrap()
    .pages();
    assert!(pt_near(glyph(&pages[0], "A").x, 0.0));
    assert!(pt_near(glyph(&pages[0], "B").x, 40.0));
}

#[test]
fn table_padding_wraps_cell_text_earlier_than_the_open_grid() {
    if skip_without_font() {
        return;
    }
    let width = common::advance_pt(&font_bytes(), 'A', 12.0);
    let page = PageSize::custom(Size::try_pt(width * 4.0, 400.0).unwrap());
    let open = font_pages(
        page,
        sec(vec![grid(
            vec![],
            vec![vec![cell(vec![CellBlock::Paragraph(ParagraphSpec::text(
                (),
                "A A",
            ))])]],
        )]),
    )
    .unwrap()
    .pages();
    let padded = font_pages(
        page,
        sec(vec![Block::Table(TableSpec {
            cell_padding: nn(width),
            body: vec![vec![cell(vec![CellBlock::Paragraph(ParagraphSpec::text(
                (),
                "A A",
            ))])]],
            ..TableSpec::default()
        })]),
    )
    .unwrap()
    .pages();
    assert_eq!(runs(&open[0]).len(), 1);
    assert_eq!(runs(&padded[0]).len(), 2);
    assert!(pt_near(runs(&padded[0])[1].y, line() + width + baseline()));
}

#[test]
fn table_zero_border_draws_no_stroke_and_a_positive_border_does() {
    if skip_without_font() {
        return;
    }
    let page = PageSize::custom(Size::try_pt(200.0, 400.0).unwrap());
    let none = font_pages(page, sec(vec![grid(vec![], vec![text_row(&["A"])])]))
        .unwrap()
        .pages();
    assert!(none[0]
        .ops()
        .iter()
        .all(|op| !matches!(op, CanvasCommand::StrokePath { .. })));
    let stroked = font_pages(
        page,
        sec(vec![Block::Table(TableSpec {
            border_width: nn(0.5),
            body: vec![text_row(&["A"])],
            ..TableSpec::default()
        })]),
    )
    .unwrap()
    .pages();
    assert!(stroked[0]
        .ops()
        .iter()
        .any(|op| matches!(op, CanvasCommand::StrokePath { .. })));
}

#[test]
fn columns_fixed_plus_auto_gives_the_remainder_to_auto() {
    if skip_without_font() {
        return;
    }
    let pages = font_pages(
        PageSize::custom(Size::try_pt(200.0, 400.0).unwrap()),
        sec(vec![Block::Columns(ColumnsSpec {
            gap: nn(10.0),
            columns: vec![fixed_col(50.0, vec![para("L")]), auto_col(vec![para("R")])],
        })]),
    )
    .unwrap()
    .pages();
    assert!(pt_near(glyph(&pages[0], "L").x, 0.0));
    assert!(pt_near(glyph(&pages[0], "R").x, 60.0));
}

#[test]
fn columns_gap_zero_places_the_second_column_against_the_first() {
    if skip_without_font() {
        return;
    }
    let pages = font_pages(
        PageSize::custom(Size::try_pt(200.0, 400.0).unwrap()),
        sec(vec![Block::Columns(ColumnsSpec {
            gap: NonNegative::ZERO,
            columns: vec![fixed_col(80.0, vec![para("L")]), auto_col(vec![para("R")])],
        })]),
    )
    .unwrap()
    .pages();
    assert!(pt_near(glyph(&pages[0], "R").x, 80.0));
}

#[test]
fn ratio_rejects_a_weight_that_is_not_positive_and_finite() {
    if skip_without_font() {
        return;
    }
    assert_eq!(
        Ratio::new(0.0).unwrap_err().kind,
        ErrorKind::ColumnWidths {
            reason: ColumnWidthsReason::NonPositiveRatio,
        }
    );
    assert_eq!(
        Ratio::new(-1.0).unwrap_err().kind,
        ErrorKind::ColumnWidths {
            reason: ColumnWidthsReason::NonPositiveRatio,
        }
    );
    assert_eq!(
        Ratio::new(f32::NAN).unwrap_err().kind,
        ErrorKind::ColumnWidths {
            reason: ColumnWidthsReason::NonFiniteRatio,
        }
    );
    let pages = font_pages(
        PageSize::custom(Size::try_pt(300.0, 200.0).unwrap()),
        sec(vec![Block::Table(TableSpec {
            tracks: Some(vec![
                Track::Ratio(Ratio::new(1.0).unwrap()),
                Track::Ratio(Ratio::new(2.0).unwrap()),
            ]),
            body: vec![text_row(&["A", "B"])],
            ..TableSpec::default()
        })]),
    )
    .unwrap()
    .pages();
    assert!(pt_near(glyph(&pages[0], "B").x, 100.0));
}

#[test]
fn auto_with_ratio_fails_on_columns_and_table_tracks() {
    if skip_without_font() {
        return;
    }
    let columns = font_pages(
        PageSize::A4,
        sec(vec![Block::Columns(ColumnsSpec {
            gap: nn(12.0),
            columns: vec![auto_col(vec![para("A")]), ratio_col(1.0, vec![para("B")])],
        })]),
    );
    assert_eq!(
        columns.err().map(|error| error.first().kind.clone()),
        Some(ErrorKind::ColumnWidths {
            reason: ColumnWidthsReason::AutoWithRatio
        })
    );
    let table = font_pages(
        PageSize::A4,
        sec(vec![Block::Table(TableSpec {
            tracks: Some(vec![Track::Auto, Track::Ratio(Ratio::new(1.0).unwrap())]),
            body: vec![text_row(&["A", "B"])],
            ..TableSpec::default()
        })]),
    );
    assert_eq!(
        table.err().map(|error| error.first().kind.clone()),
        Some(ErrorKind::ColumnWidths {
            reason: ColumnWidthsReason::AutoWithRatio
        })
    );
}

#[test]
fn table_tracks_length_must_match_the_first_row() {
    if skip_without_font() {
        return;
    }
    let result = font_pages(
        PageSize::A4,
        sec(vec![Block::Table(TableSpec {
            tracks: Some(vec![Track::Ratio(Ratio::new(1.0).unwrap())]),
            body: vec![text_row(&["A", "B"])],
            ..TableSpec::default()
        })]),
    );
    assert_eq!(
        result.err().map(|error| error.first().kind.clone()),
        Some(ErrorKind::ColumnWidths {
            reason: ColumnWidthsReason::ColumnCount
        })
    );
}

#[test]
fn paragraph_center_and_end_align_each_visual_line() {
    if skip_without_font() {
        return;
    }
    let page_w = 120.0;
    let pages = font_pages(
        PageSize::custom(Size::try_pt(page_w, 400.0).unwrap()),
        sec(vec![
            para("A"),
            paragraph(vec![text_run("B")], 0.0, Alignment::Center, None, None),
            paragraph(vec![text_run("C")], 0.0, Alignment::End, None, None),
            paragraph(vec![text_run("D")], 0.0, Alignment::End, None, None),
        ]),
    )
    .unwrap()
    .pages();
    let a = glyph(&pages[0], "A");
    let b = glyph(&pages[0], "B");
    let c = glyph(&pages[0], "C");
    let d = glyph(&pages[0], "D");
    assert!(pt_near(a.x, 0.0));
    assert!(pt_near(b.x, (page_w - b.width) / 2.0));
    assert!(pt_near(c.x + c.width, page_w));
    assert!(pt_near(d.x + d.width, page_w));
}

fn canvas_label() -> Block<()> {
    let label = "送貨";
    Block::Canvas(CanvasSpec {
        size: Size::try_pt(120.0, 80.0).unwrap(),
        align: Alignment::Start,
        marks: vec![
            CanvasMark::Rect {
                rect: Rect::try_pt(0.0, 0.0, 120.0, 40.0).unwrap(),
                ink: Ink::Stroke(Pen::new(Color::rgb(170, 170, 170), pos(1.0)).unwrap()),
            },
            CanvasMark::Line {
                from: Point::try_pt(10.0, 8.0).unwrap(),
                to: Point::try_pt(10.0, 36.0).unwrap(),
                pen: Pen::new(Color::rgb(170, 170, 170), pos(1.0)).unwrap(),
            },
            CanvasMark::Fill {
                path: Path::begin(Point::try_pt(8.0, 30.0).unwrap())
                    .line_to(Point::try_pt(16.0, 38.0).unwrap())
                    .line_to(Point::try_pt(24.0, 30.0).unwrap())
                    .close(),
                color: Color::rgb(170, 170, 170),
            },
            CanvasMark::Text {
                text: label.into(),
                font: (),
                at: Point::try_pt(12.0, 40.0).unwrap(),
                style: RunStyle {
                    font_size: Some(pos(11.0)),
                    ..RunStyle::default()
                },
            },
        ],
    })
}

fn pdftotext_of(pdf: &[u8]) -> String {
    let path = std::env::temp_dir().join(format!("pdfcraft-canvas-{}.pdf", std::process::id()));
    std::fs::write(&path, pdf).unwrap();
    let output = std::process::Command::new("pdftotext")
        .arg(&path)
        .arg("-")
        .output()
        .expect("pdftotext");
    let _ = std::fs::remove_file(&path);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap()
}

#[test]
fn canvas_round_trip_paints_the_label() {
    if skip_without_font() {
        return;
    }
    let rendered = font_pages(
        PageSize::custom(Size::try_pt(200.0, 200.0).unwrap()),
        sec(vec![canvas_label()]),
    )
    .unwrap();
    let drawn = glyph(&rendered.pages()[0], "送貨");
    assert!(pt_near(drawn.x, 12.0));
    assert!(pt_near(drawn.y, 40.0));
    let pdf = to_pdf(rendered).unwrap();
    assert!(pdf.starts_with(b"%PDF"));
    assert!(pdftotext_of(&pdf).contains("送貨"));
}

#[test]
fn canvas_moves_whole_when_remainder_is_short() {
    let pages = render(
        PageSize::custom(Size::try_pt(100.0, 40.0).unwrap()),
        Resources::default(),
        vec![sec(vec![
            Block::Spacer(nn(30.0)),
            Block::Canvas(CanvasSpec {
                size: Size::try_pt(40.0, 20.0).unwrap(),
                align: Alignment::Start,
                marks: vec![CanvasMark::Rect {
                    rect: Rect::try_pt(0.0, 0.0, 40.0, 20.0).unwrap(),
                    ink: Ink::Fill(Color::rgb(1, 2, 3)),
                }],
            }),
        ])],
    )
    .unwrap()
    .pages();
    assert_eq!(pages.len(), 2);
    assert!(pages[0]
        .ops()
        .iter()
        .all(|op| !matches!(op, CanvasCommand::FillPath { .. })));
    assert!(matches!(
        pages[1].ops().first(),
        Some(CanvasCommand::FillPath { .. })
    ));
}

#[test]
fn canvas_taller_than_the_body_fails() {
    let error = render(
        PageSize::custom(Size::try_pt(100.0, 40.0).unwrap()),
        Resources::default(),
        vec![sec(vec![Block::Canvas(CanvasSpec {
            size: Size::try_pt(10.0, 50.0).unwrap(),
            align: Alignment::Start,
            marks: Vec::new(),
        })])],
    )
    .err()
    .unwrap();
    assert_eq!(
        error.first().kind.clone(),
        ErrorKind::UnsplittableTallerThanFrame {
            height: 50.0,
            avail: 40.0,
        }
    );
}

#[test]
fn canvas_wider_than_the_column_fails() {
    let error = render(
        PageSize::custom(Size::try_pt(100.0, 40.0).unwrap()),
        Resources::default(),
        vec![sec(vec![Block::Canvas(CanvasSpec {
            size: Size::try_pt(150.0, 10.0).unwrap(),
            align: Alignment::Start,
            marks: Vec::new(),
        })])],
    )
    .err()
    .unwrap();
    assert_eq!(
        error.first().kind.clone(),
        ErrorKind::UnbreakableWiderThanFrame {
            text: None,
            width: 150.0,
            avail: 100.0,
        }
    );
}

#[test]
fn text_width_matches_the_laid_out_advance() {
    if skip_without_font() {
        return;
    }
    let bytes = font_bytes();
    let face = FontFace::new(bytes.clone(), 0);
    let page = font_pages(
        PageSize::custom(Size::try_pt(200.0, 80.0).unwrap()),
        sec(vec![para("Hi")]),
    )
    .unwrap()
    .pages()
    .into_iter()
    .next()
    .unwrap();
    let drawn = glyph(&page, "Hi");
    assert_eq!(
        face.text_width("Hi", pos(12.0)).unwrap().to_bits(),
        drawn.width.to_bits()
    );
    assert!(matches!(
        face.text_width("\u{E000}", pos(12.0)).unwrap_err().kind,
        ErrorKind::MissingGlyph {
            scalar: '\u{E000}',
            ..
        }
    ));
    assert_eq!(
        face.line_height(pos(11.0)).unwrap().to_bits(),
        common::line_height_at(&bytes, 11.0, 1.0).to_bits()
    );
}

#[test]
fn empty_canvas_text_fails_layout() {
    if skip_without_font() {
        return;
    }
    let error = font_pages(
        PageSize::custom(Size::try_pt(100.0, 100.0).unwrap()),
        sec(vec![Block::Canvas(CanvasSpec {
            size: Size::try_pt(40.0, 20.0).unwrap(),
            align: Alignment::Start,
            marks: vec![CanvasMark::Text {
                text: String::new(),
                font: (),
                at: Point::try_pt(0.0, 10.0).unwrap(),
                style: RunStyle::default(),
            }],
        })]),
    )
    .err()
    .unwrap();
    assert_eq!(error.first().kind.clone(), ErrorKind::EmptyCanvasText);
}

#[test]
fn inch_preset_on_a_narrow_page_is_invalid_geometry() {
    if skip_without_font() {
        return;
    }
    let error = font_pages(
        PageSize::custom(Size::try_pt(100.0, 200.0).unwrap()),
        SectionSpec {
            margins: MarginPreset::Inch.into(),
            body: vec![para("A")],
            ..SectionSpec::default()
        },
    )
    .err()
    .unwrap();
    assert_eq!(
        error.first().kind.clone(),
        ErrorKind::InvalidGeometry(GeometryKind::BodyFrame {
            width: -44.0,
            height: 56.0,
        })
    );
    assert_eq!(location(error.first()), (Some(0), None, None, None));
}
