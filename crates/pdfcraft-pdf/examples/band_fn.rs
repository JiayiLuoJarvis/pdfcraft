//! Per-page header and footer samples.
//!
//! ```text
//! BAND_FN_CASE=first cargo run -p pdfcraft-pdf --example band_fn
//! ```
//!
//! `BAND_FN_CASE` selects `first`, `last`, `odd-even`, `odd-even-marks`,
//! `skip`, `count`, `sections`, `link`, `too-tall`, `fail`, or
//! `footer-skip`. The default is `first`. Writes `out/band_fn.pdf`. `fail`
//! and `too-tall` return the layout error.
//!
//! Font lookup matches `hello`.

#![expect(clippy::print_stderr, reason = "example reports the written PDF path")]

use std::env;
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;

use pdfcraft_pdf::spec::{
    BandFn, BandInset, BandMark, BandSlot, BandSpec, Block, FontFace, FontFamily, FontRegistry,
    ImageRef, LayerShape, Resources, RunStyle, SectionSpec, TextRun,
};
use pdfcraft_pdf::{
    layout_spec, to_pdf, Color, DocumentSpec, Error, ImageFormat, Ink, Limits, Margins, PageSize,
    Paper, Pen, Point, Positive, Rect, Result,
};

const MARK: &[u8] = include_bytes!("assets/mark.png");
const DIAMOND: &[u8] = include_bytes!("assets/diamond.png");

fn load_font() -> std::result::Result<Vec<u8>, Box<dyn std::error::Error>> {
    let path = env::var_os("PDFCRAFT_PDF_FONT")
        .or_else(|| env::var_os("PDFCRAFT_CJK_FONT"))
        .ok_or("set PDFCRAFT_PDF_FONT or PDFCRAFT_CJK_FONT to a .ttf/.otf path")?;
    let bytes = fs::read(&path)
        .map_err(|error| format!("read font {}: {error}", PathBuf::from(&path).display()))?;
    Ok(bytes)
}

fn text_band(text: impl Into<String>, style: RunStyle) -> BandSpec<()> {
    BandSpec {
        inset: BandInset::PageMargins,
        left: Some(BandSlot::Text {
            runs: vec![TextRun::Literal(text.into())],
            font: (),
            style,
        }),
        right: None,
        marks: Vec::new(),
    }
}

fn literal_band(text: impl Into<String>) -> BandSpec<()> {
    text_band(text, RunStyle::default())
}

fn demo_body() -> Vec<Block<()>> {
    vec![
        Block::paragraph((), "one"),
        Block::PageBreak,
        Block::paragraph((), "two"),
        Block::PageBreak,
        Block::paragraph((), "three"),
    ]
}

fn section(
    margins: Margins,
    header: Option<BandFn<()>>,
    footer: Option<BandFn<()>>,
    body: Vec<Block<()>>,
) -> SectionSpec<()> {
    SectionSpec {
        margins,
        header,
        footer,
        body,
        ..SectionSpec::default()
    }
}

fn size_header() -> BandFn<()> {
    BandFn::from_fn(|query| {
        let label = format!(
            "{} x {}",
            query.size.width().as_pt(),
            query.size.height().as_pt()
        );
        Ok(Some(literal_band(label)))
    })
}

fn when_page(
    label: &'static str,
    keep: impl Fn(&pdfcraft_pdf::PageBandQuery) -> bool + Send + Sync + 'static,
) -> BandFn<()> {
    BandFn::from_fn(move |query| {
        if keep(query) {
            Ok(Some(literal_band(label)))
        } else {
            Ok(None)
        }
    })
}

fn link_band() -> BandSpec<()> {
    BandSpec {
        inset: BandInset::PageMargins,
        left: Some(BandSlot::Link {
            font: (),
            uri: vec![TextRun::Literal("https://example.test/band".into())],
            text: vec![TextRun::Literal("band link".into())],
            style: RunStyle::default(),
        }),
        right: None,
        marks: Vec::new(),
    }
}

fn tall_band() -> Result<BandSpec<()>> {
    Ok(text_band(
        "T",
        RunStyle {
            font_size: Some(Positive::pt(48.0).map_err(pdfcraft_pdf::Errors::from)?),
            ..RunStyle::default()
        },
    ))
}

fn odd_even_marks(odd_image: ImageRef, even_image: ImageRef) -> BandFn<()> {
    BandFn::from_fn(move |query| {
        let marks = if query.number.get() % 2 == 1 {
            vec![BandMark::Shape(LayerShape::Image {
                image: odd_image,
                rect: Rect::try_pt(270.0, 12.5, 55.0, 55.0)?,
            })]
        } else {
            vec![
                BandMark::Shape(LayerShape::Rect {
                    rect: Rect::try_pt(36.0, 7.5, 180.0, 65.0)?,
                    ink: Ink::Stroke(Pen::new(Color::rgb(153, 153, 153), 1.0)?),
                }),
                BandMark::Shape(LayerShape::Text {
                    text: vec![TextRun::Literal("Anti-counterfeit".into())],
                    font: (),
                    at: Point::try_pt(44.0, 15.0)?,
                    style: RunStyle {
                        font_size: Some(Positive::pt(9.0)?),
                        ..RunStyle::default()
                    },
                }),
                BandMark::Shape(LayerShape::Text {
                    text: vec![TextRun::Literal("Scan to verify".into())],
                    font: (),
                    at: Point::try_pt(44.0, 31.0)?,
                    style: RunStyle {
                        font_size: Some(Positive::pt(9.0)?),
                        ..RunStyle::default()
                    },
                }),
                BandMark::Shape(LayerShape::Text {
                    text: vec![TextRun::Literal("No. ".into()), TextRun::PageNumber],
                    font: (),
                    at: Point::try_pt(44.0, 49.0)?,
                    style: RunStyle {
                        font_size: Some(Positive::pt(8.0)?),
                        color: Some(Color::rgb(128, 128, 128)),
                        ..RunStyle::default()
                    },
                }),
                BandMark::Shape(LayerShape::Image {
                    image: even_image,
                    rect: Rect::try_pt(156.0, 12.5, 55.0, 55.0)?,
                }),
            ]
        };
        Ok(Some(BandSpec {
            marks,
            ..BandSpec::default()
        }))
    })
}

fn sections_for(
    case: &str,
    odd_image: ImageRef,
    even_image: ImageRef,
) -> Result<Vec<SectionSpec<()>>> {
    let margins = Margins::try_uniform(36.0)?;
    let pages = |header, footer| section(margins, header, footer, demo_body());
    let sections = match case {
        "first" => vec![pages(
            Some(when_page("first", |query| query.number.get() == 1)),
            None,
        )],
        "last" => vec![pages(
            Some(when_page("last", |query| query.number == query.count)),
            None,
        )],
        "odd-even" => vec![pages(
            Some(BandFn::from_fn(|query| {
                let label = if query.number.get() % 2 == 1 {
                    "odd"
                } else {
                    "even"
                };
                Ok(Some(literal_band(label)))
            })),
            None,
        )],
        "odd-even-marks" => vec![section(
            Margins::try_new(36.0, 36.0, 80.0, 36.0)?,
            None,
            Some(odd_even_marks(odd_image, even_image)),
            demo_body(),
        )],
        "skip" => vec![pages(
            Some(when_page("kept", |query| query.number.get() != 2)),
            None,
        )],
        "count" => vec![pages(
            Some(BandFn::from_fn(|query| {
                Ok(Some(literal_band(format!(
                    "{} / {}",
                    query.number, query.count
                ))))
            })),
            None,
        )],
        "sections" => vec![
            section(
                margins,
                Some(size_header()),
                None,
                vec![Block::paragraph((), "portrait")],
            ),
            SectionSpec {
                page_size: Some(Paper::A4.landscape()),
                margins,
                header: Some(size_header()),
                body: vec![Block::paragraph((), "landscape")],
                ..SectionSpec::default()
            },
        ],
        "link" => vec![pages(
            Some(BandFn::from_fn(|query| {
                if query.number.get() == 2 {
                    Ok(Some(link_band()))
                } else {
                    Ok(None)
                }
            })),
            None,
        )],
        "too-tall" => vec![section(
            Margins::try_new(8.0, 36.0, 36.0, 36.0)?,
            Some(BandFn::fixed(tall_band()?)),
            None,
            vec![Block::paragraph((), "body")],
        )],
        "fail" => vec![pages(
            Some(BandFn::from_fn(|_| Err(Error::band_fn("boom")))),
            None,
        )],
        "footer-skip" => vec![pages(
            Some(BandFn::fixed(literal_band("header"))),
            Some(when_page("footer", |query| query.number.get() != 2)),
        )],
        _ => {
            return Err(pdfcraft_pdf::Errors::from(Error::band_fn(
                "BAND_FN_CASE must be first, last, odd-even, odd-even-marks, skip, count, sections, link, too-tall, fail, or footer-skip",
            )));
        }
    };
    Ok(sections)
}

fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    let case = env::var("BAND_FN_CASE").unwrap_or_else(|_| "first".into());
    let out = env::args()
        .nth(1)
        .map_or_else(|| PathBuf::from("out/band_fn.pdf"), PathBuf::from);
    if let Some(parent) = out.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)?;
        }
    }

    let font = Arc::new(load_font()?);
    let mut resources = Resources::with_fonts(FontRegistry::single(FontFamily {
        regular: FontFace::from_shared(Arc::clone(&font), 0),
        bold: Some(FontFace::from_shared(font, 0)),
        italic: None,
        bold_italic: None,
    }));
    let odd_image = resources.add_image(MARK.to_vec(), ImageFormat::Png);
    let even_image = resources.add_image(DIAMOND.to_vec(), ImageFormat::Png);
    let sections = sections_for(&case, odd_image, even_image)?;
    let rendered = layout_spec(DocumentSpec {
        page_size: PageSize::A4,
        resources,
        limits: Limits::default(),
        info: pdfcraft_pdf::spec::DocumentInfo::default(),
        sections,
    });
    let rendered = rendered?;
    let pdf = to_pdf(rendered)?;
    fs::write(&out, &pdf)?;
    eprintln!("wrote {} ({} bytes)", out.display(), pdf.len());
    Ok(())
}
