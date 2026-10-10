# Examples

## hello

Minimal `document` → `to_pdf` path. Default feature includes krilla.

```bash
cargo run --example hello
```

Writes `hello.pdf` in the current directory. Glyphs must exist in the registered font. The fixture used here is Latin-only. Chinese or other missing cmap entries fail the whole document.
