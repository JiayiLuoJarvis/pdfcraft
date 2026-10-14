# pdfcraft-forms guide

How to inspect and fill existing AcroForm PDFs with `pdfcraft-forms`: mental
model, field values, fonts, alignment, decorative lines, errors, and testing.
For a one-page overview see the [crate README](../README.md). For every public
item signature see [rustdoc](https://docs.rs/pdfcraft-forms)
(`cargo doc -p pdfcraft-forms --open` locally).

Maintainer behavior contracts live in the workspace
[`docs/pdfcraft-forms-spec.md`](../../../docs/pdfcraft-forms-spec.md). This guide
is the caller-facing how-to.

---

## Mental model

You do **not** build a blank form, write `/V`, or keep appearance streams. You
pass PDF bytes you already have, a map of field names to values, and optional
draw settings. The library overlays ink into each widget rectangle, then strips
every widget annotation and the catalog `/AcroForm` entry.

```text
PDF bytes  →  list_fields  →  Vec<FieldInfo>

PDF bytes
  + HashMap<String, FieldValue>
  + FillOptions
  →  fill_and_flatten  →  PDF bytes (form removed)
```

An empty field map still flattens. Unfilled widgets disappear with the form.
Pre-printed page content stays.

This crate does not depend on `pdfcraft-pdf` and does not use krilla.

```rust
use std::collections::HashMap;

use pdfcraft_forms::{
    fill_and_flatten, list_fields, FieldValue, FillOptions, FontBytes, Result,
};

fn fill_employee(pdf: &[u8], font: &[u8]) -> Result<Vec<u8>> {
    let widgets = list_fields(pdf)?;
    assert!(widgets.iter().any(|w| w.name == "employee_name"));

    let mut fields = HashMap::new();
    fields.insert("employee_name".into(), FieldValue::Text("Ada".into()));
    fields.insert("agree".into(), FieldValue::CheckBox(true));

    fill_and_flatten(
        pdf,
        &fields,
        &FillOptions {
            font: Some(FontBytes {
                bytes: font,
                index: 0,
            }),
            ..FillOptions::default()
        },
    )
}
```

---

## Coordinates

| Rule | Detail |
| --- | --- |
| Space | PDF user space |
| Origin | Bottom-left of the page |
| Axes | `x` right, `y` **up** |
| Unit | PDF point (`1/72` inch) |

`AbsoluteLine` uses the same space. This differs from `pdfcraft-pdf` (top-left
origin, y downward). Do not mix the two conventions in one mental model.

---

## Inspect widgets

`list_fields` returns **one `FieldInfo` per widget annotation**, not one entry
per logical field tree. Two widgets that share `/T` produce two values.

| Field | Meaning |
| --- | --- |
| `name` | `/T`, inherited from the first parent that has the key |
| `field_type` | Text, CheckBox, ListBox, ComboBox, Signature, RadioButton, PushButton |
| `page_index` | Zero-based page |
| `options` | Legal choice strings for list/combo; `None` otherwise |

`FieldInfo` does **not** expose the widget rectangle. Rectangles stay internal
for drawing.

Open failures include encrypted PDFs and AcroForms that contain `/XFA`.

---

## Field values

```text
FieldValue::Text(String)      // Text, ListBox, ComboBox
FieldValue::CheckBox(bool)    // CheckBox only
```

There is no loose `str | bool | int` union. Putting the wrong variant on a
checkbox fails with `InvalidCheckbox`.

### Text and choices

- Non-empty text needs a usable TrueType font with a `glyf` table on
  `FillOptions::font`.
- Empty strings draw nothing and do not require a font.
- For ListBox / ComboBox, the string must be one of the widget's `/Opt` export
  **or** display values. The library draws your string as given. It does not
  rewrite it to the display label.
- An illegal choice is `IllegalChoice`.

### Checkboxes

- `true` draws a check clipped to the widget rectangle.
- `false` draws nothing.
- Checkboxes do not use the font.

### Unsupported widgets

You may see Signature, RadioButton, and PushButton in `list_fields`. Putting
them in `fields` fails (`SignatureNotWritable` or `WidgetUnsupported`). Leaving
them out of `fields` still removes them on flatten.

---

## FillOptions

| Field | Default | Role |
| --- | --- | --- |
| `font` | `None` | Required when any non-empty text/choice is drawn |
| `font_size` | `12.0` | Starting size in points. The fitter may step down to `7.0` |
| `font_size_overrides` | none | Per-field starting size |
| `missing` | `Error` | Unknown names in maps: fail, or `Ignore` |
| `h_align` / `v_align` | `Left` / `Bottom` | Default text placement |
| `h_align_overrides` / `v_align_overrides` | none | Per-field placement |
| `line_below` | none | Field name → offset below the widget bottom. Draws a 1 pt black rule the width of the widget. No font needed |
| `absolute_lines` | empty | Page-absolute strokes. Not field names |

Unknown names in `fields`, alignment maps, `font_size_overrides`, and
`line_below` follow `missing`. `absolute_lines` do not participate in that
check. An out-of-range page index fails during flatten.

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
use pdfcraft_forms::FontBytes;

let font = FontBytes {
    bytes: &ttf_bytes,
    index: 0, // face index; single-face TTF uses 0
};
```

- TrueType with `glyf` is required for text.
- A TrueType collection is reduced to `index` before embed.
- OTF and font subsetting are **not** implemented. Successful fills embed the
  whole face.
- CJK works when the face contains the needed glyphs. Set
  `PDFCRAFT_FORMS_FONT` for the Chinese fixture test and examples.
- Fonts are not vendored in git.

Same inputs should produce the same output bytes (the implementation zeros
font timestamps or equivalent).

---

## Decorative lines

**`line_below`.** Map a field name to a point offset under the widget bottom.
The library draws a black 1 pt horizontal rule the width of that widget.

**`absolute_lines`.** `AbsoluteLine` strokes in PDF user space. Defaults match
a common production accent (red, 0.8 pt) via `AbsoluteLine::new`.

```rust
use pdfcraft_forms::AbsoluteLine;

let rule = AbsoluteLine::new(72.0, 100.0, 400.0, 100.0); // page 0, red 0.8 pt
```

---

## Errors

Match on `Error::kind` (`ErrorKind`). Field-related kinds carry `field_name`
where it applies.

| Kind | When |
| --- | --- |
| `NotAPdf` | Bytes are not an openable PDF |
| `Encrypted` | Encrypted input |
| `HasXfa` | AcroForm has `/XFA` |
| `UnknownField` | Name not in the PDF (when `missing = Error`) |
| `IllegalChoice` | Choice not in `/Opt` |
| `InvalidCheckbox` | Wrong value variant for a checkbox |
| `InvalidArgument` | Non-finite or illegal sizes |
| `MissingGlyph` | Character absent from the font |
| `BoxOverflow` | Text still exceeds the box at minimum size |
| `FontNotConfigured` | Text needed a font that was missing or unusable |
| `SignatureNotWritable` | Tried to fill a signature |
| `WidgetUnsupported` | Tried to fill radio or push button |
| `FlattenFailed` | Overlay merge or write failed |

There is no HTTP-oriented `to_dict`. Map kinds in your application layer.

---

## Examples and tests

```bash
# Needs PDFCRAFT_FORMS_FONT → TrueType with glyf
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
- OTF fonts or font subsetting
- Depend on `pdfcraft-pdf`, or pull a second PDF stack into that crate
- Process-wide default fonts

Filling AcroForms is not a feature flag of `pdfcraft-pdf`. Use this crate.

---

## Crate map

| Path | Role |
| --- | --- |
| `list_fields` / `fill_and_flatten` | Public entrypoints |
| `FieldInfo`, `FieldValue`, `FillOptions`, … | Values |
| `Error` / `ErrorKind` | Failures |
| `examples/` | Runnable demos |
| `docs/guide.md` | This guide |
| Workspace `docs/pdfcraft-forms-spec.md` | Maintainer reference |
