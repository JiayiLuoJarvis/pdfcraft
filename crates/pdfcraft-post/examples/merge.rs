//! Merge two or more PDFs into one file.

use std::env;
use std::fs;
use std::process::ExitCode;

use pdfcraft_post::merge_pdfs;

fn main() -> ExitCode {
    let mut args = env::args().skip(1);
    let Some(output) = args.next() else {
        eprintln!("usage: merge <output.pdf> <input1.pdf> [input2.pdf ...]");
        return ExitCode::FAILURE;
    };
    let inputs: Vec<String> = args.collect();
    if inputs.is_empty() {
        eprintln!("usage: merge <output.pdf> <input1.pdf> [input2.pdf ...]");
        return ExitCode::FAILURE;
    }

    let mut parts = Vec::new();
    for path in &inputs {
        match fs::read(path) {
            Ok(bytes) => parts.push(bytes),
            Err(err) => {
                eprintln!("read {path}: {err}");
                return ExitCode::FAILURE;
            }
        }
    }

    let out = match merge_pdfs(&parts, false) {
        Ok(bytes) => bytes,
        Err(err) => {
            eprintln!("merge: {err}");
            return ExitCode::FAILURE;
        }
    };

    if let Err(err) = fs::write(&output, &out) {
        eprintln!("write {output}: {err}");
        return ExitCode::FAILURE;
    }
    println!("wrote {output} ({} bytes)", out.len());
    ExitCode::SUCCESS
}
