# pdfcraft-pdf guide

How to build documents with `pdfcraft-pdf`: concepts, value types, layout rules,
errors, limits, and testing.

| Doc | Role |
| --- | --- |
| [crate README](../README.md) | Install and quick start |
| This guide | How-to for real documents |
| [paragraphs.md](paragraphs.md) | Caller guide for paragraphs and spans (Chinese, with examples) |
| [tables.md](tables.md) | Caller guide for tables and cells (Chinese, with examples) |
| [lists.md](lists.md) | Caller guide for lists and insets (Chinese, with examples) |
| [migrate-0.6.md](migrate-0.6.md) | Field map from `0.5` to `0.6` |
| [migrate-inline.md](migrate-inline.md) | Earlier step from `FontFamily.normal` / `InlineRun::Bold` / `TextStyle` |
| [migrate-table-row.md](migrate-table-row.md) | Earlier step from `header: Vec<Vec<CellSpec>>` / `border_width` + `border_color` |
| [migrate-band-marks.md](migrate-band-marks.md) | Earlier step that moved page-coordinate footer marks into a footer band |
| [rustdoc](https://docs.rs/pdfcraft-pdf) | Every public signature (`cargo doc -p pdfcraft-pdf --open`) |

Related crates (separate packages, separate docs):

- [`pdfcraft-forms`](https://docs.rs/pdfcraft-forms) fills existing AcroForm PDFs.
- [`pdfcraft-orgchart`](https://docs.rs/pdfcraft-orgchart) draws equity charts.

---

## Mental model

You do **not** hand the library a template engine or a JSON schema. Body
blocks are ordinary Rust values. A header or footer may be `BandFn`. You then
call two functions:

```text
Resources + DocumentSpec  →  layout_spec  →  Rendered  →  to_pdf  →  PDF bytes
```

Two moments matter:

1. **Assemble time.** Your application already has data. Use Rust `if` / `for`
   to push `Block`s into `SectionSpec.body`. You do not know the page count yet.
2. **Paint time.** After pagination, the library paints each page. It calls
   the header function, then the footer function, and fills `PageText` page
   counters. `PageWhen` still chooses layer marks only.

```rust
use pdfcraft_pdf::spec::{
    Block, CellSpec, DocumentInfo, FontFace, FontFamily, FontRegistry, Resources, SectionSpec,
    TableHeader, TableRow, TableSpec,
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
                    TableRow {
                        children: vec![
                            CellSpec {
                                children: vec![Block::paragraph((), item.name.clone())],
                                ..CellSpec::default()
                            },
                            CellSpec {
                                children: vec![Block::paragraph((), item.amount.clone())],
                                ..CellSpec::default()
                            },
                        ],
                    }
                })
                .collect();
            body.push(Block::Table(TableSpec {
                header: Some(TableHeader {
                    rows: vec![TableRow {
                        children: vec![
                            CellSpec {
                                children: vec![Block::paragraph((), "Item")],
                                ..CellSpec::default()
                            },
                            CellSpec {
                                children: vec![Block::paragraph((), "Amount")],
                                ..CellSpec::default()
                            },
                        ],
                    }],
                    ..TableHeader::default()
                }),
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

Each run names its face. The library does not pick a face from the script of the text. List markers use `marker_font` when it is set, and otherwise the first face that registered successfully. They use that family's regular face.

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
| `style.page_size` | `Some(…)` overrides the document default for this section |
| `style.margins` | Body inset. Top and bottom margins are the band heights |
| `header` / `footer` | Optional `BandFn` (`fixed` or per page) |
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
| `KeepTogether { children }` | Children must fit on one page |
| `Styled { style, children }` | Children inherit a `RunStyle` (face, lines, tracking, size, color) |
| `Columns(ColumnsSpec)` | Side-by-side columns (not newspaper snaking) |
| `Table(TableSpec)` | Table. May split with a repeating header |
| `List(ListSpec)` | Bulleted or numbered list |
| `Inset { style, children }` | Shift children right by `style.left` and narrow the frame by `left + right` |
| `Image(ImageSpec)` | Raster block (atomic, not split mid-image) |
| `Canvas(CanvasSpec)` | Fixed-size vector block in the flow |
| `HLine(HLineStyle)` | Horizontal rule. Height is `thickness` |

`Block::paragraph(font, "…")` is shorthand for one `Span::Text` on that font key. `ParagraphSpec::text` is the same span inside a paragraph value.

`Block::Canvas` occupies `CanvasSpec.size` and does not split. Marks use block coordinates, origin at the top-left, y down. `CanvasStyle.align` places the box in the frame. `Alignment::Justify` is treated as `Start`. Page-fixed marks stay on `LayerMark`, not on this block.

### Nesting rules (v1)

- Columns may nest stacks of blocks; stacks may contain columns again.
- Table cells may hold paragraphs, rules, images, canvases, spacers, lists, insets, and `Styled` groups. Paragraphs in a cell inherit every field of an enclosing `Styled`.
- Table cells may **not** hold nested tables, columns, keep-together groups, or page breaks.
  This also applies inside each item of a list in a cell.
- List items may hold every block except page breaks: paragraphs, tables,
  columns, images, canvases, insets, keep-together groups, and nested lists.
- A rejected block reports the innermost container that forbids it. A page
  break in a list item in a cell reports `"list item"`. Columns there report
  `"table cell"`.
- `Inset` draws nothing and adds no vertical space. `style.left` and `style.right` are
  non-negative. `left + right` greater than or equal to the frame being laid
  out fails with `GeometryKind::InsetWiderThanFrame`. A child that is still
  too wide for the narrowed frame reports its own error, such as
  `UnbreakableWiderThanFrame` for an image. An inset counts as one nesting
  level and one block. A list marker aligned to an inset uses the child's
  first baseline. The sideways shift does not move it. Header and footer
  bands are not built from `Block`, so they have no inset.

---

## Paragraphs, blanks, and links

Caller-oriented recipes (Chinese, with fuller examples): [paragraphs.md](paragraphs.md).

```rust
use pdfcraft_pdf::spec::{
    Alignment, BlankSpec, BlankWidth, ParagraphSpec, ParagraphStyle, RunStyle, Span,
};
use pdfcraft_pdf::{Color, NonNegative, Positive};

let paragraph = ParagraphSpec {
    style: ParagraphStyle {
        align: Alignment::Start,
        ..ParagraphStyle::default()
    },
    children: vec![
        Span::Text {
            text: "Amount: ".into(),
            font: (),
            style: RunStyle { bold: Some(true), ..RunStyle::default() },
        },
        Span::Blank(BlankSpec {
            width: BlankWidth::Em(8),
            thickness: Some(Positive::pt(0.75).unwrap()),
            color: Some(Color::rgb(0, 0, 0)),
            baseline_offset: Some(NonNegative::ZERO),
        }),
        Span::Text { text: ". See ".into(), font: (), style: RunStyle::default() },
        Span::Link {
            target: pdfcraft_pdf::LinkTarget::uri("https://example.test/pay").unwrap(),
            children: vec![Span::Text {
                text: "payment terms".into(),
                font: (),
                style: RunStyle { underline: Some(true), ..RunStyle::default() },
            }],
        },
        Span::Text { text: ".".into(), font: (), style: RunStyle::default() },
    ],
};
```

| Topic | Rule |
| --- | --- |
| `Span::Text` | `{ text, font, style }`. `RunStyle` fields left `None` inherit. `Some(false)` turns off an inherited flag. |
| Inheritance | Seed (12 pt, black, regular, no lines, no tracking), then each `Block::Styled`, then `ParagraphStyle.text` (every `RunStyle` field), then the span. Paragraphs inside table cells inherit every enclosing `Styled` field. List markers inherit only size and color, and stay on the regular face. A list's `marker_font` selects which family. Bands, layers, and canvas text start from the seed and ignore `Block::Styled`. |
| Underline / strikethrough | Position scales with the run's size. Thickness is 1 pt. Adjacent runs on one line with the same size and color share one stroke, placed from the first segment. Spaces inside the line are covered. A blank keeps its own stroke. |
| `tracking` | Extra space after each cluster followed by more text on the line. It counts toward line breaking. Negative values may not push a glyph's advance below zero (`ErrorKind::NegativeTracking`). |
| Mixed sizes | Each face on a line is scaled by its own size. The line box takes the largest ascent, the deepest descent, and the largest leading, then the larger of that natural height and the largest size times the line-height factor. A line of only blanks uses the first text run's resolved face at the paragraph size. A paragraph with no text runs does not read a font. |
| `Alignment::Justify` | Placement only. Line breaking is unchanged. Phase 1 widens U+0020 spaces up to 0.5em of that space's span size. Phase 2 averages leftover into eligible cluster gaps (Han and fullwidth punctuation, and CJK–Latin boundaries). Latin word interiors are not stretched. The last paragraph line, a hard-break line, a `BlankWidth::ToEndOfLine` line, a line with no stretch slots, and a non-positive remainder stay Start-shaped. Image, `HLine`, and canvas treat `Justify` as Start. |
| `Span::Link` | `target: LinkTarget` is `Uri` or `Anchor`. Layout resolves `Anchor` to page-local `Xyz` before paint. Empty URI fails with `EmptyLink`. Empty anchor id fails with `EmptyAnchorId`. |
| `Block::Anchor` | `{ id, outline, children }`. The destination is the first effective fragment among `children` (top-left, y down). Optional `OutlineEntry { title, level }` builds the bookmark tree. |
| Outline levels | First outline item must be level `1`. Later items deepen by at most +1; jumping up (e.g. 3→1) is allowed. Level `0` and empty titles fail. No outline entries → `to_pdf` does not write an outline. |
| `BlankWidth::Fixed` / `Em` / `ToEndOfLine` | Underline-style blanks in forms |
| Band / layer copy | Uses `PageText` (resolved at paint time), not `Span`. `BandSlot.link` is URI-only. |

---

## Columns

```rust
use pdfcraft_pdf::spec::{ColumnSpec, ColumnStyle, ColumnsSpec, ColumnsStyle};
use pdfcraft_pdf::{NonNegative, Ratio, Track};

ColumnsSpec {
    style: ColumnsStyle {
        gap: NonNegative::pt(12.0).unwrap(),
    },
    children: vec![
        ColumnSpec {
            style: ColumnStyle {
                width: Track::Ratio(Ratio::new(1.0).unwrap()),
            },
            children: vec![/* … */],
        },
        ColumnSpec {
            style: ColumnStyle {
                width: Track::Ratio(Ratio::new(2.0).unwrap()),
            },
            children: vec![/* … */],
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

Caller-oriented recipes (Chinese, with fuller examples): [tables.md](tables.md).

```rust
use std::num::NonZeroU16;
use pdfcraft_pdf::spec::{
    Block, CellSpec, TableBorder, TableHeader, TableRow, TableSpec, TableStyle,
};
use pdfcraft_pdf::{Color, NonNegative, Track};

TableSpec {
    header: Some(TableHeader {
        repeat: true, // default
        rows: vec![TableRow {
            children: vec![
                CellSpec {
                    children: vec![Block::paragraph((), "Name")],
                    ..CellSpec::default()
                },
                CellSpec {
                    children: vec![Block::paragraph((), "Qty")],
                    col_span: NonZeroU16::new(2).unwrap(),
                    ..CellSpec::default()
                },
            ],
        }],
    }),
    body: vec![/* rows of TableRow */],
    style: TableStyle {
        tracks: Some(vec![Track::Auto, Track::Ratio(pdfcraft_pdf::Ratio::new(1.0).unwrap())]),
        cell_padding: NonNegative::pt(4.0).unwrap(),
        border: TableBorder {
            width: NonNegative::pt(0.5).unwrap(),
            color: Color::rgb(0, 0, 0),
        },
    },
}
```

Tables wider than the frame, or taller than a full body frame when unsplittable
pieces cannot fit, fail with a specific `ErrorKind` rather than shrinking
silently.

---

## Lists

Caller-oriented recipes (Chinese, with fuller examples): [lists.md](lists.md).

```rust
use std::num::NonZeroU32;
use pdfcraft_pdf::spec::{
    ListItemSpec, ListKind, ListSpec, ListStyle, MarkerAlign, MarkerFormat,
};
use pdfcraft_pdf::{NonNegative, Numbering, Positive};

ListSpec {
    style: ListStyle {
        kind: ListKind::Ordered {
            numbering: Numbering::Decimal,
            start: NonZeroU32::new(1).unwrap(),
            format: MarkerFormat::ClosingParen, // 1)  2)  also DotSuffix, Parens, Bare, IdeographicComma
        },
        marker_width: Positive::pt(18.0).unwrap(),
        body_indent: None, // defaults to marker_width
        item_gap: NonNegative::pt(6.0).unwrap(),
        marker_font: None, // Some(key) uses that family's regular face
        marker_align: MarkerAlign::End, // 9) and 10) share the marker column's right edge
    },
    children: vec![
        ListItemSpec {
            marker: None, // automatic label
            children: vec![Block::paragraph((), "First")],
        },
        ListItemSpec {
            marker: Some("*".into()), // custom; does not advance the counter or take format
            children: vec![Block::paragraph((), "Aside")],
        },
    ],
}
```

`item_gap` adds space only between adjacent items. The value is added to the
previous item's trailing spacing. The default is 0. A gap that does not fit
on the page is dropped. The next page does not start with that blank.

`body_indent` is the body column's left edge, measured from the list's left
edge. `None` uses `marker_width`. A smaller value fails.

`ListKind::Ordered.format` changes the generated number (`1.`, `1、`, `1)`, `(1)`, `1`).
A bullet list has no `format` field. Custom markers ignore it. `marker_align` places every marker on
the left or right edge of the marker column. The width is the final string.
A marker wider than `marker_width` is an error.

Markers appear on the first fragment of an item. Continuations keep the hanging
indent and do not reprint the marker. The marker baseline matches the first
text baseline of the item's first block: a paragraph's first line, a table's
first physical row (the header when present), the topmost first line among
columns, or a nested list's first marker. An image, canvas, horizontal rule,
or spacer taller than zero has no text baseline, so the marker's line box sits
on the item top. Put blank space before the list, or use `item_gap`, when you
want vertical gap without moving the marker relative to the first text block.
Bullets, generated numbers, and custom markers use `marker_font` when it is
set, and otherwise the first face that registered successfully. They stay on
that family's regular face. Size and color follow `Block::Styled`. An unknown
`marker_font` key fails with `UnregisteredFont`. A missing glyph fails the
document. Automatic outline numbering (`1.1.1`) is not provided. Nest lists
instead.

---

## Header, footer, and page numbers

```rust
use pdfcraft_pdf::spec::{
    BandFn, BandInset, BandSlot, BandSpec, BandStyle, PageText, RunStyle,
};

BandFn::fixed(BandSpec {
    style: BandStyle {
        inset: BandInset::PageMargins, // or BandInset::Fixed { left, right }
    },
    left: Some(BandSlot {
        children: vec![PageText::Literal("Acme Co.".into())],
        font: (),
        style: RunStyle { bold: Some(true), ..RunStyle::default() },
        link: None,
    }),
    right: Some(BandSlot {
        children: vec![
            PageText::Literal("Page ".into()),
            PageText::PageNumber,
            PageText::Literal(" / ".into()),
            PageText::PageCount,
        ],
        font: (),
        style: RunStyle::default(),
        link: None,
    }),
    ..BandSpec::default()
})
```

| `PageText` | Value |
| --- | --- |
| `Literal` | Fixed string |
| `PageNumber` / `PageCount` | Document page (1-based) |
| `SectionNumber` / `SectionCount` | Page within the current section |

`BandFn::from_fn` receives a `PageBandQuery` (`number`, `count`,
`section_number`, `section_count`, and the page's final `size`). `Ok(None)`
skips that side on that page. The margin stays, and the body origin does not
move. `Ok(Some(spec))` resolves fonts for that page. `Err` fails `layout_spec`.
Wrap your own failure with `Error::band_fn`. Its display stays
`header or footer function failed`, and `std::error::Error::source` returns
your error. `layout_spec` requires `K: 'static`.

Bands span the full page width unless inset. Content taller than the margin
band fails with `ErrorKind::DecorationTallerThanMargin`. A `fixed` band reports
that once for the section. A `from_fn` band reports it on the page.
`BandSlot.link` builds the URI from `PageText` values at paint time the same way.
Band, layer, and canvas text take a `RunStyle` that starts from the seed, not from any enclosing block.

`BandSpec.marks` holds `Mark` values. Coordinates start at the band
top-left. A header point `(x, y)` is the same point on the page. A footer
point `(x, y)` is the page point `(x, y + page_height - margin.bottom)`.
`BandInset` moves the slots only. The library paints the marks in order, then
the slots. Each mark box, including half the stroke width, must stay inside
the page width and that side's margin. A box that starts above or to the left
of the band is `GeometryKind::BandMarkOutside`. A box past the bottom of the
margin is `DecorationTallerThanMargin`. A box past the right edge of the page
is `DecorationWiderThanFrame`. The library does not clip the mark and does not
increase the margin.

To vary a header or footer by page, call `BandFn::from_fn` and branch on
`query.number.get() % 2`. Return `Ok(Some(spec))` with that page's marks, or
`Ok(None)` when that page should stay blank. To move a `front` footer onto the
band, follow [migrate-band-marks.md](migrate-band-marks.md).

```rust
use pdfcraft_pdf::spec::{BandFn, BandSpec, LayerShape, Mark};
use pdfcraft_pdf::{Color, Ink, Pen, Rect};

let footer = BandFn::from_fn(|query| {
    if query.number.get() % 2 == 1 {
        return Ok(None);
    }
    let pen = Pen::new(Color::rgb(153, 153, 153), 1.0)?;
    Ok(Some(BandSpec {
        marks: vec![Mark::Shape(LayerShape::Rect {
            rect: Rect::try_pt(24.0, 8.0, 180.0, 65.0)?,
            ink: Ink::Stroke(pen),
        })],
        ..BandSpec::default()
    }))
});
```

---

## Layers (`behind` / `front`)

Use `LayerMark { when, child }` for page-anchored content (seals, watermarks, paths).
`child` is a `Mark`. The same `Mark` type is used in a header or footer band.

| `Mark` | Notes |
| --- | --- |
| `Shape(LayerShape::Text)` | One unwrapped line on a font key. Forced breaks fail. `at` is the line-box top-left. |
| `Shape(LayerShape::Image)` | Stretched into a page `Rect` |
| `Shape(LayerShape::Line)`, `Rect`, or `Ellipse` | Geometry plus pen or ink |
| `Rotated { degrees, pivot, shapes }` | One `Angle` and one pivot for a group of `LayerShape<K>`. The group may mix path, text, and image. `degrees` comes from `Angle::degrees`. |

`PageWhen::All` paints every page of the section.
`PageWhen::Last` paints only the last page of the whole document.

Paint order is `behind`, then the header, then the body, then the footer, then `front`.

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
| Nesting depth | 16 |

Overflow → `ErrorKind::LimitExceeded` + `LimitKind`.

Nesting depth is the deepest level in the whole document. Section body blocks
are level 0. Keep-together groups, styled groups, anchors, insets, columns,
list items, table cells, and links each add one level. It is checked before
fonts and images are read. Unoptimized builds may need a larger thread stack
when the cap is raised above 16.

---

## Errors

`layout_spec` and `to_pdf` return `Result<T, Errors>`:

- Up to **64** failures, discovery order
- Inspect with `first()`, `as_slice()`, `iter()`, `truncated()`
- Each `Error` has `kind: ErrorKind` and optional `at: Location` (section, page,
  region, cell)
- Skrifa, krilla, and `Error::band_fn` failures: `std::error::Error::source()`
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
BAND_FN_CASE=odd-even-marks cargo run -p pdfcraft-pdf --example band_fn  # odd/even footer marks → out/band_fn.pdf
```

See [examples/README.md](../examples/README.md).

---

## What this crate does not do

- Bidirectional text, vertical writing, kerning, ligatures, full UAX #14
- Punctuation compression / hanging punctuation for Justify
- TOC generation, QR encoding, SVG parsing, a named image dictionary, encryption
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
