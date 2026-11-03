//! Encrypt a PDF with the read-only printable preset.

#![expect(clippy::print_stdout, reason = "example reports the written PDF path")]

use std::env;
use std::fs;

use pdfcraft_post::{Encrypt, EncryptionPreset};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = env::args().skip(1);
    let input = args
        .next()
        .ok_or("usage: encrypt <input.pdf> <output.pdf> <owner-password>")?;
    let output = args
        .next()
        .ok_or("usage: encrypt <input.pdf> <output.pdf> <owner-password>")?;
    let owner = args
        .next()
        .ok_or("usage: encrypt <input.pdf> <output.pdf> <owner-password>")?;

    let pdf = fs::read(&input).map_err(|err| format!("read {input}: {err}"))?;
    let out = (Encrypt {
        config: Some(EncryptionPreset::READ_ONLY_PRINTABLE),
        owner_password: Some(owner),
        user_password: None,
    })
    .apply(&pdf)
    .map_err(|err| format!("encrypt: {err}"))?;
    fs::write(&output, &out).map_err(|err| format!("write {output}: {err}"))?;
    println!("wrote {output} ({} bytes)", out.len());
    Ok(())
}
