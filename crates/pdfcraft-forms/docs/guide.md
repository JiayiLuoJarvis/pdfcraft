# Fill an existing AcroForm

Open a PDF, compile a `FillRequest`, and write a flattened PDF. This page
covers field values, fonts, alignment, decorative lines, errors, and tests.
For a one-page overview see the [crate README](../README.md). For every public
item signature see [rustdoc](https://docs.rs/pdfcraft-forms)
(`cargo doc -p pdfcraft-forms --open` locally).

Maintainer behavior contracts live in the workspace
[`docs/pdfcraft-forms-spec.md`](../../../docs/pdfcraft-forms-spec.md). This guide
is the caller-facing how-to.

---

## Open, fill, and flatten

You do **not** build a blank form, write `/V`, or keep appearance streams. You
open PDF bytes you already have, compile a `FillRequest`, then write the
flattened PDF. The library overlays ink into each widget rectangle, then strips
every widget annotation and the catalog `/AcroForm` entry.

```text
PDF bytes → Form::open → &[FieldInfo]

Form + FillRequest + optional Font → Fill::new → Fill::to_pdf → PDF bytes
```

An empty request still flattens. Unfilled widgets disappear with the form.
Pre-printed page content stays.

This crate does not depend on `pdfcraft-pdf` and does not use krilla.

```rust
use std::collections::BTreeMap;

use pdfcraft_forms::{
    FieldFill, FieldRule, FieldStyle, Fill, FillRequest, FillStyle, Font, Form, HAlign, Result,
};

fn fill_employee(pdf: &[u8], font_bytes: Vec<u8>) -> Result<Vec<u8>> {
    let form = Form::open(pdf)?;
    assert!(form.fields().iter().any(|widget| widget.name() == "employee_name"));

    let font = Font::new(font_bytes)?;
    let name = FieldFill {
        style: FieldStyle {
            h_align: Some(HAlign::Center),
            ..FieldStyle::default()
        },
        ..FieldFill::text("Ada")
    };
    let request = FillRequest {
        style: FillStyle {
            font_size: 14.0,
            ..FillStyle::default()
        },
        rules: BTreeMap::from([("employee_name".to_owned(), FieldRule::new(2.0))]),
        ..FillRequest::new(BTreeMap::from([
            ("employee_name".to_owned(), name),
            ("agree".to_owned(), FieldFill::check(true)),
        ]))
    };
    Fill::new(&form, request, Some(&font))?.to_pdf()
}
```

Stripping the form without drawing a value:

```rust
let form = Form::open(pdf)?;
let filled = Fill::new(&form, FillRequest::default(), None)?.to_pdf()?;
```

A single `sfnt` uses `Font::new`. A `ttcf` uses `Font::from_collection(bytes, index)`.

---

## Coordinates

| Rule | Detail |
| --- | --- |
| Space | PDF user space |
| Origin | Bottom-left of the page |
| Axes | `x` right, `y` **up** |
| Unit | PDF point (`1/72` inch) |

`AbsoluteLine` uses the same space. `pdfcraft-pdf` uses a top-left origin and
y downward. Write `AbsoluteLine` coordinates with the bottom-left origin.

---

## Inspect widgets

`Form::fields` returns **one `FieldInfo` per widget annotation**, not one entry
per logical field tree. Two widgets that share `/T` produce two values.

| Accessor | Meaning |
| --- | --- |
| `name()` | `/T`, inherited from the first parent that has the key |
| `field_type()` | Text, CheckBox, ListBox, ComboBox, Signature, RadioButton, PushButton |
| `page_index()` | Zero-based page |
| `options()` | Legal choice strings for list/combo; `None` otherwise |

`FieldInfo` does **not** expose the widget rectangle. Rectangles stay internal
for drawing.

Open failures include encrypted PDFs and AcroForms that contain `/XFA`.

---

## Field values

```text
FieldValue::Text(String)      // Text, ListBox, ComboBox
FieldValue::CheckBox(bool)    // true draws the check; false draws nothing
```

There is no loose `str | bool | int` union. Putting the wrong variant on a
checkbox fails with `InvalidCheckbox`.

Put a value in `FieldFill::text` or `FieldFill::check`. A per-field override
is `FieldFill { style: FieldStyle { .. }, ..FieldFill::text("Ada") }`. A
`FieldStyle` on a checkbox is checked and does not change the check mark.

### Text and choices

- Non-empty text needs a usable TrueType font with a `glyf` table, passed as
  `Some(&font)` to `Fill::new`.
- Empty strings draw nothing and do not require a font.
- For ListBox / ComboBox, the string must be one of the widget's `/Opt` export
  **or** display values. The library draws your string as given. It does not
  rewrite it to the display label.
- An illegal choice is `IllegalChoice`.

### Checkboxes

- `true` draws a check clipped to the widget rectangle.
- `false` draws nothing.
- Checkboxes do not use the font. `None` and `Some(&font)` write the same PDF
  when no text is drawn.

### Unsupported widgets

You may see Signature, RadioButton, and PushButton in `Form::fields`. Putting
them in `fields` fails (`SignatureNotWritable` or `WidgetUnsupported`). Leaving
them out of `fields` still removes them on flatten.

---

## FillRequest

| Field | Default | Role |
| --- | --- | --- |
| `style.font_size` | `12.0` | Starting size in points. The fitter may step down to `7.0` |
| `style.h_align` / `style.v_align` | `Left` / `Bottom` | Default text placement |
| `fields` | empty | `BTreeMap<String, FieldFill>`. A value is required |
| `FieldStyle` | all `None` | Per-field size and alignment. `None` keeps the document style |
| `missing` | `Error` | Unknown names in `fields`, then `rules`: fail, or `Ignore` |
| `rules` | empty | `BTreeMap<String, FieldRule>`. 1 pt black rule below the widget. No font needed |
| `lines` | empty | `AbsoluteLine` strokes. Not field names |

Unknown names in `fields` and `rules` follow `missing`. `lines` do not
participate in that check. A line whose `page_index` is past the document
fails in `Fill::new` as `InvalidArgument(PageOutOfRange)`, and `page()` is
that index.

### Fit and wrap (v1 defaults)

| Constant | Value |
| --- | --- |
| Minimum font size | `7.0` pt |
| Step down | `0.5` pt |
| Line height | `1.2` × font size |
| Padding | left/right `1` pt, bottom `1` pt, top `0.5` pt |

Wrap rules:

- Code points `> 0x7F` may break per character.
- ASCII breaks on words, then hard-breaks oversized words.
- Explicit `\n` starts a new paragraph.
- Still too wide at the minimum size → `BoxOverflow`.
- A character with no cmap entry → `MissingGlyph`.

Drawing is clipped to the widget `/Rect`.

---

## Fonts

Pass face bytes yourself. There is no process-wide default font.

```rust
use pdfcraft_forms::Font;

let font = Font::new(ttf_bytes)?;
let face = Font::from_collection(ttc_bytes, 0)?;
```

- TrueType with `glyf` is required for text.
- `Font::new` rejects bytes that start with `ttcf`. `Font::from_collection`
  extracts one face and rejects a single `sfnt`.
- Construction does not modify the caller's bytes. `from_shared` stores that
  `Arc`. Cloning a `Font` does not copy the face bytes.
- OTF is not implemented. A successful text fill embeds a `glyf` subset that
  keeps the original glyph ids. The subset `head` timestamps are zero.
- CJK works when the face contains the needed glyphs. Set
  `PDFCRAFT_FORMS_FONT` for the Chinese fixture test and examples.
- Fonts are not vendored in git.

The same `Fill` written twice produces the same PDF bytes. When no text is
drawn, `Fill::new` does not read the font.

---

## Decorative lines

**`FieldRule`.** Map a field name to a point offset under the widget bottom.
The library draws a black 1 pt horizontal rule the width of that widget.
The name can be absent from `fields`.

**`AbsoluteLine`.** Strokes in PDF user space. Defaults match a common
production accent (red, 0.8 pt) via `AbsoluteLine::new`. Width and color live
on `style: StrokeStyle`.

```rust
use pdfcraft_forms::AbsoluteLine;

let rule = AbsoluteLine::new(72.0, 100.0, 400.0, 100.0); // page 0, red 0.8 pt
```

---

## Errors

Compare `error.kind()`. Read `field()`, `page()`, and `scalar()` for the
widget name, page index, and missing character. `Display` does not include
`source()` text.

| Kind | When |
| --- | --- |
| `NotAPdf` | Bytes are not an openable PDF |
| `Encrypted` | Encrypted input |
| `HasXfa` | AcroForm has `/XFA` |
| `UnknownField` | Name not in the PDF (when `missing = Error`) |
| `IllegalChoice` | Choice not in `/Opt` |
| `InvalidCheckbox` | Wrong value variant for a checkbox |
| `InvalidArgument` | Non-finite or illegal sizes, or a line page past the document |
| `MissingGlyph` | Character absent from the font. Returned by `Fill::new` |
| `BoxOverflow` | Text still exceeds the box at minimum size. Returned by `Fill::new` |
| `FontNotConfigured` | Text needed a font that was missing or unusable |
| `SignatureNotWritable` | Tried to fill a signature |
| `WidgetUnsupported` | Tried to fill radio or push button |
| `FlattenFailed` | Overlay merge or write failed |

There is no HTTP-oriented `to_dict`. Map kinds in your application layer.

---

## Examples and tests

```bash
# Set PDFCRAFT_FORMS_FONT to a TrueType face with glyf
cargo run -p pdfcraft-forms --example fill_basic
cargo test -p pdfcraft-forms
```

Prefer assertions on:

- Output starts with `%PDF`
- Catalog has no `/AcroForm` after a successful fill
- `pdftotext` (or content search) contains the filled text
- Error kinds for illegal inputs
- Byte equality across two identical fills
- Empty map still strips every widget

Fixture notes live in [tests/fixtures/README.md](../tests/fixtures/README.md).

---

## What this crate does not do

- Create or edit blank form structure
- XFA runtime, digital signature writing, encryption writing
- Read `/Q`, `/DA`, `/MaxLen`, multiline flags, `/Rotate`, or CropBox (v1)
- OTF fonts. Text embed uses a `glyf` subset and keeps the original glyph ids
- Depend on `pdfcraft-pdf`, or pull a second PDF stack into that crate
- Process-wide default fonts

Filling AcroForms is not a feature flag of `pdfcraft-pdf`. Use this crate.

---

## Crate map

| Path | Role |
| --- | --- |
| `Form` / `Fill` / `Font` | Public entrypoints |
| `FieldInfo`, `FieldFill`, `FillRequest`, … | Values |
| `Error` / `ErrorKind` | Failures |
| `examples/` | Runnable demos |
| `docs/guide.md` | This guide |
| Workspace `docs/pdfcraft-forms-spec.md` | Maintainer reference |
