//! Encrypt a PDF with the read-only printable preset.

use std::env;
use std::fs;
use std::process::ExitCode;

use pdfcraft_post::{Encrypt, EncryptionPreset};

fn main() -> ExitCode {
    let mut args = env::args().skip(1);
    let Some(input) = args.next() else {
        eprintln!("usage: encrypt <input.pdf> <output.pdf> <owner-password>");
        return ExitCode::FAILURE;
    };
    let Some(output) = args.next() else {
        eprintln!("usage: encrypt <input.pdf> <output.pdf> <owner-password>");
        return ExitCode::FAILURE;
    };
    let Some(owner) = args.next() else {
        eprintln!("usage: encrypt <input.pdf> <output.pdf> <owner-password>");
        return ExitCode::FAILURE;
    };

    let pdf = match fs::read(&input) {
        Ok(bytes) => bytes,
        Err(err) => {
            eprintln!("read {input}: {err}");
            return ExitCode::FAILURE;
        }
    };

    let out = match (Encrypt {
        config: Some(EncryptionPreset::READ_ONLY_PRINTABLE),
        owner_password: Some(owner),
        user_password: None,
    })
    .apply(&pdf)
    {
        Ok(bytes) => bytes,
        Err(err) => {
            eprintln!("encrypt: {err}");
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
