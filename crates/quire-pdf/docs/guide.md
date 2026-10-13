# quire-pdf guide

How to build documents with `quire-pdf`: concepts, value types, layout rules,
errors, limits, and testing. For a one-page overview see the
[crate README](../README.md). For every public item signature see
[rustdoc](https://docs.rs/quire-pdf) (`cargo doc -p quire-pdf --open` locally).

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
use quire_pdf::spec::{Block, CellBlock, CellSpec, Resources, SectionSpec, TableSpec};
use quire_pdf::{layout_spec, to_pdf, DocumentSpec, Limits, PageSize, Result};

enum Kind { A, B }

struct Item { name: String, amount: String }
struct Contract { id: String, kind: Kind, items: Vec<Item> }

fn build(contract: &Contract, font: Vec<u8>) -> Result<Vec<u8>> {
    let mut body = vec![Block::paragraph(format!("Contract {}", contract.id))];
    match contract.kind {
        Kind::A => {
            body.push(Block::paragraph("Plan A."));
            let rows = contract
                .items
                .iter()
                .map(|item| {
                    vec![
                        CellSpec {
                            content: vec![CellBlock::Text(item.name.clone())],
                            ..CellSpec::default()
                        },
                        CellSpec {
                            content: vec![CellBlock::Text(item.amount.clone())],
                            ..CellSpec::default()
                        },
                    ]
                })
                .collect();
            body.push(Block::Table(TableSpec {
                header: vec![vec![
                    CellSpec {
                        content: vec![CellBlock::Text("Item".into())],
                        ..CellSpec::default()
                    },
                    CellSpec {
                        content: vec![CellBlock::Text("Amount".into())],
                        ..CellSpec::default()
                    },
                ]],
                body: rows,
                ..TableSpec::default()
            }));
        }
        Kind::B => {
            body.push(Block::paragraph("Plan B."));
            body.push(Block::paragraph("No detail table."));
        }
    }

    let rendered = layout_spec(DocumentSpec {
        page_size: PageSize::A4,
        resources: Resources::new(font),
        limits: Limits::default(),
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
use quire_pdf::spec::Resources;
use quire_pdf::ImageFormat;

let mut resources = Resources::new(body_font_ttf_or_otf);
resources.bold_font = Some(bold_face_bytes); // only if you use InlineRun::Bold
let logo = resources.add_image(png_bytes, ImageFormat::Png);
```

| Topic | Behavior |
| --- | --- |
| Body font | Required for any text |
| Bold font | Required when the document contains `InlineRun::Bold` |
| Missing glyph | Layout fails with `ErrorKind::MissingGlyph` — no fallback face, no `.notdef` box |
| Images | Encoded bytes + `ImageFormat` only (no HTTP) |
| Handle scope | An `ImageRef` is valid only for the `Resources` that created it |
| PNG pixel errors | Fail in `layout_spec` (`ErrorKind::Image`) |
| JPEG / GIF / WebP pixel errors | May fail in `to_pdf` (`ErrorKind::Backend`, no page attached) |

Text shaping scope: horizontal Chinese and Latin. No bidi, vertical writing,
kerning, ligatures, or justified lines in v1 (`Alignment` is Start / Center /
End only).

---

## DocumentSpec and sections

```rust
DocumentSpec {
    page_size: PageSize::A4,      // default paper for sections that omit override
    resources,
    limits: Limits::default(),
    sections: vec![SectionSpec { /* … */ }],
}
```

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
| `Styled { style, children }` | Inherit font size / color |
| `Columns(ColumnsSpec)` | Side-by-side columns (not newspaper snaking) |
| `Table(TableSpec)` | Table; may split with repeating header |
| `List(ListSpec)` | Bulleted or numbered list |
| `Image(ImageSpec)` | Raster block (atomic; not split mid-image) |
| `HLine(HLineSpec)` | Horizontal rule |

`Block::paragraph("…")` is shorthand for a single normal-face text run.

### Nesting rules (v1)

- Columns may nest stacks of blocks; stacks may contain columns again.
- Table cells may hold text, paragraphs, rules, images, spacers, and lists.
- Table cells may **not** hold nested tables or columns.
- List items may hold paragraphs, rules, spacers, and nested lists.
- List items may **not** hold tables, columns, images, or page breaks.

---

## Paragraphs, blanks, and links

```rust
use quire_pdf::spec::{Alignment, BlankSpec, BlankWidth, InlineRun, ParagraphSpec};
use quire_pdf::{Color, NonNegative, Positive};

let paragraph = ParagraphSpec {
    runs: vec![
        InlineRun::Text("Amount: ".into()),
        InlineRun::Blank(BlankSpec {
            width: BlankWidth::Em(8),
            thickness: Some(Positive::pt(0.75).unwrap()),
            color: Some(Color::rgb(0, 0, 0)),
            baseline_offset: Some(NonNegative::ZERO),
        }),
        InlineRun::Text(". See ".into()),
        InlineRun::Link {
            uri: "https://example.test/pay".into(),
            runs: vec![InlineRun::Text("payment terms".into())],
        },
        InlineRun::Text(".".into()),
    ],
    first_line_indent: NonNegative::ZERO,
    align: Alignment::Start,
    color: None,
    font_size: None,
};
```

| Topic | Rule |
| --- | --- |
| `InlineRun::Text` / `Bold` | Body / bold face |
| `InlineRun::Link` | Writes PDF link annotations over the glyph boxes; does not fetch the URI. Empty URI fails layout |
| `BlankWidth::Fixed` / `Em` / `ToEndOfLine` | Underline-style blanks in forms |
| Band / layer copy | Uses `TextRun` (resolved at paint time), not `InlineRun` |

---

## Columns

```rust
use quire_pdf::spec::{ColumnSpec, ColumnsSpec};
use quire_pdf::{NonNegative, Ratio, Track};

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
use quire_pdf::spec::{CellBlock, CellSpec, TableSpec};
use quire_pdf::{Color, NonNegative, Track};

TableSpec {
    header: vec![vec![
        CellSpec {
            content: vec![CellBlock::Text("Name".into())],
            ..CellSpec::default()
        },
        CellSpec {
            content: vec![CellBlock::Text("Qty".into())],
            col_span: NonZeroU16::new(2).unwrap(),
            ..CellSpec::default()
        },
    ]],
    body: vec![/* rows of CellSpec */],
    repeat_header: true, // default
    tracks: Some(vec![Track::Auto, Track::Ratio(quire_pdf::Ratio::new(1.0).unwrap())]),
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
use quire_pdf::spec::{ListItemSpec, ListKind, ListSpec};
use quire_pdf::Positive;

ListSpec {
    kind: ListKind::Decimal, // also bullet / alpha / roman helpers on the enum
    start: NonZeroU32::new(1).unwrap(),
    marker_width: Positive::pt(18.0).unwrap(),
    nest_indent: None, // defaults to marker_width
    items: vec![
        ListItemSpec {
            marker: None, // automatic label
            blocks: vec![Block::paragraph("First")],
        },
        ListItemSpec {
            marker: Some("*".into()), // custom; does not advance the counter
            blocks: vec![Block::paragraph("Aside")],
        },
    ],
}
```

Markers appear on the first fragment of an item; continuations keep the hanging
indent without reprinting the marker. Automatic outline numbering (`1.1.1`) is
not provided — nest lists instead.

---

## Header, footer, and page numbers

```rust
use quire_pdf::spec::{BandInset, BandSlot, BandSpec, TextRun};

BandSpec {
    inset: BandInset::PageMargins, // or BandInset::Fixed { left, right }
    left: Some(BandSlot::Text(vec![TextRun::Literal("Acme Co.".into())])),
    right: Some(BandSlot::Text(vec![
        TextRun::Literal("Page ".into()),
        TextRun::PageNumber,
        TextRun::Literal(" / ".into()),
        TextRun::PageCount,
    ])),
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

---

## Layers (`behind` / `front`)

Use `LayerMark` for page-anchored content (seals, watermarks, paths):

| Variant | Notes |
| --- | --- |
| `Text` | One unwrapped line; forced breaks error |
| `Image` | Stretched into a page `Rect` |
| `Line` / `Rect` / `Ellipse` | Geometry + pen / ink |
| `Rotated` | Group of `LayerShape` around a pivot |

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
use quire_pdf::canvas::pt_near;
use quire_pdf::{layout_spec, CanvasCommand, DocumentSpec /* … */};

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
cargo run -p quire-pdf --example hello   # writes out/hello.pdf
cargo run -p quire-pdf --example bill    # CJK: set QUIRE_CJK_FONT or use .fonts/
```

See [examples/README.md](../examples/README.md).

---

## What this crate does not do

- Bidirectional text, vertical writing, kerning, ligatures, full UAX #14
- Justified lines
- TOC generation, QR encoding, SVG parsing, in-flow vector canvas, encryption
- Fetching images over HTTP
- Silent missing-glyph substitution
- A second PDF backend
- Filling existing AcroForm PDFs (see workspace crate `quire-forms`)
- Exposing an implementable `Flowable` trait to callers

---

## Crate map

| Module | Public role |
| --- | --- |
| `spec` | `DocumentSpec`, blocks, bands, layers, `layout_spec`, `Rendered` |
| `geom` | Lengths, pages, colors, paths, margins |
| `error` | `Error`, `ErrorKind`, `Errors`, `Location` |
| `limits` | Batch caps |
| `canvas` | `Canvas`, `RecordingCanvas`, image formats (also re-exported) |
| `fonts` | `FontId` (measurement store is internal) |
| crate root | `to_pdf`, common re-exports |

Layout and text engines are intentionally **not** public.
