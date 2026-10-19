# Examples

Companion programs for the [crate README](../README.md) and
[usage guide](../docs/guide.md). Run from the workspace root:

```bash
cargo run -p pdfcraft-pdf --example <name>
```

Output paths default under `out/` (gitignored). Pass another path as argv[1].

## hello

End-to-end smoke: sections, bands with page numbers, columns, tables, lists,
links, blanks, images, and layers → `layout_spec` → `to_pdf`.

Requires `PDFCRAFT_PDF_FONT` or `PDFCRAFT_CJK_FONT` (face must cover the demo glyphs).
Any other missing cmap entry fails the document.

## bill

Service-bill layout on the same API. Needs a full CJK face:

- env `PDFCRAFT_CJK_FONT=/path/to/font.ttf`, or
- a file under workspace `.fonts/` (see the example source header)

Without a suitable face the example exits with `MissingGlyph` rather than
emitting boxes.

## anchors

Visual check for §12.4: an external URI, in-document links onto `Block::Anchor`,
a collapsed outline (`開篇` → `付款` → back up to `簽章`), and a repeating table
header whose `CellBlock::Anchor` stays on the first placement.

```bash
cargo run -p pdfcraft-pdf --example anchors
```

Writes `out/anchors.pdf`. The program prints what to look for with `pdfinfo`,
`pdftotext`, and a reader outline sidebar. Font: `PDFCRAFT_PDF_FONT`, else
`PDFCRAFT_CJK_FONT`, else `.fonts/body.ttf`. The face must cover the CJK sample.
