# pdfcraft-orgchart

**Draw equity and outbound-investment charts to PDF.**

Build a `ChartRequest`, compile it with `Chart::new`, and draw with
`chart.to_pdf`. Pass one parsed `Font`. Shareholders sit above the subject.
Outbound investments sit below. Measurement uses
[skrifa](https://crates.io/crates/skrifa). Drawing uses
[krilla](https://crates.io/crates/krilla). This crate does not depend on
`pdfcraft-pdf` or `pdfcraft-forms`.

| | |
| --- | --- |
| Guide | [docs/guide.md](docs/guide.md) |
| API | [docs.rs/pdfcraft-orgchart](https://docs.rs/pdfcraft-orgchart) |
| MSRV | 1.98 |
| License | MIT OR Apache-2.0 |
| Status | `0.3.0`. The API can break before `1.0` |

---

## Install

```toml
[dependencies]
pdfcraft-orgchart = "0.3.1"
```

---

## Quick start

```rust
use pdfcraft_orgchart::{Chart, ChartRequest, Edge, Font, Node};

fn main() -> Result<(), pdfcraft_orgchart::Error> {
    let font = Font::new(std::fs::read("face.ttf").expect("font"))?;
    let request = ChartRequest::new(
        "root",
        vec![
            Node::new("holder", "甲股东"),
            Node::new("root", "主体公司"),
            Node::new("invest", "乙投资"),
        ],
        vec![Edge::new("holder", "root"), Edge::new("root", "invest")],
    );
    let chart = Chart::new(request)?;
    let _title = chart.title();
    let pdf = chart.to_pdf(&font)?;
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
ChartRequest ──Chart::new──► Chart
Font::new
Chart::to_pdf(&Font) ──► PDF bytes
Chart::title ──► String (you write /Info)
```

1. **Assemble** nodes, edges (`holder` holds `held`), and `ChartStyle`.
2. **Compile** the protocol and the optional `top_n` fold. This does not open a font.
3. **Draw** by measuring, paginating, and emitting PDF bytes.

Fonts: one face, shared as `Arc<Vec<u8>>` inside `Font`. `Font::new` parses owned bytes. `Font::from_shared` reuses an `Arc`. Collections are rejected.
`to_pdf` does not write document info. `chart.title()` is `{subject name}股权架构图` if you need a `/Title` string.

Deep dive: **[docs/guide.md](docs/guide.md)** (style, display rules, errors, testing).

---

## Capsule API

| Item | Role |
| --- | --- |
| `ChartRequest` / `ChartStyle` | Chart values, `top_n`, size caps |
| `Node` / `Edge` / `PaidIn` / `NodeType` | Parties, holding edges, paid-in capital |
| `Font` | One parsed face. `new` and `from_shared` return `Result` |
| `Chart` | `new` compiles. `to_pdf` draws. `title` and `subject_name` read the subject |
| `Error` / `ErrorCode` | Stable failure codes |

---

## Design constraints (v1)

- No dependency on `pdfcraft-pdf` or `pdfcraft-forms`
- No font collections; one face per call
- Page headers come from the chart role. `Chart::title` is not the page header
- Fonts are not vendored in the published package

---

## Compatibility

- **Rust**: MSRV 1.98 (edition 2021)
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
