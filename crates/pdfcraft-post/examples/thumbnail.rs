//! Render the first page of a PDF to a JPEG thumbnail.
//!
//! Requires the `thumbnail` feature:
//! `cargo run -p pdfcraft-post --features thumbnail --example thumbnail`.

use std::process::ExitCode;

#[cfg(feature = "thumbnail")]
fn main() -> ExitCode {
    use std::{env, fs};

    use pdfcraft_post::{first_page_jpeg, ThumbnailOptions};

    let mut args = env::args().skip(1);
    let Some(input) = args.next() else {
        eprintln!("usage: thumbnail <input.pdf> <output.jpg>");
        return ExitCode::FAILURE;
    };
    let Some(output) = args.next() else {
        eprintln!("usage: thumbnail <input.pdf> <output.jpg>");
        return ExitCode::FAILURE;
    };

    let pdf = match fs::read(&input) {
        Ok(bytes) => bytes,
        Err(err) => {
            eprintln!("read {input}: {err}");
            return ExitCode::FAILURE;
        }
    };

    let thumb = match first_page_jpeg(&pdf, &ThumbnailOptions::default()) {
        Ok(Some(bytes)) => bytes,
        Ok(None) => {
            eprintln!("thumbnail: input has no pages");
            return ExitCode::FAILURE;
        }
        Err(err) => {
            eprintln!("thumbnail: {err}");
            return ExitCode::FAILURE;
        }
    };

    if let Err(err) = fs::write(&output, &thumb) {
        eprintln!("write {output}: {err}");
        return ExitCode::FAILURE;
    }
    println!("wrote {output} ({} bytes)", thumb.len());
    ExitCode::SUCCESS
}

#[cfg(not(feature = "thumbnail"))]
fn main() -> ExitCode {
    eprintln!(
        "thumbnail example needs the `thumbnail` feature; \
         re-run with `cargo run -p pdfcraft-post --features thumbnail \
         --example thumbnail -- <input.pdf> <output.jpg>`"
    );
    ExitCode::FAILURE
}
