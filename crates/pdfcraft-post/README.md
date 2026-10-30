# pdfcraft-post

**Post-process existing PDF bytes: `/Info`, encrypt, merge, split, pipelines, plus optional first-page JPEG thumbnails.**

Pass PDF bytes in and get PDF bytes out. Write or overwrite selected document
info fields, encrypt with AES-256 (Standard Security Handler revision 6),
concatenate page lists, split one PDF per page, or run ordered steps in a
`Pipeline`. An opt-in `thumbnail` feature renders page 1 to JPEG bytes. Reading and
writing use [lopdf](https://crates.io/crates/lopdf). This crate does not depend
on `pdfcraft-pdf`, `pdfcraft-forms`, or `pdfcraft-orgchart`, and does not use
krilla.

| | |
| --- | --- |
| Guide | [docs/guide.md](docs/guide.md) |
| API | [docs.rs/pdfcraft-post](https://docs.rs/pdfcraft-post) |
| MSRV | 1.92 |
| License | MIT OR Apache-2.0 |
| Status | `0.2.0`. The API can break before `1.0` |

---

## Install

```toml
[dependencies]
pdfcraft-post = "0.2.0"
```

---

## Quick start

```rust
use pdfcraft_post::{
    Encrypt, EncryptionPreset, SetMetadata, StampAndEncrypt, Result,
};

fn stamp_export(pdf: &[u8]) -> Result<Vec<u8>> {
    StampAndEncrypt {
        metadata: SetMetadata {
            title: Some("Export".into()),
            creator: Some("my-app".into()),
            ..SetMetadata::default()
        },
        encrypt: Encrypt {
            config: Some(EncryptionPreset::READ_ONLY_PRINTABLE),
            owner_password: Some("owner-secret".into()),
            user_password: None, // empty user password: anyone can open
        },
    }
    .apply(pdf)
}

fn main() -> Result<()> {
    let pdf = std::fs::read("in.pdf").expect("pdf");
    let out = stamp_export(&pdf)?;
    assert!(out.starts_with(b"%PDF"));
    std::fs::write("out/stamped.pdf", out).expect("write");
    Ok(())
}
```

Runnable demos:

```bash
cargo run -p pdfcraft-post --example encrypt -- in.pdf out.pdf owner-secret
cargo run -p pdfcraft-post --example stamp -- in.pdf out.pdf owner-secret "Title"
cargo run -p pdfcraft-post --example merge -- out.pdf a.pdf b.pdf
cargo run -p pdfcraft-post --example split -- in.pdf out_dir
cargo run -p pdfcraft-post --features thumbnail --example thumbnail -- in.pdf out.jpg
```

---

## How it works

```text
PDF bytes ──SetMetadata.apply──► PDF bytes
PDF bytes ──Encrypt.apply──────► PDF bytes
PDF bytes ──StampAndEncrypt────► PDF bytes   (Info then encrypt, one open/save)
PDF bytes… ──merge_pdfs────────► PDF bytes
PDF bytes ──split_pdfs─────────► one single-page PDF per page
PDF bytes ──first_page_jpeg──► JPEG bytes  (needs `--features thumbnail`)
PDF bytes ──Pipeline.run───────► PDF bytes   (ordered steps on one buffer)
```

1. **Bytes only** — no path API and no network access.
2. **Processors are values** — each step has `apply` (or `Pipeline::run`).
3. **Merge is multi-input** — usually merge first, then run a pipeline.

`creator` and `producer` are writable here. That differs from
`pdfcraft-pdf`'s `DocumentSpec.info`, which keeps Creator fixed as `pdfcraft`
and does not expose Producer.

Deep dive: **[docs/guide.md](docs/guide.md)** (fields, presets, merge rules,
errors, testing).

---

## Capsule API

| Item | Role |
| --- | --- |
| `SetMetadata` | Write selected `/Info` fields (`None` = leave alone) |
| `Encrypt` | AES-256 encrypt; config required; owner password required |
| `EncryptionPreset` | `READ_ONLY`, `READ_ONLY_PRINTABLE`, `FULL_PERMISSIONS` |
| `EncryptionConfig` | Full permission bits and default passwords |
| `StampAndEncrypt` | Metadata then encrypt in one open/save |
| `merge_pdfs` | Concatenate pages; optional first-part outlines |
| `split_pdfs` | One single-page PDF per page; empty splits to `Ok(vec![])` |
| `first_page_jpeg` / `ThumbnailOptions` | Page-1 JPEG; opt-in `thumbnail` feature; empty yields `None` |
| `Pipeline` / `Step` | Ordered steps on one buffer |
| `Error` / `ErrorKind` | Open, config, encrypt, metadata, and merge failures |

---

## Design constraints (v1)

- No Watermark and no input byte/page limits (deferred); first-page JPEGs render out of the PDF and are not watermarks
- No HTTP / `to_dict` adapters and no Web finalize orchestration
- Owner passwords are caller-supplied; the library never invents one
- Empty user password is a supported "anyone can open" setup
- No dependency on `pdfcraft-pdf`, `pdfcraft-forms`, or `pdfcraft-orgchart`
- `lopdf` types are not re-exported

---

## Compatibility

- **Rust**: MSRV 1.92 (edition 2021)
- **`unsafe`**: forbidden in this crate
- **SemVer**: `0.x` — breaking changes may land without a `1.0` bump; read
  `CHANGELOG.md`

---

## License

Licensed under either of

- Apache License, Version 2.0, [LICENSE-APACHE](LICENSE-APACHE)
- MIT license, [LICENSE-MIT](LICENSE-MIT)

at your option.
