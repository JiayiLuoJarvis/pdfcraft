# quire-forms

`quire-forms` lists AcroForm widgets in an existing PDF, fills them, and flattens the result. `list_fields` returns one value per widget. `fill_and_flatten` draws the values and removes the form. PDF reading uses lopdf. Text measurement uses skrifa.

The behavior spec is [docs/quire-forms-spec.md](../../docs/quire-forms-spec.md).

## Install

```toml
[dependencies]
quire-forms = "0.1.0"
```

## Status

The version is `0.1.0`. The API can break before `1.0`.

## License

Licensed under either of

- Apache License, Version 2.0, [LICENSE-APACHE](LICENSE-APACHE)
- MIT license, [LICENSE-MIT](LICENSE-MIT)

at your option.

[tests/fixtures/README.md](tests/fixtures/README.md) records which fixtures the package ships.
