//! Stamp `/Info` Title and encrypt in one pass.

#![expect(clippy::print_stdout, reason = "example reports the written PDF path")]

use std::env;
use std::fs;

use pdfcraft_post::{Encrypt, EncryptionPreset, SetMetadata, StampAndEncrypt};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = env::args().skip(1);
    let input = args
        .next()
        .ok_or("usage: stamp <input.pdf> <output.pdf> <owner-password> <title>")?;
    let output = args
        .next()
        .ok_or("usage: stamp <input.pdf> <output.pdf> <owner-password> <title>")?;
    let owner = args
        .next()
        .ok_or("usage: stamp <input.pdf> <output.pdf> <owner-password> <title>")?;
    let title = args.next().unwrap_or_else(|| "stamped".into());

    let pdf = fs::read(&input).map_err(|err| format!("read {input}: {err}"))?;
    let out = (StampAndEncrypt {
        metadata: SetMetadata {
            title: Some(title),
            ..SetMetadata::default()
        },
        encrypt: Encrypt {
            config: Some(EncryptionPreset::READ_ONLY_PRINTABLE),
            owner_password: Some(owner),
            user_password: None,
        },
    })
    .apply(&pdf)
    .map_err(|err| format!("stamp: {err}"))?;
    fs::write(&output, &out).map_err(|err| format!("write {output}: {err}"))?;
    println!("wrote {output} ({} bytes)", out.len());
    Ok(())
}
