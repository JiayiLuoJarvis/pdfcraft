//! Merge two or more PDFs into one file.

#![expect(clippy::print_stdout, reason = "example reports the written PDF path")]

use std::env;
use std::fs;

use pdfcraft_post::merge_pdfs;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = env::args().skip(1);
    let output = args
        .next()
        .ok_or("usage: merge <output.pdf> <input1.pdf> [input2.pdf ...]")?;
    let inputs: Vec<String> = args.collect();
    if inputs.is_empty() {
        return Err("usage: merge <output.pdf> <input1.pdf> [input2.pdf ...]".into());
    }

    let mut parts = Vec::new();
    for path in &inputs {
        parts.push(fs::read(path).map_err(|err| format!("read {path}: {err}"))?);
    }

    let out = merge_pdfs(&parts, false).map_err(|err| format!("merge: {err}"))?;
    fs::write(&output, &out).map_err(|err| format!("write {output}: {err}"))?;
    println!("wrote {output} ({} bytes)", out.len());
    Ok(())
}
