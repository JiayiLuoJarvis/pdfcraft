//! Behavior checks for the data-only document specification path.

use quire_pdf::spec::{
    BandInset, BandSlot, Block, LayerMark, PageWhen, Resources, SectionSpec, TextRun, TextStyle,
};
#[cfg(feature = "backend-krilla")]
use quire_pdf::to_pdf;
use quire_pdf::{
    layout_spec, pt_near, CanvasCommand, DocumentSpec, Margins, NonNegative, PageSize, Point,
};

const FONT: &[u8] = include_bytes!("fixtures/quire-measure.ttf");
const BASELINE: f32 = 13.812;

fn texts(page: &quire_pdf::RecordedPage) -> Vec<&str> {
    page.ops()
        .iter()
        .filter_map(|command| match command {
            CanvasCommand::DrawGlyphRun { run } => Some(run.text.as_str()),
            _ => None,
        })
        .collect()
}

fn links(page: &quire_pdf::RecordedPage) -> Vec<&str> {
    page.ops()
        .iter()
        .filter_map(|command| match command {
            CanvasCommand::AnnotateLink { target, .. } => Some(target.as_str()),
            _ => None,
        })
        .collect()
}

#[test]
fn two_page_spec_resolves_page_text_links_and_last_page_marks() {
    let margin = NonNegative::pt(36.0).unwrap();
    let mut section = SectionSpec {
        margins: Margins::uniform(margin),
        ..SectionSpec::default()
    };
    section.header = Some(quire_pdf::spec::BandSpec {
        inset: BandInset::PageMargins,
        left: Some(BandSlot::Link {
            uri: vec![
                TextRun::Literal("https://example.com/".to_owned()),
                TextRun::PageNumber,
            ],
            text: vec![TextRun::Literal("Home".to_owned())],
        }),
        right: Some(BandSlot::Text(vec![
            TextRun::Literal("第 ".to_owned()),
            TextRun::PageNumber,
            TextRun::Literal(" 页 / 共 ".to_owned()),
            TextRun::PageCount,
            TextRun::Literal(" 页".to_owned()),
        ])),
    });
    section.body = vec![
        Block::paragraph("Alpha"),
        Block::PageBreak,
        Block::paragraph("Beta"),
    ];
    section.front.push(LayerMark::Text {
        when: PageWhen::Last,
        text: vec![TextRun::Literal("SEAL".to_owned())],
        at: Point::try_pt(430.0, 680.0).unwrap(),
        style: TextStyle::default(),
    });

    let spec = DocumentSpec {
        page_size: PageSize::A4,
        resources: Resources::new(FONT.to_vec()),
        limits: quire_pdf::Limits::default(),
        sections: vec![section],
    };
    let rendered = layout_spec(spec).unwrap();
    let pages = rendered.pages();
    assert_eq!(pages.len(), 2);
    assert_eq!(links(&pages[0]), ["https://example.com/1"]);
    assert_eq!(links(&pages[1]), ["https://example.com/2"]);
    assert!(texts(&pages[0]).contains(&"Alpha"));
    assert!(!texts(&pages[0]).contains(&"Beta"));
    assert!(!texts(&pages[0]).contains(&"SEAL"));
    assert!(texts(&pages[1]).contains(&"Beta"));
    assert!(texts(&pages[1]).contains(&"SEAL"));

    let seal = pages[1]
        .ops()
        .iter()
        .find_map(|command| match command {
            CanvasCommand::DrawGlyphRun { run } if run.text == "SEAL" => Some(run),
            _ => None,
        })
        .unwrap();
    assert!(pt_near(seal.origin.x().as_pt(), 430.0));
    assert!(pt_near(seal.origin.y().as_pt(), 680.0 + BASELINE));

    #[cfg(feature = "backend-krilla")]
    {
        let pdf = to_pdf(rendered).unwrap();
        assert!(pdf.starts_with(b"%PDF"));
        std::fs::write("target/spec-parity.pdf", pdf).unwrap();
    }
}
