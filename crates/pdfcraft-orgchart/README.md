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

The test font `SourceHanSerifHC-ExtraLight.ttf` is Source Han Serif HC ExtraLight. The git repository stores that file. The published crate leaves it out so the `.crate` stays under the crates.io upload limit of 10485760 bytes. Its license is the SIL Open Font License 1.1 in [tests/fixtures/fonts/OFL.txt](tests/fixtures/fonts/OFL.txt). [tests/fixtures/fonts/PROVENANCE.txt](tests/fixtures/fonts/PROVENANCE.txt) records where the file came from.
