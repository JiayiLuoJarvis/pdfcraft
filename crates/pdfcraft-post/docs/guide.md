# pdfcraft-post guide

How to post-process existing PDF bytes with `pdfcraft-post`: mental model,
metadata, encryption presets, stamp-and-encrypt, merge, pipelines, errors, and
testing. For a one-page overview see the [crate README](../README.md). For every
public item signature see [rustdoc](https://docs.rs/pdfcraft-post)
(`cargo doc -p pdfcraft-post --open` locally).

Maintainer behavior contracts live in the workspace
[`docs/pdfcraft-post-spec.md`](../../../docs/pdfcraft-post-spec.md). This guide
is the caller-facing how-to.

---

## Mental model

You pass PDF **bytes** in and get PDF **bytes** out. There is no path API and no
network access. Processors are plain values with an `apply` method.
`Pipeline` runs several steps on one buffer. `merge_pdfs` takes many buffers and
usually runs before a pipeline.

```text
PDF bytes  →  SetMetadata.apply       →  PDF bytes
PDF bytes  →  Encrypt.apply           →  PDF bytes
PDF bytes  →  StampAndEncrypt.apply   →  PDF bytes
PDF bytes… →  merge_pdfs              →  PDF bytes
PDF bytes  →  split_pdfs              →  one single-page PDF per page
PDF bytes  →  first_page_jpeg (*)     →  JPEG bytes (feature-gated)
PDF bytes  →  Pipeline::run           →  PDF bytes
```

(`*`) `first_page_jpeg` needs the opt-in `thumbnail` feature. Without it
every input that would otherwise render reports `ThumbnailFailed`; empty input
still yields `None`, and `NotAPdf` / `Encrypted` still apply.

This crate is not `pdfcraft-pdf` layout and not `pdfcraft-forms` fill. It does
not depend on either, and does not use krilla.

```rust
use pdfcraft_post::{
    Encrypt, EncryptionPreset, Pipeline, SetMetadata, StampAndEncrypt, Step, Result,
};

fn finalize(pdf: &[u8]) -> Result<Vec<u8>> {
    // Prefer StampAndEncrypt when metadata and encrypt always run together.
    StampAndEncrypt {
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
    .apply(pdf)
}

fn metadata_only_then_encrypt(pdf: &[u8]) -> Result<Vec<u8>> {
    Pipeline::new([
        Step::Metadata(SetMetadata {
            author: Some("Ada".into()),
            ..SetMetadata::default()
        }),
        Step::Encrypt(Encrypt {
            config: Some(EncryptionPreset::READ_ONLY),
            owner_password: Some("owner-secret".into()),
            user_password: None,
        }),
    ])
    .run(pdf)
}
```

---

## SetMetadata

Write or overwrite selected `/Info` fields. Every field is `Option`. `None`
leaves the existing value alone. Fields that are `Some` overwrite.

| Field | PDF key | Notes |
| --- | --- | --- |
| `title` | `/Title` | |
| `author` | `/Author` | |
| `subject` | `/Subject` | |
| `keywords` | `/Keywords` | Joined with `", "` when written |
| `creator` | `/Creator` | **Writable** here |
| `producer` | `/Producer` | **Writable** here |
| `creation_date` | `/CreationDate` | PDF `D:` date syntax |
| `modification_date` | `/ModDate` | PDF `D:` date syntax |

### Difference from `pdfcraft-pdf`

| | `DocumentSpec.info` (`pdfcraft-pdf`) | `SetMetadata` (`pdfcraft-post`) |
| --- | --- | --- |
| Role | Write info while **generating** a PDF | Change `/Info` on an **existing** PDF |
| Creator | Fixed as `pdfcraft` | Caller may set |
| Producer | Not exposed | Caller may set |
| Keywords | `Vec`-style field on layout info | Joined to a comma-separated string |

Dates must be ASCII and start with `D:` with at least six characters (for
example `D:20261014120000`). Invalid syntax fails with `MetadataFailed`.

Encrypted input fails (`MetadataFailed`). Non-PDF bytes fail with `NotAPdf`.

```rust
use pdfcraft_post::SetMetadata;

let out = SetMetadata {
    title: Some("Quarterly report".into()),
    creator: Some("my-exporter".into()),
    producer: Some("pdfcraft-post".into()),
    keywords: Some(vec!["finance".into(), "q3".into()]),
    modification_date: Some("D:20261014120000".into()),
    ..SetMetadata::default()
}
.apply(pdf)?;
```

---

## Encrypt

Encrypt with AES-256, Standard Security Handler revision 6 (lopdf
`EncryptionVersion::V5`). Compatible with tools that expect pikepdf-style `R=6`
/ `aesv3`.

### Required inputs

- `Encrypt::config` must be `Some(...)`. Missing config → `ConfigMissing`.
- A non-empty **owner** password must come from `Encrypt::owner_password` or
  `EncryptionConfig::owner_password`. The library never invents one.
- **User** password may be empty. Empty user password means anyone can open the
  file; owner still controls permission bits. This is a common production setup.

`Encrypt::owner_password` / `user_password` override the config when `Some`.

### Presets

Passwords stay unset on the preset. Supply the owner password on `Encrypt` (or
on the config). Every preset keeps accessibility extraction on.

| Preset | Print | High-quality print | Copy / edit / annotate / fill / assemble | Accessibility |
| --- | --- | --- | --- | --- |
| `READ_ONLY` | off | off | off | on |
| `READ_ONLY_PRINTABLE` | on | on | off | on |
| `FULL_PERMISSIONS` | on | on | on | on |

For custom bits, clone a preset into `EncryptionConfig` and flip fields, or
build `EncryptionConfig` directly.

```rust
use pdfcraft_post::{Encrypt, EncryptionPreset};

let out = Encrypt {
    config: Some(EncryptionPreset::READ_ONLY_PRINTABLE),
    owner_password: Some("owner-secret".into()),
    user_password: None, // keep config's empty user password
}
.apply(pdf)?;
```

Already-encrypted input fails (`EncryptFailed`). Non-PDF bytes fail with
`NotAPdf`.

---

## StampAndEncrypt

Holds one `SetMetadata` and one `Encrypt`. Writes `/Info` then encrypts in a
**single** open/save path. Prefer this when both always run together.

Semantically equivalent to metadata then encrypt. Prefer
`StampAndEncrypt` over `Pipeline(SetMetadata, Encrypt)` to avoid two full
serializations. `Pipeline` remains available when steps are optional or
assembled dynamically.

```rust
use pdfcraft_post::{Encrypt, EncryptionPreset, SetMetadata, StampAndEncrypt};

let out = StampAndEncrypt {
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
.apply(pdf)?;
```

---

## merge_pdfs

```rust
use pdfcraft_post::merge_pdfs;

let out = merge_pdfs([&a, &b, &c], false)?;
```

| Input | Result |
| --- | --- |
| Empty list | `MergeEmpty` |
| One part | Same bytes returned (no forced round-trip) |
| Two or more | Pages concatenated in list order |

`preserve_outlines`:

- `false` (typical default for callers): drop outlines.
- `true`: keep the **first** part's outlines when present; do not merge outlines
  from later parts.

Attachments and structure trees are not kept. Encrypted parts fail
(`MergeFailed`). Non-PDF parts fail with `NotAPdf`.

Because merge takes many inputs, run it **outside** `Pipeline`, then pass the
merged buffer into `Pipeline::run` if you also need metadata or encryption.

```rust
use pdfcraft_post::{merge_pdfs, Encrypt, EncryptionPreset, Pipeline, Step};

let merged = merge_pdfs([&a, &b], false)?;
let out = Pipeline::new([Step::Encrypt(Encrypt {
    config: Some(EncryptionPreset::READ_ONLY),
    owner_password: Some("owner-secret".into()),
    user_password: None,
})])
.run(&merged)?;
```

---

## split_pdfs

Split one PDF into one single-page PDF per page.

```rust
use pdfcraft_post::split_pdfs;

let parts: Vec<Vec<u8>> = split_pdfs(pdf)?;
```

| Input | Result |
| --- | --- |
| No pages | `Ok(vec![])`; splitting takes one document, so there is no empty-list error |
| N pages | N outputs; output `i` holds page `i` |

Each output is a self-contained single-page PDF: its own `Pages` parent
(`Count` 1), the page's materialized `MediaBox` / `Resources` / `CropBox` /
`Rotate`, and the page-level `Annots`. Document-level outlines, attachments,
and structure trees are not kept (same rule as `merge_pdfs` without outline
preservation).

Outputs stay in reading order, so a split round-trips through merge: feeding
the parts back into `merge_pdfs` keeps page count and order (covered by
`split_then_merge_round_trip` in `tests/split.rs`).

Encrypted input fails with `Encrypted`, including files whose empty user
password `load_mem` would auto-decrypt; a split never re-saves auto-decrypted
bytes. Non-PDF bytes fail with `NotAPdf`. A page that cannot be copied fails
with `SplitFailed`.

```bash
cargo run -p pdfcraft-post --example split -- in.pdf out_dir
```

---

## first-page thumbnail (optional thumbnail feature)

Render page 1 of a PDF to JPEG bytes. Only the first page renders: a
multi-page input still yields exactly one JPEG.

The renderer lives behind the opt-in `thumbnail` feature (hayro plus image),
so enable it explicitly:

```bash
cargo run -p pdfcraft-post --features thumbnail --example thumbnail -- in.pdf out.jpg
cargo test -p pdfcraft-post --features thumbnail
```

```rust
use pdfcraft_post::{first_page_jpeg, ThumbnailOptions};

let mut opts = ThumbnailOptions::default();
opts.max_longest_side = 320;
if let Some(jpeg) = first_page_jpeg(pdf, &opts)? {
    std::fs::write("thumb.jpg", jpeg).expect("write");
}
```

(`ThumbnailOptions` is `non_exhaustive`: start from `default()` and assign
the fields to change.)

| Option | Default | Rules |
| --- | --- | --- |
| `max_longest_side` | `320` | Longest output side in pixels. A page that already fits keeps its native size; output is never upscaled. `0` cannot fit any page, so rendering fails with `ThumbnailFailed`. |
| `quality` | `75` | JPEG quality, clamped to `1..=100`. `0` and values above `100` do not error; they behave as `1` and `100`. |

A PDF with no pages yields `Ok(None)`; that is not an error. Alpha flattens
to white, so the output never carries transparency.

Encrypted input fails with `Encrypted`, including files whose empty user
password `load_mem` would auto-decrypt; the input gate runs before any
renderer call, so an encrypted input never reaches the renderer. Non-PDF
bytes fail with `NotAPdf`. Everything else fails with `ThumbnailFailed`:
unresolvable page size, disabled `thumbnail` feature, or a render/encode
failure. Without the feature, inputs that would otherwise render report
`ThumbnailFailed`; empty input still yields `None`, and `NotAPdf` /
`Encrypted` still apply because the input gate runs first.

No system packages, fonts, or Docker services needed: hayro renders
in-process from the PDF bytes.

A thumbnail renders **out of** the PDF (you get JPEG bytes). That is not a
watermark, which would draw **into** the PDF; Watermark stays deferred (see
below).

---

## Pipeline

Ordered list of `Step` values applied to one buffer.

| Variant | Processor |
| --- | --- |
| `Step::Metadata(SetMetadata)` | `SetMetadata::apply` |
| `Step::Encrypt(Encrypt)` | `Encrypt::apply` |
| `Step::StampAndEncrypt(StampAndEncrypt)` | `StampAndEncrypt::apply` |

An empty pipeline returns the input bytes unchanged (`to_vec`).

```rust
use pdfcraft_post::{Pipeline, SetMetadata, Step};

let out = Pipeline::new([Step::Metadata(SetMetadata {
    subject: Some("filed".into()),
    ..SetMetadata::default()
})])
.run(pdf)?;
```

---

## Errors

Match on `Error::kind` (`ErrorKind`). Equality compares kind only. Use
`std::error::Error::source` for dependency details. There is no HTTP-oriented
`to_dict`.

| Kind | When |
| --- | --- |
| `NotAPdf` | Bytes are not an openable PDF |
| `Encrypted` | Input PDF is encrypted; the crate never unlocks input |
| `ConfigMissing` | Encrypt config missing, or owner password empty after overrides |
| `EncryptFailed` | Already encrypted, random key failure, or security handler write failed |
| `MetadataFailed` | Encrypted input, bad `D:` date, or `/Info` write failed |
| `MergeEmpty` | `merge_pdfs` received an empty part list |
| `MergeFailed` | Encrypted part, page copy failure, or merge serialize failure |
| `SplitFailed` | Page copy or single-page save failed in `split_pdfs` |
| `ThumbnailFailed` | Page size unresolvable, `thumbnail` feature off, or render/encode failed |

`ErrorKind` is `#[non_exhaustive]`. Map kinds in your application layer.

---

## Examples and tests

```bash
cargo run -p pdfcraft-post --example encrypt -- in.pdf out.pdf owner-secret
cargo run -p pdfcraft-post --example stamp -- in.pdf out.pdf owner-secret "Title"
cargo run -p pdfcraft-post --example merge -- out.pdf a.pdf b.pdf
cargo run -p pdfcraft-post --example split -- in.pdf out_dir
cargo run -p pdfcraft-post --features thumbnail --example thumbnail -- in.pdf out.jpg
cargo test -p pdfcraft-post
cargo test -p pdfcraft-post --features thumbnail
cargo doc -p pdfcraft-post --no-deps
```

Prefer assertions on:

- Output starts with `%PDF`
- Empty user password opens after encrypt
- Permission presets match expected bits
- `merge_pdfs` empty → `MergeEmpty`; one part → same bytes
- `split_pdfs` empty → `Ok(vec![])`; N pages → N single-page outputs; split-then-merge keeps order
- `first_page_jpeg` empty → `Ok(None)`; longest side capped; quality clamped; feature off → `ThumbnailFailed`
- Error kinds for illegal inputs (`NotAPdf`, `Encrypted`, `ConfigMissing`, …)

---

## What this crate does not do

- Watermark (deferred); a first-page JPEG renders out of the PDF and is not a watermark
- Input byte/page limits / `configure_limits` (deferred); `split_pdfs` takes page count as-is
- HTTP adapters, `to_dict`, or Web finalize orchestration
- Auto-generate owner passwords inside the library
- Depend on `pdfcraft-pdf`, `pdfcraft-forms`, or `pdfcraft-orgchart`
- Re-export `lopdf` types
- Put encrypt / merge / post-process Info into `pdfcraft-pdf` features

Post-processing existing PDFs is not a feature flag of `pdfcraft-pdf`. Use this
crate.

---

## Crate map

| Path | Role |
| --- | --- |
| `SetMetadata` / `Encrypt` / `StampAndEncrypt` | Public processors |
| `merge_pdfs` | Multi-PDF page concatenate |
| `split_pdfs` | One single-page PDF per page |
| `first_page_jpeg` / `ThumbnailOptions` | Page-1 JPEG (`thumbnail` feature) |
| `Pipeline` / `Step` | Ordered single-buffer steps |
| `EncryptionConfig` / `EncryptionPreset` | Permissions and presets |
| `Error` / `ErrorKind` | Failures |
| `examples/` | Runnable demos (`encrypt`, `stamp`, `merge`, `split`, `thumbnail`) |
| `docs/guide.md` | This guide |
| Workspace `docs/pdfcraft-post-spec.md` | Maintainer reference |
