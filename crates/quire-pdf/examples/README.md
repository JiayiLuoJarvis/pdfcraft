# Examples

## hello

Smoke demo on `DocumentSpec` → `layout_spec` → `to_pdf`. Default feature includes krilla.

```bash
cargo run --example hello
```

Writes `out/hello.pdf` (or the path in argv[1]). Glyphs must exist in the registered font. The fixture used here is Latin-only. Chinese or other missing cmap entries fail the whole document.

## bill

Service-bill layout on the same `DocumentSpec` path. Needs a CJK face (see the example header for `QUIRE_CJK_FONT` / `.fonts/`).

```bash
cargo run --example bill
```

Writes `out/bill.pdf` (or the path in argv[1]).
