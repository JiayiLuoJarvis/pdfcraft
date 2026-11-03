//! Render the first page of a PDF to a JPEG thumbnail.
//!
//! Requires the `thumbnail` feature:
//! `cargo run -p pdfcraft-post --features thumbnail --example thumbnail`.

#![cfg_attr(
    feature = "thumbnail",
    expect(clippy::print_stdout, reason = "example reports the written JPEG path")
)]

#[cfg(feature = "thumbnail")]
use std::env;
#[cfg(feature = "thumbnail")]
use std::fs;

#[cfg(feature = "thumbnail")]
use pdfcraft_post::{first_page_jpeg, ThumbnailOptions};

#[cfg(feature = "thumbnail")]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = env::args().skip(1);
    let input = args
        .next()
        .ok_or("usage: thumbnail <input.pdf> <output.jpg>")?;
    let output = args
        .next()
        .ok_or("usage: thumbnail <input.pdf> <output.jpg>")?;

    let pdf = fs::read(&input).map_err(|err| format!("read {input}: {err}"))?;
    let thumb = first_page_jpeg(&pdf, &ThumbnailOptions::default())
        .map_err(|err| format!("thumbnail: {err}"))?
        .ok_or("thumbnail: input has no pages")?;
    fs::write(&output, &thumb).map_err(|err| format!("write {output}: {err}"))?;
    println!("wrote {output} ({} bytes)", thumb.len());
    Ok(())
}

#[cfg(not(feature = "thumbnail"))]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    Err("thumbnail example needs the `thumbnail` feature; \
         re-run with `cargo run -p pdfcraft-post --features thumbnail \
         --example thumbnail -- <input.pdf> <output.jpg>`"
        .into())
}
