# pdfcraft-orgchart

**Draw equity and outbound-investment charts to PDF.**

Build a `ChartRequest` (subject, nodes, holding edges, options), pass one font
face as `FontBytes`, and call `generate_pdf`. Shareholders sit above the
subject. Outbound investments sit below. Measurement uses
[skrifa](https://crates.io/crates/skrifa). Drawing uses
[krilla](https://crates.io/crates/krilla). This crate does not depend on
`pdfcraft-pdf` or `pdfcraft-forms`.

| | |
| --- | --- |
| Guide | [docs/guide.md](docs/guide.md) |
| API | [docs.rs/pdfcraft-orgchart](https://docs.rs/pdfcraft-orgchart) |
| MSRV | 1.92 |
| License | MIT OR Apache-2.0 |
| Status | `0.1.1`. The API can break before `1.0` |

---

## Install

```toml
[dependencies]
pdfcraft-orgchart = "0.1.1"
```

---

## Quick start

```rust
use pdfcraft_orgchart::{
    document_title, generate_pdf, validate_request, ChartRequest, Edge, FontBytes, Node,
};

fn main() -> Result<(), pdfcraft_orgchart::Error> {
    let font = FontBytes::new(std::fs::read("face.ttf").expect("font"));
    let request = ChartRequest::new(
        "root",
        vec![
            Node::new("holder", "甲股东"),
            Node::new("root", "主体公司"),
            Node::new("invest", "乙投资"),
        ],
        vec![Edge::new("holder", "root"), Edge::new("root", "invest")],
    );
    validate_request(&request)?;
    let _title = document_title(&request)?;
    let pdf = generate_pdf(&request, &font)?;
    assert!(pdf.starts_with(b"%PDF"));
    std::fs::write("out/equity_chart.pdf", pdf).expect("write");
    Ok(())
}
```

Runnable demo (needs `PDFCRAFT_ORGCHART_FONT` or `PDFCRAFT_CJK_FONT`):

```bash
cargo run -p pdfcraft-orgchart --example equity_chart
```

---

## How it works

```text
ChartRequest (+ FontBytes) ──validate_request──► Ok (optional)
                           ──generate_pdf──────► PDF bytes
ChartRequest ──document_title──► String (you write /Info)
```

1. **Assemble** — nodes, edges (`from_id` holds `to_id`), options.
2. **Validate** — protocol and optional `top_n` fold without opening a font.
3. **Draw** — fold again, measure, paginate, emit PDF.

Fonts: one face, shared as `Arc<Vec<u8>>`. `FontBytes::new` wraps owned bytes. `FontBytes::from_shared` reuses an `Arc`. Collections are rejected.  
Title: `generate_pdf` does not write document info. Use `document_title` if you
need a `/Title` string.

Deep dive: **[docs/guide.md](docs/guide.md)** (options, display rules, errors,
testing).

---

## Capsule API

| Item | Role |
| --- | --- |
| `ChartRequest` / `ChartOptions` | Chart values, `top_n`, size caps |
| `Node` / `Edge` / `NodeType` | Parties and holding edges |
| `FontBytes` | Shared face bytes. `new` wraps owned bytes. `from_shared` reuses an `Arc`. |
| `validate_request` | Protocol check |
| `document_title` | Title string for caller-owned PDF info |
| `generate_pdf` | Draw chart → `Vec<u8>` |
| `Error` / `ErrorCode` | Stable failure codes |
| `MAX_NODES` and related | Hard caps for options |

---

## Design constraints (v1)

- No dependency on `pdfcraft-pdf` or `pdfcraft-forms`
- No font collections; one face per call
- Page headers come from chart role, not from `ChartOptions::title`
- Fonts are not vendored in the published package

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

OFL notes for a common Source Han reference face are in
[tests/fixtures/fonts/](tests/fixtures/fonts/).
