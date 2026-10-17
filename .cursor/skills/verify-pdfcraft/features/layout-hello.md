# Layout hello

The `hello` example builds a multi-page document from `DocumentSpec` (bands, lists, columns, tables, links, images) and writes a vector PDF a caller can open.

## Sub-features

- `hello-run` compiles and runs `pdfcraft-pdf`'s `hello` example.
- `hello-multipage` emits at least two pages (table overflow + page break).
- `hello-title` embeds extractable title text `Quire layout tour`.

## How to get to it (user POV)

- From the workspace README / `crates/pdfcraft-pdf/examples/README.md`: set a covering font, then `cargo run -p pdfcraft-pdf --example hello`.
- Optionally pass an output path as argv[1] (default `out/hello.pdf`).

## Driving it with shell+Cargo+poppler

Preconditions:

- `doctor.sh` and `launch.sh` succeeded for this `VERIFY_RUN_ID`.
- Layout font covers ASCII and `第` / `页` / `共` (CJK face preferred).

- **Run example.** Execute `.cursor/skills/verify-pdfcraft/scripts/drive-layout-hello.sh` (or `cargo run -p pdfcraft-pdf --example hello -- <scratch>/hello.pdf`). Exit code `0` and stderr contains `wrote … bytes`.
- **Observe file.** `pdfinfo` on the proof PDF reports `Pages:` ≥ 2; file starts with `%PDF-`.
- **Observe text.** `pdftotext -layout` contains `Quire layout tour`.
- **Proof.** Keep `proof/layout-hello/hello.pdf`, `observe.txt`, and `layout-hello.action.log` under `evidence/<RUN_ID>/`.

## Gotchas

- Without `PDFCRAFT_PDF_FONT` / `PDFCRAFT_CJK_FONT` (or `.fonts/body.ttf`), the example panics before writing.
- DejaVu-only faces fail on Chinese header glyphs (`MissingGlyph`) — that is a real failure, not a skip.
- Do not assert krilla content-stream bytes; magic + pages + `pdftotext` are the user-visible proof.
