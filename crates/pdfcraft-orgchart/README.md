# pdfcraft-orgchart

`pdfcraft-orgchart` draws equity and outbound-investment charts to PDF (shareholders above the subject, investments below). Callers pass the face as `FontBytes`. `validate_request` checks a `ChartRequest`. `generate_pdf` draws the chart. Measurement uses skrifa, and drawing uses krilla. The crate does not depend on `pdfcraft-pdf` or `pdfcraft-forms`.

The behavior spec is [docs/pdfcraft-orgchart-spec.md](../../docs/pdfcraft-orgchart-spec.md).

## Install

```toml
[dependencies]
pdfcraft-orgchart = "0.1.0"
```

## Status

The version is `0.1.0`. The API can break before `1.0`.

## License

Licensed under either of

- Apache License, Version 2.0, [LICENSE-APACHE](LICENSE-APACHE)
- MIT license, [LICENSE-MIT](LICENSE-MIT)

at your option.

Tests and the `equity_chart` example need a single-face font from the environment: set `PDFCRAFT_ORGCHART_FONT` or `PDFCRAFT_CJK_FONT`. Font files are not vendored in git. OFL notes for a common Source Han reference face are in [tests/fixtures/fonts/](tests/fixtures/fonts/).
