# pdfcraft-post

Post-process existing PDF bytes: write `/Info`, encrypt (AES-256 / revision 6),
merge page lists, and run a `Pipeline`.

| Item | |
| --- | --- |
| Guide | [`docs/guide.md`](docs/guide.md) |
| API | [docs.rs/pdfcraft-post](https://docs.rs/pdfcraft-post) |

## Install

```toml
[dependencies]
pdfcraft-post = "0.1"
```

## Quick start

```rust
use pdfcraft_post::{Encrypt, EncryptionPreset, SetMetadata, StampAndEncrypt};

let stamped = StampAndEncrypt {
    metadata: SetMetadata {
        title: Some("Export".into()),
        ..SetMetadata::default()
    },
    encrypt: Encrypt {
        config: Some(EncryptionPreset::READ_ONLY_PRINTABLE),
        owner_password: Some("owner-secret".into()),
        user_password: None,
    },
}
.apply(pdf_bytes)?;
```

Processors: `SetMetadata`, `Encrypt`, `StampAndEncrypt`, `merge_pdfs`, `Pipeline`.

## Non-goals (this stack)

- Watermark (deferred)
- Input byte/page limits (deferred)
- HTTP / `to_dict` adapters
- Web finalize orchestration or library-side auto-generated owner passwords

## License

MIT OR Apache-2.0
