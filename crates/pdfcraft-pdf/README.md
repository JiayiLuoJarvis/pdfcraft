# pdfcraft-pdf

**Lay out business documents in Rust and emit vector PDF.**

You build a `DocumentSpec` — flow content, headers/footers, and page-anchored
marks — then call `layout_spec` and `to_pdf`. The library measures text with
[skrifa](https://crates.io/crates/skrifa), breaks lines and pages, places
tables and lists, draws through an internal canvas, and serializes PDF with a
private [krilla](https://crates.io/crates/krilla) adapter. krilla types are not
part of the public API.

| | |
| --- | --- |
| Guide | [docs/guide.md](docs/guide.md) |
| API | [docs.rs/pdfcraft-pdf](https://docs.rs/pdfcraft-pdf) |
| MSRV | 1.92 |
| License | MIT OR Apache-2.0 |
| Status | `0.1.0`. The API can break before `1.0` |

---

## Install

```toml
[dependencies]
pdfcraft-pdf = "0.1.0"
```

Default features enable the krilla backend (`to_pdf`).

```toml
# Optional. Re-exports `pt_near` and `PT_ABS_TOLERANCE` at the crate root.
# `canvas::pt_near` is available without this feature.
pdfcraft-pdf = { version = "0.1.0", features = ["test-utils"] }
```

---

## Quick start

```rust
use pdfcraft_pdf::spec::{
    BandInset, BandSlot, BandSpec, Block, DocumentInfo, FontFace, FontFamily, FontRegistry,
    Resources, SectionSpec, TextRun,
};
use pdfcraft_pdf::{layout_spec, to_pdf, DocumentSpec, Limits, Margins, PageSize, Result};

fn main() -> Result<()> {
    let font = std::fs::read("path/to/face.ttf").expect("font");
    let resources = Resources::with_fonts(FontRegistry::single(FontFamily {
        normal: FontFace { bytes: font, index: 0 },
        bold: None,
    }));

    let rendered = layout_spec(DocumentSpec {
        page_size: PageSize::A4,
        resources,
        limits: Limits::default(),
        info: DocumentInfo::default(),
        sections: vec![SectionSpec {
            margins: Margins::try_uniform(48.0)?,
            header: Some(BandSpec {
                inset: BandInset::PageMargins,
                left: Some(BandSlot::Text {
                    runs: vec![TextRun::Literal("pdfcraft-pdf".into())],
                    font: (),
                }),
                right: Some(BandSlot::Text {
                    runs: vec![
                        TextRun::Literal("Page ".into()),
                        TextRun::PageNumber,
                        TextRun::Literal(" / ".into()),
                        TextRun::PageCount,
                    ],
                    font: (),
                }),
            }),
            body: vec![
                Block::paragraph((), "Hello from pdfcraft-pdf."),
                Block::paragraph((), "Missing glyphs fail the whole document."),
            ],
            ..SectionSpec::default()
        }],
    })?;

    let pdf = to_pdf(rendered)?;
    assert!(pdf.starts_with(b"%PDF"));
    std::fs::write("out/quickstart.pdf", pdf).expect("write");
    Ok(())
}
```

Runnable demos:

```bash
cargo run -p pdfcraft-pdf --example hello
cargo run -p pdfcraft-pdf --example bill   # needs a CJK face; see examples/README.md
```

---

## How it works

```text
┌─────────────┐     layout_spec      ┌──────────┐    to_pdf     ┌─────────┐
│ DocumentSpec│ ──────────────────► │ Rendered │ ────────────► │ PDF     │
│ + Resources │   measure / break    │ pages +  │   krilla      │ bytes   │
└─────────────┘   paginate / paint   │ fonts    │   (private)   └─────────┘
                                     └──────────┘
```

1. **Flow body** — paragraphs, spacers, page breaks, keep-together, styled
   scopes, columns, tables, lists, images, horizontal rules.
2. **Bands** — header/footer slots; band height is the top/bottom margin.
3. **Layers** — `behind` / `front` marks in page coordinates (seals, watermarks,
   paths).

Coordinates: PDF points, **top-left origin, y downward**.  
Fonts: register face bytes under your own key type. A missing cmap entry fails the document.  
Dynamic data: use Rust `if` / `for` when building values — there is no template
DSL and no caller-provided layout closures.

Deep dive: **[docs/guide.md](docs/guide.md)** (blocks, tables, links, limits,
errors, testing).

---

## Feature flags

| Feature | Default | Purpose |
| --- | --- | --- |
| `backend-krilla` | ✓ | Private PDF backend + `to_pdf` |
| `test-utils` | | `pt_near` / `PT_ABS_TOLERANCE` for layout tests |

---

## Capsule API

| Item | Role |
| --- | --- |
| `DocumentSpec` / `SectionSpec` | Document and section values |
| `DocumentSpec.info` | PDF title and author (`DocumentInfo`). Creator stays `pdfcraft` |
| `FontRegistry` / `Resources::with_fonts` | Faces keyed by your type, plus `add_image` → `ImageRef` |
| `Block` and friends | Flow content |
| `BandSpec` / `TextRun` | Header/footer; page numbers at paint time |
| `LayerMark` / `PageWhen` | Page-anchored marks |
| `Limits` | Batch safety caps |
| `layout_spec` | Measure, paginate, record |
| `to_pdf` | Serialize `Rendered` → `Vec<u8>` |
| `Errors` / `ErrorKind` | Up to 64 failures with optional `Location` |

---

## Design constraints (v1)

- Horizontal Chinese + Latin; no bidi, vertical text, kerning, ligatures, or justify
- No TOC / QR / SVG / in-flow canvas / encryption APIs
- No HTTP image fetch; no fallback fonts
- No second PDF backend inside this crate
- `Flowable` stays private

Filling existing AcroForm PDFs is a separate crate (`pdfcraft-forms`), not a
feature flag here.

---

## Compatibility

- **Rust**: MSRV 1.92 (edition 2021)
- **`unsafe`**: forbidden in this crate
- **SemVer**: `0.x` — breaking changes may land without a `1.0` bump; read
  `CHANGELOG.md`

---

## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))
- MIT license ([LICENSE-MIT](LICENSE-MIT))

at your option.
