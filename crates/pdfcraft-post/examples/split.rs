//! Split a PDF into one single-page PDF per page.

use std::env;
use std::fs;
use std::path::Path;
use std::process::ExitCode;

use pdfcraft_post::split_pdfs;

fn main() -> ExitCode {
    let mut args = env::args().skip(1);
    let Some(input) = args.next() else {
        eprintln!("usage: split <input.pdf> <out_dir>");
        return ExitCode::FAILURE;
    };
    let Some(out_dir) = args.next() else {
        eprintln!("usage: split <input.pdf> <out_dir>");
        return ExitCode::FAILURE;
    };

    let pdf = match fs::read(&input) {
        Ok(bytes) => bytes,
        Err(err) => {
            eprintln!("read {input}: {err}");
            return ExitCode::FAILURE;
        }
    };

    let pages = match split_pdfs(&pdf) {
        Ok(pages) => pages,
        Err(err) => {
            eprintln!("split: {err}");
            return ExitCode::FAILURE;
        }
    };

    if let Err(err) = fs::create_dir_all(&out_dir) {
        eprintln!("create dir {out_dir}: {err}");
        return ExitCode::FAILURE;
    }
    let out_dir = Path::new(&out_dir);
    for (index, page) in pages.iter().enumerate() {
        let path = out_dir.join(format!("page-{:03}.pdf", index + 1));
        if let Err(err) = fs::write(&path, page) {
            eprintln!("write {}: {err}", path.display());
            return ExitCode::FAILURE;
        }
    }
    println!("wrote {} file(s) to {}", pages.len(), out_dir.display());
    ExitCode::SUCCESS
}
