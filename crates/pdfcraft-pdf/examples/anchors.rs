//! Visual sample for §12.4 anchors, in-document links, and the outline tree.
//!
//! ```text
//! cargo run -p pdfcraft-pdf --example anchors
//! ```
//!
//! Writes `out/anchors.pdf` (gitignored). Pass another path as argv[1].
//!
//! Font lookup matches `hello`, then falls back to `.fonts/body.ttf` (same face
//! `bill` uses). The face must cover the CJK in this sample.

use std::env;
use std::fs;
use std::num::NonZeroU16;
use std::path::{Path, PathBuf};

use pdfcraft_pdf::spec::{
    BandInset, BandSlot, BandSpec, Block, CellBlock, CellSpec, FontFace, FontFamily, FontRegistry,
    InlineRun, OutlineEntry, ParagraphSpec, Resources, RunStyle, SectionSpec, TableSpec, TextRun,
};
use pdfcraft_pdf::{
    layout_spec, to_pdf, Color, DocumentSpec, Length, Limits, Margins, NonNegative, PageSize,
    Positive, Ratio, Result, Track,
};

const LINK_BLUE: Color = Color::rgb(0x1a, 0x56, 0xdb);
const HEADER_FILL: Color = Color::rgb(0xe8, 0xee, 0xf8);
const BODY_ROWS: usize = 36;

fn main() -> Result<()> {
    let out = env::args()
        .nth(1)
        .map_or_else(|| PathBuf::from("out/anchors.pdf"), PathBuf::from);
    if let Some(parent) = out.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent).expect("create output dir");
        }
    }

    let font = load_font()?;
    let rendered = layout_spec(DocumentSpec {
        page_size: PageSize::A4,
        resources: Resources::with_fonts(FontRegistry::single(FontFamily {
            regular: FontFace::new(font, 0),
            bold: None,
            italic: None,
            bold_italic: None,
        })),
        limits: Limits::default(),
        info: pdfcraft_pdf::spec::DocumentInfo {
            title: Some("pdfcraft-pdf section 12.4 anchors".into()),
            subject: Some("anchors / in-document links / outline".into()),
            ..pdfcraft_pdf::spec::DocumentInfo::default()
        },
        sections: vec![SectionSpec {
            margins: Margins::try_uniform(48.0)?,
            header: Some(page_chrome()),
            body: sample_body()?,
            ..SectionSpec::default()
        }],
    })?;
    let pages = rendered.pages().len();
    let outline_len = rendered.outline_len();
    let pdf = to_pdf(rendered)?;
    fs::write(&out, &pdf).expect("write PDF");
    eprintln!(
        "wrote {} ({} bytes, {pages} pages)",
        out.display(),
        pdf.len()
    );
    eprintln!("outline entries: {outline_len} (no outline dictionary when this is 0)");
    print_how_to_look(&out);
    Ok(())
}

fn print_how_to_look(out: &Path) {
    let path = out.display();
    eprintln!(
        "
如何看这份样张
  运行：cargo run -p pdfcraft-pdf --example anchors
        可选：cargo run -p pdfcraft-pdf --example anchors -- {path}
  字体：PDFCRAFT_PDF_FONT，否则 PDFCRAFT_CJK_FONT，否则仓库根 .fonts/body.ttf
        默认 .fonts/body.ttf 是 Source Han Serif HK，正文用繁体才能覆盖。

  pdfinfo {path}
      看页数（应多于 1 页）和页面尺寸（A4）。pdfinfo 不列出书签。
  pdftotext -layout {path} -
      正文里应能读到「開篇」「付款」「簽章」，以及重复出现的表头文字。

  用带书签侧栏和链接注释的阅读器打开（Firefox、Evince、Okular、Acrobat）。
  侧栏默认折叠：先看到与「開篇」同级的「簽章」；「付款」缩在「開篇」下面，展开后才出现。
  Chrome / Edge 内置查看器经常不画书签，链接点了也可能没有手型。

书签（点一项跳到该标题行左上，缩放不变）
  開篇    层级 1    第 1 页正文顶（页边距内）
    付款  层级 2    「付款」那一页的标题行左上（分页后的页顶附近）
  簽章    层级 1    「簽章」那一页的标题行左上；从 2 上跳回 1，与「開篇」同级

正文里的蓝下划线
  「打開 example.test/pay」  外链 URI，应离开文档打开 https://example.test/pay
  「見付款」                  文内链，落到「付款」标题左上，不是标题下面的说明
  「見表頭」                  文内链，落到第一页表头格左上
                              后面页上重复画出的同一行表头不是目标
  页眉「頁首外鏈…」            BandSlot::Link，只有 URI，没有文内锚点

已知限制
  书签是否默认折叠取决于阅读器是否遵守 outline 的负 Count。
  XYZ 的 zoom 为空：阅读器保持当前缩放，不会改成「适合页面」。
  调用方的锚点 id 不写进 PDF 名称树，侧栏里看不到 id，只能看到书签标题。
"
    );
}

fn page_chrome() -> BandSpec<()> {
    BandSpec {
        inset: BandInset::PageMargins,
        left: Some(BandSlot::Link {
            font: (),
            uri: vec![TextRun::Literal("https://example.test/pay".into())],
            text: vec![TextRun::Literal("頁首外鏈 example.test/pay".into())],
            style: RunStyle::default(),
        }),
        right: Some(BandSlot::Text {
            runs: vec![
                TextRun::Literal("第 ".into()),
                TextRun::PageNumber,
                TextRun::Literal(" / ".into()),
                TextRun::PageCount,
                TextRun::Literal(" 頁".into()),
            ],
            font: (),
            style: RunStyle::default(),
        }),
    }
}

fn sample_body() -> Result<Vec<Block<()>>> {
    let body = vec![
        anchor("intro", "開篇", 1, heading("開篇（書籤層級 1）")?),
        para(
            "側欄先點「開篇」應落在上一行標題的左上。子項「付款」預設收合，展開「開篇」才能看見。",
        )?,
        link_line(
            "外鏈 URI：",
            pdfcraft_pdf::LinkTarget::uri("https://example.test/pay")?,
            "打開 example.test/pay",
        )?,
        link_line(
            "文內鏈：",
            pdfcraft_pdf::LinkTarget::anchor("pay")?,
            "見付款",
        )?,
        link_line(
            "重複表頭：",
            pdfcraft_pdf::LinkTarget::anchor("table-head")?,
            "見表頭",
        )?,
        para(
            "下面的表從本頁畫起。表頭格子帶錨點 table-head。翻頁後表頭會再畫一遍，目標仍是第一次。",
        )?,
        sample_table()?,
        Block::PageBreak,
        anchor("pay", "付款", 2, heading("付款（書籤層級 2）")?),
        para("點「見付款」應落在上一行標題的左上，而不是這段說明。")?,
        para("付款條款寫在標題下面，用來把落點和後文分開。金額、賬戶、期限都只是佔位文字。")?,
        Block::PageBreak,
        anchor("sign", "簽章", 1, heading("簽章（書籤層級 1，從 2 上跳）")?),
        para("側欄裡「簽章」與「開篇」同級，不縮在「付款」下面。")?,
        link_line(
            "再跳一次表頭：",
            pdfcraft_pdf::LinkTarget::anchor("table-head")?,
            "見表頭",
        )?,
    ];
    Ok(body)
}

fn sample_table() -> Result<Block<()>> {
    let header = vec![cell(
        vec![CellBlock::Anchor {
            id: "table-head".into(),
            outline: None,
            child: Box::new(CellBlock::Text {
                text: "表頭錨點（只登記第一次）".into(),
                font: (),
            }),
        }],
        Some(HEADER_FILL),
    )];
    let mut rows = Vec::with_capacity(BODY_ROWS);
    for index in 1..=BODY_ROWS {
        rows.push(vec![cell(
            vec![CellBlock::Text {
                text: format!("第 {index} 行。跨頁後的表頭不是連結目標。"),
                font: (),
            }],
            None,
        )]);
    }
    Ok(Block::Table(TableSpec {
        tracks: Some(vec![Track::Ratio(Ratio::new(1.0).expect("weight"))]),
        header: vec![header],
        body: rows,
        repeat_header: true,
        cell_padding: nn(4.0)?,
        border_width: nn(0.5)?,
        border_color: Color::rgb(0x99, 0x99, 0x99),
    }))
}

fn anchor(id: &str, title: &str, level: u8, child: Block<()>) -> Block<()> {
    Block::Anchor {
        id: id.into(),
        outline: Some(OutlineEntry {
            title: title.into(),
            level,
        }),
        child: Box::new(child),
    }
}

fn heading(text: &str) -> Result<Block<()>> {
    Ok(Block::Styled {
        style: RunStyle {
            font_size: Some(pos(18.0)?),
            ..RunStyle::default()
        },
        children: vec![Block::paragraph((), text)],
    })
}

fn para(text: &str) -> Result<Block<()>> {
    Ok(Block::Paragraph(ParagraphSpec {
        runs: vec![text_run(text)],
        font_size: Some(pos(11.0)?),
        ..ParagraphSpec::default()
    }))
}

fn link_line(label: &str, target: pdfcraft_pdf::LinkTarget, link_text: &str) -> Result<Block<()>> {
    Ok(Block::Paragraph(ParagraphSpec {
        runs: vec![
            text_run(label),
            InlineRun::Link {
                target,
                runs: vec![InlineRun::Text {
                    text: link_text.into(),
                    font: (),
                    style: RunStyle {
                        color: Some(LINK_BLUE),
                        underline: Some(true),
                        ..RunStyle::default()
                    },
                }],
            },
        ],
        font_size: Some(pos(11.0)?),
        ..ParagraphSpec::default()
    }))
}

fn text_run(text: &str) -> InlineRun<()> {
    InlineRun::Text {
        text: text.into(),
        font: (),
        style: RunStyle::default(),
    }
}

fn cell(content: Vec<CellBlock<()>>, fill: Option<Color>) -> CellSpec<()> {
    CellSpec {
        content,
        col_span: NonZeroU16::MIN,
        row_span: NonZeroU16::MIN,
        fill,
    }
}

fn pos(value: f32) -> Result<Length<Positive>> {
    Positive::pt(value).map_err(pdfcraft_pdf::Errors::from)
}

fn nn(value: f32) -> Result<Length<NonNegative>> {
    NonNegative::pt(value).map_err(pdfcraft_pdf::Errors::from)
}

fn load_font() -> Result<Vec<u8>> {
    if let Some(path) =
        env::var_os("PDFCRAFT_PDF_FONT").or_else(|| env::var_os("PDFCRAFT_CJK_FONT"))
    {
        let bytes = fs::read(&path).map_err(|error| {
            pdfcraft_pdf::Errors::from(font_error(format!(
                "read {}: {error}",
                PathBuf::from(&path).display()
            )))
        })?;
        eprintln!(
            "font: {} ({} bytes)",
            PathBuf::from(&path).display(),
            bytes.len()
        );
        return Ok(bytes);
    }
    let fallback = Path::new(".fonts/body.ttf");
    if fallback.is_file() {
        let bytes = fs::read(fallback).map_err(|error| {
            pdfcraft_pdf::Errors::from(font_error(format!("read {}: {error}", fallback.display())))
        })?;
        eprintln!("font: {} ({} bytes)", fallback.display(), bytes.len());
        return Ok(bytes);
    }
    Err(pdfcraft_pdf::Errors::from(font_error(
        "set PDFCRAFT_PDF_FONT or PDFCRAFT_CJK_FONT, or place a face at .fonts/body.ttf".into(),
    )))
}

fn font_error(message: String) -> pdfcraft_pdf::Error {
    pdfcraft_pdf::Error::from(pdfcraft_pdf::ErrorKind::Font { index: 0, message })
}
