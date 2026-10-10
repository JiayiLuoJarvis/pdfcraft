//! Small PDF smoke demo. Needs a font with the glyphs you print.
//!
//! ```text
//! cargo run --example hello
//! ```
//!
//! Writes `hello.pdf` in the current directory. Pass another path as argv[1].
//! The bundled fixture only maps a Latin subset. A missing glyph fails the document.

use std::env;
use std::fs;
use std::path::PathBuf;

use quire_pdf::{document, Margins, NonNegative, Numbering, PageSize};

const FONT: &[u8] = include_bytes!("../tests/fixtures/quire-measure.ttf");

fn main() -> quire_pdf::Result<()> {
    let out = env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("hello.pdf"));

    let pdf = document(PageSize::A4, |doc| {
        doc.font(FONT.to_vec())?;
        doc.margins(Margins::uniform(NonNegative::pt(36.0)?));
        doc.header(|h| {
            let m = h.page().margins;
            h.inset(m.left(), m.right());
            h.left("quire hello");
            h.right(format!("page {}", h.page().number));
        });
        doc.paragraph(|p| {
            p.center();
            p.text("Hello from quire");
        });
        doc.paragraph(|p| p.text("Body text uses the first registered font at 12 pt."));
        doc.ol(Numbering::Decimal, |list| {
            list.item(|item| item.paragraph(|p| p.text("measure")));
            list.item(|item| item.paragraph(|p| p.text("break lines")));
            list.item(|item| item.paragraph(|p| p.text("write PDF")));
        });
        Ok(())
    })?
    .to_pdf()?;

    fs::write(&out, &pdf).expect("write PDF");
    eprintln!("wrote {} ({} bytes)", out.display(), pdf.len());
    Ok(())
}
