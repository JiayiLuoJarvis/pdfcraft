# pdfcraft-pdf guide

How to build documents with `pdfcraft-pdf`: concepts, value types, layout rules,
errors, limits, and testing.

| Doc | Role |
| --- | --- |
| [crate README](../README.md) | Install and quick start |
| This guide | How-to for real documents |
| [migrate-inline.md](migrate-inline.md) | Required edits from `FontFamily.normal` / `InlineRun::Bold` / `TextStyle` |
| [rustdoc](https://docs.rs/pdfcraft-pdf) | Every public signature (`cargo doc -p pdfcraft-pdf --open`) |

Related crates (separate packages, separate docs):

- [`pdfcraft-forms`](https://docs.rs/pdfcraft-forms) fills existing AcroForm PDFs.
- [`pdfcraft-orgchart`](https://docs.rs/pdfcraft-orgchart) draws equity charts.

---

## Mental model

You do **not** hand the library a template engine, JSON schema, or layout
closures. You build ordinary Rust values, then call two functions:

```text
Resources + DocumentSpec  →  layout_spec  →  Rendered  →  to_pdf  →  PDF bytes
```

Two moments matter:

1. **Assemble time.** Your application already has data. Use Rust `if` / `for`
   to push `Block`s into `SectionSpec.body`. You do not know the page count yet.
2. **Paint time.** After pagination, the library paints each page. Header,
   footer, and layer marks may contain `TextRun` / `PageWhen` values; the
   library substitutes page numbers and last-page visibility then.

```rust
use pdfcraft_pdf::spec::{
    Block, CellBlock, CellSpec, DocumentInfo, FontFace, FontFamily, FontRegistry, Resources,
    SectionSpec, TableSpec,
};
use pdfcraft_pdf::{layout_spec, to_pdf, DocumentSpec, Limits, PageSize, Result};

enum Kind { A, B }

struct Item { name: String, amount: String }
struct Contract { id: String, kind: Kind, items: Vec<Item> }

fn build(contract: &Contract, font: Vec<u8>) -> Result<Vec<u8>> {
    let resources = Resources::with_fonts(FontRegistry::single(FontFamily {
        regular: FontFace::new(font, 0),
        bold: None,
        italic: None,
        bold_italic: None,
    }));
    let mut body = vec![Block::paragraph((), format!("Contract {}", contract.id))];
    match contract.kind {
        Kind::A => {
            body.push(Block::paragraph((), "Plan A."));
            let rows = contract
                .items
                .iter()
                .map(|item| {
                    vec![
                        CellSpec {
                            content: vec![CellBlock::Text {
                                text: item.name.clone(),
                                font: (),
                            }],
                            ..CellSpec::default()
                        },
                        CellSpec {
                            content: vec![CellBlock::Text {
                                text: item.amount.clone(),
                                font: (),
                            }],
                            ..CellSpec::default()
                        },
                    ]
                })
                .collect();
            body.push(Block::Table(TableSpec {
                header: vec![vec![
                    CellSpec {
                        content: vec![CellBlock::Text {
                            text: "Item".into(),
                            font: (),
                        }],
                        ..CellSpec::default()
                    },
                    CellSpec {
                        content: vec![CellBlock::Text {
                            text: "Amount".into(),
                            font: (),
                        }],
                        ..CellSpec::default()
                    },
                ]],
                body: rows,
                ..TableSpec::default()
            }));
        }
        Kind::B => {
            body.push(Block::paragraph((), "Plan B."));
            body.push(Block::paragraph((), "No detail table."));
        }
    }

    let rendered = layout_spec(DocumentSpec {
        page_size: PageSize::A4,
        resources,
        limits: Limits::default(),
        info: DocumentInfo::default(),
        sections: vec![SectionSpec {
            body,
            ..SectionSpec::default()
        }],
    })?;
    to_pdf(rendered)
}
```

---

## Coordinates and lengths

| Rule | Detail |
| --- | --- |
| Unit | PDF point (`1/72` inch). `MM_TO_PT` converts millimeters. |
| Origin | Top-left of the page |
| Axes | `x` right, `y` **down** |
| Brands | `Length<Finite>`, `Length<NonNegative>`, `Length<Positive>` |

Invalid numbers fail construction (`ErrorKind::InvalidGeometry`) instead of
becoming silent NaNs. Prefer `Positive::pt`, `NonNegative::pt`,
`Margins::try_uniform`, and related helpers.

**Default margins are zero on all four sides.** Top/bottom margin height is also
the header/footer band height. Set margins yourself (or use `MarginPreset::Inch`
/ `HalfInch`).

---

## Resources

```rust
use pdfcraft_pdf::spec::{FontFace, FontFamily, FontRegistry, Resources};
use pdfcraft_pdf::ImageFormat;

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
enum DocFont { Body, Cjk }

let mut fonts = FontRegistry::new();
fonts.register(DocFont::Body, FontFamily {
    regular: FontFace::new(body_ttf, 0),
    bold: Some(FontFace::new(bold_ttf, 0)),
    italic: Some(FontFace::new(italic_ttf, 0)),
    bold_italic: None,
});
fonts.register(DocFont::Cjk, FontFamily {
    regular: FontFace::new(cjk_ttf, 0),
    bold: None,
    italic: None,
    bold_italic: None,
});
let mut resources = Resources::with_fonts(fonts);
let logo = resources.add_image(png_bytes, ImageFormat::Png);
```

A document that uses one family can skip the enum. `FontRegistry::single` registers that family under `()`.

```rust
let resources = Resources::with_fonts(FontRegistry::single(FontFamily {
    regular: FontFace::new(body_ttf, 0),
    bold: None,
    italic: None,
    bold_italic: None,
}));
```

| Topic | Behavior |
| --- | --- |
| Font key | Your type `K` (`Copy + Eq + Hash + Debug`). Runs store `K`. |
| Registry | `FontRegistry::register`. Registering the same key again replaces that family. |
| One family | `FontRegistry::single` uses the key `()`. |
| Collection index | `FontFace.index` selects a face inside a collection. `0` is the first face. |
| Face bytes | `FontFace.bytes` is `Arc<Vec<u8>>`. `FontFace::new` wraps owned bytes. `FontFace::from_shared` reuses an `Arc`. |
| Faces | `RunStyle.bold` needs `FontFamily.bold`, `italic` needs `FontFamily.italic`, and both together need `FontFamily.bold_italic`. A missing face records `ErrorKind::BoldFaceRequired`, `ItalicFaceRequired`, or `BoldItalicFaceRequired`. There is no synthetic bold or slant. |
| Decorations | Underline position comes from the regular face's `post` table and strikethrough position from its `OS/2` table. Both strokes are 1 pt thick. A face without a finite offset fails with `ErrorKind::MissingDecorationMetrics` when a run asks for that line. |
| Unknown key | `ErrorKind::UnregisteredFont`. The document fails. |
| Missing glyph | `ErrorKind::MissingGlyph`. There is no fallback face and no `.notdef` box. |
| Empty registry | Valid when the document has no text. |
| Images | Encoded bytes + `ImageFormat` only (no HTTP) |
| Handle scope | An `ImageRef` is valid only for the `Resources` that created it |
| PNG pixel errors | Fail in `layout_spec` (`ErrorKind::Image`) |
| JPEG / GIF / WebP pixel errors | May fail in `to_pdf` (`ErrorKind::Backend`, no page attached) |

Each run names its face. The library does not pick a face from the script of the text. List markers use the first face that registered successfully.

Text shaping scope: horizontal Chinese and Latin. No bidi, vertical writing,
kerning, or ligatures. `Alignment` is Start / Center / End / Justify.

---

## DocumentSpec and sections

```rust
DocumentSpec {
    page_size: PageSize::A4,      // default paper for sections that omit override
    resources,
    limits: Limits::default(),
    info: DocumentInfo::default(),
    sections: vec![SectionSpec { /* … */ }],
}
```

`info` is `DocumentInfo`. `to_pdf` writes a non-empty `title` and `author` into the PDF information dictionary. Empty strings are omitted. The creator is `pdfcraft`. The library does not set a producer, and it does not take a custom creator.

`pdfinfo` prints those strings on the `Title` and `Author` lines.

| `SectionSpec` field | Meaning |
| --- | --- |
| `page_size` | `Some(…)` overrides the document default for this section |
| `margins` | Body inset; top/bottom = band height |
| `header` / `footer` | Optional `BandSpec` |
| `behind` / `front` | Page-fixed `LayerMark` lists |
| `body` | Flow content (`Vec<Block>`) |

Each section starts on a new page. An empty `sections` vector still yields one
default empty section.

Paper: `PageSize::Paper(Paper, Orientation)` (A0–A5, Letter, …) or
`PageSize::Custom(Size)`. Constants such as `PageSize::A4` and `LETTER` are
available.

---

## Flow blocks

| `Block` | Role |
| --- | --- |
| `Paragraph(ParagraphSpec)` | Wrapping paragraph |
| `Spacer(Length<NonNegative>)` | Vertical gap (may be `0`) |
| `PageBreak` | Force a new page (a leading break is a no-op) |
| `KeepTogether(Vec<Block>)` | Children must fit on one page |
| `Styled { style, children }` | Children inherit a `RunStyle` (face, lines, tracking, size, color) |
| `Columns(ColumnsSpec)` | Side-by-side columns (not newspaper snaking) |
| `Table(TableSpec)` | Table; may split with repeating header |
| `List(ListSpec)` | Bulleted or numbered list |
| `Image(ImageSpec)` | Raster block (atomic; not split mid-image) |
| `HLine(HLineSpec)` | Horizontal rule |

`Block::paragraph(font, "…")` is shorthand for one `InlineRun::Text` on that font key. `ParagraphSpec::text` is the same run inside a paragraph value.

### Nesting rules (v1)

- Columns may nest stacks of blocks; stacks may contain columns again.
- Table cells may hold text, paragraphs, rules, images, spacers, and lists.
- Table cells may **not** hold nested tables or columns.
- List items may hold paragraphs, rules, spacers, and nested lists.
- List items may **not** hold tables, columns, images, or page breaks.

---

## Paragraphs, blanks, and links

```rust
use pdfcraft_pdf::spec::{Alignment, BlankSpec, BlankWidth, InlineRun, ParagraphSpec, RunStyle};
use pdfcraft_pdf::{Color, NonNegative, Positive};

let paragraph = ParagraphSpec {
    runs: vec![
        InlineRun::Text {
            text: "Amount: ".into(),
            font: (),
            style: RunStyle { bold: Some(true), ..RunStyle::default() },
        },
        InlineRun::Blank(BlankSpec {
            width: BlankWidth::Em(8),
            thickness: Some(Positive::pt(0.75).unwrap()),
            color: Some(Color::rgb(0, 0, 0)),
            baseline_offset: Some(NonNegative::ZERO),
        }),
        InlineRun::Text { text: ". See ".into(), font: (), style: RunStyle::default() },
        InlineRun::Link {
            target: pdfcraft_pdf::LinkTarget::uri("https://example.test/pay").unwrap(),
            runs: vec![InlineRun::Text {
                text: "payment terms".into(),
                font: (),
                style: RunStyle { underline: Some(true), ..RunStyle::default() },
            }],
        },
        InlineRun::Text { text: ".".into(), font: (), style: RunStyle::default() },
    ],
    first_line_indent: NonNegative::ZERO,
    align: Alignment::Start,
    color: None,
    font_size: None,
    line_height_factor: None,
};
```

| Topic | Rule |
| --- | --- |
| `InlineRun::Text` | `{ text, font, style }`. `RunStyle` fields left `None` inherit. `Some(false)` turns off an inherited flag. |
| Inheritance | Seed (12 pt, black, regular, no lines, no tracking), then each `Block::Styled`, then `ParagraphSpec` size and color, then the run. Bands, layers, and canvas text start from the seed. Table cell text and list markers use the regular face and inherit only size and color. |
| Underline / strikethrough | Position scales with the run's size. Thickness is 1 pt. Adjacent runs on one line with the same size and color share one stroke, placed from the first segment. Spaces inside the line are covered. A blank keeps its own stroke. |
| `tracking` | Extra space after each cluster followed by more text on the line. It counts toward line breaking. Negative values may not push a glyph's advance below zero (`ErrorKind::NegativeTracking`). |
| Mixed sizes | Each face on a line is scaled by its own size. The line box takes the largest ascent, the deepest descent, and the largest leading, then the larger of that natural height and the largest size times the line-height factor. A line of only blanks uses the first text run's resolved face at the paragraph size. A paragraph with no text runs does not read a font. |
| `Alignment::Justify` | Placement only; line breaking is unchanged. Phase 1 widens U+0020 spaces up to 0.5em of that space's run size. Phase 2 averages leftover into eligible cluster gaps (Han / fullwidth punctuation; CJK–Latin boundaries yes; Latin word interiors no). The last paragraph line, a hard-break line, a `BlankWidth::ToEndOfLine` line, a line with no stretch slots, and a non-positive remainder stay Start-shaped. `ImageSpec` / `HLineSpec` / `CanvasSpec` treat `Justify` as Start. |
| `InlineRun::Link` | `target: LinkTarget` is `Uri` or `Anchor`. Layout resolves `Anchor` to page-local `Xyz` before paint. Empty URI → `EmptyLink`. Empty anchor id → `EmptyAnchorId`. |
| `Block::Anchor` | `{ id, outline, child }`. Destination is the first placed fragment of `child` (top-left, y down). Optional `OutlineEntry { title, level }` builds the bookmark tree. |
| Outline levels | First outline item must be level `1`. Later items deepen by at most +1; jumping up (e.g. 3→1) is allowed. Level `0` and empty titles fail. No outline entries → `to_pdf` does not write an outline. |
| `BlankWidth::Fixed` / `Em` / `ToEndOfLine` | Underline-style blanks in forms |
| Band / layer copy | Uses `TextRun` (resolved at paint time), not `InlineRun`. `BandSlot::Link` stays URI-only. |

---

## Columns

```rust
use pdfcraft_pdf::spec::{ColumnSpec, ColumnsSpec};
use pdfcraft_pdf::{NonNegative, Ratio, Track};

ColumnsSpec {
    gap: NonNegative::pt(12.0).unwrap(),
    columns: vec![
        ColumnSpec {
            width: Track::Ratio(Ratio::new(1.0).unwrap()),
            blocks: vec![/* … */],
        },
        ColumnSpec {
            width: Track::Ratio(Ratio::new(2.0).unwrap()),
            blocks: vec![/* … */],
        },
    ],
}
```

`Track` variants: `Fixed`, `Ratio`, `Auto`.

- At most one `Auto`.
- Do not mix `Auto` with `Ratio` (`ErrorKind::ColumnWidths`).
- `Ratio::new` rejects non-finite or non-positive weights.

---

## Tables

```rust
use std::num::NonZeroU16;
use pdfcraft_pdf::spec::{CellBlock, CellSpec, TableSpec};
use pdfcraft_pdf::{Color, NonNegative, Track};

TableSpec {
    header: vec![vec![
        CellSpec {
            content: vec![CellBlock::Text { text: "Name".into(), font: () }],
            ..CellSpec::default()
        },
        CellSpec {
            content: vec![CellBlock::Text { text: "Qty".into(), font: () }],
            col_span: NonZeroU16::new(2).unwrap(),
            ..CellSpec::default()
        },
    ]],
    body: vec![/* rows of CellSpec */],
    repeat_header: true, // default
    tracks: Some(vec![Track::Auto, Track::Ratio(pdfcraft_pdf::Ratio::new(1.0).unwrap())]),
    cell_padding: NonNegative::pt(4.0).unwrap(),
    border_width: NonNegative::pt(0.5).unwrap(),
    border_color: Color::rgb(0, 0, 0),
}
```

Tables wider than the frame, or taller than a full body frame when unsplittable
pieces cannot fit, fail with a specific `ErrorKind` rather than shrinking
silently.

---

## Lists

```rust
use std::num::NonZeroU32;
use pdfcraft_pdf::spec::{ListItemSpec, ListKind, ListSpec};
use pdfcraft_pdf::Positive;

ListSpec {
    kind: ListKind::Decimal, // also bullet / alpha / roman helpers on the enum
    start: NonZeroU32::new(1).unwrap(),
    marker_width: Positive::pt(18.0).unwrap(),
    nest_indent: None, // defaults to marker_width
    items: vec![
        ListItemSpec {
            marker: None, // automatic label
            blocks: vec![Block::paragraph((), "First")],
        },
        ListItemSpec {
            marker: Some("*".into()), // custom; does not advance the counter
            blocks: vec![Block::paragraph((), "Aside")],
        },
    ],
}
```

Markers appear on the first fragment of an item. Continuations keep the hanging
indent and do not reprint the marker. Generated and custom markers use the first
face that registered successfully. Automatic outline numbering (`1.1.1`) is
not provided. Nest lists instead.

---

## Header, footer, and page numbers

```rust
use pdfcraft_pdf::spec::{BandInset, BandSlot, BandSpec, RunStyle, TextRun};

BandSpec {
    inset: BandInset::PageMargins, // or BandInset::Fixed { left, right }
    left: Some(BandSlot::Text {
        runs: vec![TextRun::Literal("Acme Co.".into())],
        font: (),
        style: RunStyle { bold: Some(true), ..RunStyle::default() },
    }),
    right: Some(BandSlot::Text {
        runs: vec![
            TextRun::Literal("Page ".into()),
            TextRun::PageNumber,
            TextRun::Literal(" / ".into()),
            TextRun::PageCount,
        ],
        font: (),
        style: RunStyle::default(),
    }),
}
```

| `TextRun` | Value |
| --- | --- |
| `Literal` | Fixed string |
| `PageNumber` / `PageCount` | Document page (1-based) |
| `SectionNumber` / `SectionCount` | Page within the current section |

Bands span the full page width unless inset. Content taller than the margin
band fails with `ErrorKind::DecorationTallerThanMargin` before any page starts.
`BandSlot::Link` builds the URI from `TextRun`s at paint time the same way.
Band, layer, and canvas text take a `RunStyle` that starts from the seed, not from any enclosing block.

---

## Layers (`behind` / `front`)

Use `LayerMark` for page-anchored content (seals, watermarks, paths):

| Variant | Notes |
| --- | --- |
| `Text` | One unwrapped line on a font key. Forced breaks error. |
| `Image` | Stretched into a page `Rect` |
| `Line` / `Rect` / `Ellipse` | Geometry + pen / ink |
| `Rotated` | Group of `LayerShape<K>` around a pivot; may mix path, `Text`, and `Image` |

`PageWhen::All` — every page of the section.  
`PageWhen::Last` — only the last page of the **whole document**.

Paint order: `behind` → header → body → footer → `front`.

---

## Limits

`Limits::default()` caps batch work. Setters reject `0` and values above four
times the default.

| Cap | Default |
| --- | --- |
| Font bytes | 32 MiB |
| Image bytes | 32 MiB |
| Image pixels | 40_000_000 |
| Longer image edge | 8192 |
| Pages | 10_000 |
| Body blocks | 100_000 |
| Table cells | 50_000 |
| Paragraph UTF-8 bytes | 1_000_000 |

Overflow → `ErrorKind::LimitExceeded` + `LimitKind`.

---

## Errors

`layout_spec` and `to_pdf` return `Result<T, Errors>`:

- Up to **64** failures, discovery order
- Inspect with `first()`, `as_slice()`, `iter()`, `truncated()`
- Each `Error` has `kind: ErrorKind` and optional `at: Location` (section, page,
  region, cell)
- Underlying skrifa/krilla failures: `std::error::Error::source()`
- Equality compares `kind` + `at` only (tests need not rebuild dependency errors)

Any error fails the document; there is no “partial PDF on failure” path.

---

## Testing layout

Assert on the recording canvas, not on compressed PDF operators:

```rust
use pdfcraft_pdf::canvas::pt_near;
use pdfcraft_pdf::{layout_spec, CanvasCommand, DocumentSpec /* … */};

let rendered = layout_spec(spec)?;
let pages = rendered.pages();
// match CanvasCommand::GlyphRun / Image / … and compare with pt_near
assert!(pt_near(x, expected_x));
```

`canvas::pt_near` is available without extra features. Feature `test-utils`
re-exports the same function at the crate root. For PDF smoke tests, checking
that bytes start with `%PDF` is enough.

---

## Features

| Feature | Default | Purpose |
| --- | --- | --- |
| `backend-krilla` | yes | Private PDF backend and `to_pdf` |
| `test-utils` | no | `pt_near`, `PT_ABS_TOLERANCE` |

Without `backend-krilla`, layout and recording still compile; image registration
rejects draws, and `to_pdf` is unavailable.

---

## Examples

```bash
cargo run -p pdfcraft-pdf --example hello   # writes out/hello.pdf
cargo run -p pdfcraft-pdf --example bill    # CJK: set PDFCRAFT_CJK_FONT or use .fonts/
cargo run -p pdfcraft-pdf --example anchors # §12.4 links and outline → out/anchors.pdf
```

See [examples/README.md](../examples/README.md).

---

## What this crate does not do

- Bidirectional text, vertical writing, kerning, ligatures, full UAX #14
- Punctuation compression / hanging punctuation for Justify
- TOC generation, QR encoding, SVG parsing, in-flow vector canvas, encryption
- Fetching images over HTTP
- Silent missing-glyph substitution
- A second PDF backend
- Filling existing AcroForm PDFs (see workspace crate `pdfcraft-forms`)
- Exposing an implementable `Flowable` trait to callers
- An implementable drawing surface. `layout_spec` and `to_pdf` do not take one

---

## Crate map

| Module | Public role |
| --- | --- |
| `spec` | `DocumentSpec`, blocks, bands, layers, `layout_spec`, `Rendered` |
| `geom` | Lengths, pages, colors, paths, margins |
| `error` | `Error`, `ErrorKind`, `Errors`, `Location` |
| `limits` | Batch caps |
| `canvas` | Recorded `CanvasCommand` and `RecordingCanvas`, image formats (also re-exported). Not a backend you implement |
| `fonts` | `FontId` (measurement store is internal) |
| crate root | `to_pdf`, common re-exports |

Layout and text engines are intentionally **not** public.
