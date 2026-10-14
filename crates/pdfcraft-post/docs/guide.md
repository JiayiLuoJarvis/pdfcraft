# pdfcraft-post guide

How to post-process PDF bytes with this crate.

## Mental model

You pass PDF bytes in and get PDF bytes out. There is no path or network API.
Steps are values (`SetMetadata`, `Encrypt`, …) with an `apply` method.
`Pipeline` runs several steps on one buffer. `merge_pdfs` takes many buffers and
usually runs before a pipeline.

This crate is not `pdfcraft-pdf` layout and not `pdfcraft-forms` fill.

## SetMetadata

Set only the fields you want to change. `None` leaves an existing `/Info` value
alone. `keywords` join with `", "`.

`creator` and `producer` are writable here. That differs from
`pdfcraft-pdf`'s `DocumentSpec.info`, which keeps Creator fixed as `pdfcraft`
and does not expose Producer.

Dates use PDF `D:` syntax as strings (for example `D:20261014120000`).

## Encrypt

Supply an `EncryptionConfig`. Use `EncryptionPreset::READ_ONLY`,
`EncryptionPreset::READ_ONLY_PRINTABLE`, or `EncryptionPreset::FULL_PERMISSIONS`.
Always pass a non-empty
owner password on `Encrypt` or on the config. Empty user password is allowed
and means anyone can open the file.

Algorithm: AES-256, Standard Security Handler revision 6 (lopdf V5).

The library does not invent owner passwords.

## StampAndEncrypt

One open/save path that writes `/Info` then encrypts. Prefer this when both
always run. `Pipeline(SetMetadata, Encrypt)` remains available.

## merge_pdfs

```rust
let out = pdfcraft_post::merge_pdfs([&a, &b], false)?;
```

Empty input fails. One part returns the same bytes. Two or more concatenate
pages in list order. `preserve_outlines: true` keeps the first part's outlines
when present. Attachments and structure trees are dropped.

## Pipeline

```rust
use pdfcraft_post::{Pipeline, SetMetadata, Step};

let out = Pipeline::new([Step::Metadata(SetMetadata {
    title: Some("x".into()),
    ..SetMetadata::default()
})])
.run(pdf)?;
```

An empty pipeline returns the input unchanged.

## Errors

Failures are `Error` with `ErrorKind` (`NotAPdf`, `ConfigMissing`,
`EncryptFailed`, `MetadataFailed`, `MergeEmpty`, `MergeFailed`). Use
`std::error::Error::source` for dependency details. There is no HTTP
serialization helper.

## Non-goals

- Watermark (deferred)
- Input limits / `configure_limits` (deferred)
- HTTP adapters and Web finalize orchestration
- Auto-generated owner passwords inside the library
