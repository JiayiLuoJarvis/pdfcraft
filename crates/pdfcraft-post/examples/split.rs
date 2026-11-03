//! Split a PDF into one single-page PDF per page.

#![expect(
    clippy::print_stdout,
    reason = "example reports the written page files"
)]

use std::env;
use std::fs;
use std::path::Path;

use pdfcraft_post::split_pdfs;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = env::args().skip(1);
    let input = args.next().ok_or("usage: split <input.pdf> <out_dir>")?;
    let out_dir = args.next().ok_or("usage: split <input.pdf> <out_dir>")?;

    let pdf = fs::read(&input).map_err(|err| format!("read {input}: {err}"))?;
    let pages = split_pdfs(&pdf).map_err(|err| format!("split: {err}"))?;
    fs::create_dir_all(&out_dir).map_err(|err| format!("create dir {out_dir}: {err}"))?;
    let out_dir = Path::new(&out_dir);
    for (index, page) in pages.iter().enumerate() {
        let path = out_dir.join(format!("page-{:03}.pdf", index + 1));
        fs::write(&path, page).map_err(|err| format!("write {}: {err}", path.display()))?;
    }
    println!("wrote {} file(s) to {}", pages.len(), out_dir.display());
    Ok(())
}
