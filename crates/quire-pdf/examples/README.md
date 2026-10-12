# Examples

Companion programs for the [crate README](../README.md) and
[usage guide](../docs/guide.md). Run from the workspace root:

```bash
cargo run -p quire-pdf --example <name>
```

Output paths default under `out/` (gitignored). Pass another path as argv[1].

## hello

End-to-end smoke: sections, bands with page numbers, columns, tables, lists,
links, blanks, images, and layers → `layout_spec` → `to_pdf`.

Uses `tests/fixtures/quire-measure.ttf` (Latin + a few CJK page-label glyphs).
Any other missing cmap entry fails the document.

## bill

Service-bill layout on the same API. Needs a full CJK face:

- env `QUIRE_CJK_FONT=/path/to/font.ttf`, or
- a file under workspace `.fonts/` (see the example source header)

Without a suitable face the example exits with `MissingGlyph` rather than
emitting boxes.
