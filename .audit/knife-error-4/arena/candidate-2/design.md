# Candidate 2 — Result / boundary-collect (knife 4)

Shape in one line: **each builder owns a local `ErrorList` and `lower` returns it; `measure_text` returns soft faults beside the measured value; the paint entry is the only place those lists become public `Errors`. `LayoutCtx` has no error slot.**

Confirmed constraints in `docs/error-diagnosability.md` 刀 4 (points 1–8) are fixed.

---

## 1. Usage first (the spec)

### 1.1 One `Result`, the list is the `Err`

```rust
use quire::{document, Error, ErrorKind, Errors, A4};

fn invoice(font: Vec<u8>) -> quire::Result<Vec<u8>> {
    let rendered = document(A4, |doc| {
        doc.font(font)?;                         // Result<_, Error>; `?` lifts via From
        doc.paragraph(|p| {
            p.text("Hello 世界");                 // missing scalar: recorded at measure, walk continues
        });
        doc.table(|t| {
            t.cell_padding(-1.0);                // recorded; the closure keeps going
            t.row([Cell::text("a"), Cell::spacer(f32::NAN)]);
        });
        Ok(())
    })?;
    quire::to_pdf(rendered)                      // backend: one Error, wrapped to a one-item Errors
}

fn explain(errors: &Errors) {
    eprintln!("{errors}");                       // one item: today's Error text; else numbered lines
    let _first: &Error = errors.first();
    for error in errors {
        if matches!(error.kind, ErrorKind::MissingGlyph { .. }) { /* … */ }
    }
    if errors.truncated() { /* 64 kept; later faults were dropped */ }
}
```

`Rendered` exists only on `Ok`. Callers do not fetch a second list, construct `Errors`, or read a bag off `Document`.

Discovery order for the closure above, assuming `世` is missing and both table faults fire: both table faults first (assembly, in call order), then the glyph (measure). Assembly finishes before measurement, so a measure fault on an earlier paragraph follows a later assembly fault.

```rust
let errors = document(A4, |doc| {
    doc.font(font)?;
    doc.paragraph(|p| p.text("\u{4E16}"));
    doc.spacer(-3.0);
    Ok(())
})
.unwrap_err();

assert_eq!(errors.as_slice().len(), 2);
assert!(matches!(errors.first().kind, ErrorKind::NegativeLength { .. }));
assert!(matches!(errors.as_slice()[1].kind, ErrorKind::MissingGlyph { .. }));
assert!(!errors.truncated());
```

### 1.2 What a caller can ignore

`font` / `bold_font` / `load_image` / `pt` stay `Result<T, Error>`. Dropping that `Result` drops the fault. `paragraph` / `table` / `columns` / `section` do not return the list; the document returns it once, from `document`.

`section` returns `()`. The inner closure is still `quire::Result<()>`, so `pt(-1.0)?` aborts **that** closure only. `section` keeps the inner `Err` and the next `section` still runs. A `?` on `section(...)` itself goes away.

```rust
document(A4, |doc| {
    doc.section(|s| {
        s.paragraph(|p| p.font_size(-1.0));      // noted, closure continues
        Ok(())
    });
    doc.section(|s| {
        s.margins(Margins::try_uniform(-5.0)?);  // aborts this closure; recorded on section 1
        Ok(())
    });
    Ok(())
})
```

### 1.3 Display

One item and `truncated == false`: identical to `Error`'s `Display`. More than one item: one numbered line per item, same per-line text as `Error`. When `truncated`, a final line `further errors omitted`.

```text
1. -1 is a negative length
2. font 0 has no glyph for U+4E16 (section 0, page 1, body)
```

---

## 2. Types and signatures

`error` gains no dependency. `FontId` stays out of `error.rs` (`ErrorKind` keeps `u32`).

### 2.1 Public

```rust
pub(crate) const MAX_ERRORS: usize = 64;

/// Non-empty faults from one `document` or `to_pdf` call, discovery order.
/// At most 64 items. `truncated` means more were found and dropped.
/// `PartialEq`: each item by `Error`'s equality (`kind` + `at`) and this flag.
#[derive(Debug, Clone, PartialEq)]
pub struct Errors {
    items: Vec<Error>, // 1..=64
    truncated: bool,
}

impl Errors {
    pub fn first(&self) -> &Error;
    pub fn as_slice(&self) -> &[Error];
    pub fn iter(&self) -> std::slice::Iter<'_, Error>;
    pub fn truncated(&self) -> bool;
}

impl From<Error> for Errors { /* one item, truncated: false */ }
impl std::fmt::Display for Errors { /* §1.3 */ }
impl std::error::Error for Errors {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> { None }
}
impl<'a> IntoIterator for &'a Errors { type Item = &'a Error; /* slice */ }
impl IntoIterator for Errors { type Item = Error; }

pub type Result<T> = std::result::Result<T, Errors>;
```

No `len` (use `as_slice().len()`). No `push`. No `From<ErrorKind>`. No `ErrorKind::Truncated`. `Error::source` stays per item.

| Returns `quire::Result<T>` | Returns `Result<T, Error>` |
| --- | --- |
| `document`, `to_pdf`, the build closure, the `section` closure | `geom` / `Into*` / `LinkTarget::uri`, `Canvas`, `ImageRegistry`, `Document::font` / `bold_font` / `load_image` |

`?` bridges them with `From<Error> for Errors`.

### 2.2 `ErrorList` — the only merge currency (`pub(crate)`)

Empty is allowed. The cap lives only here.

```rust
#[derive(Debug, Clone, Default)]
pub(crate) struct ErrorList {
    items: Vec<Error>, // <= MAX_ERRORS
    truncated: bool,
}

impl ErrorList {
    pub(crate) fn push(&mut self, error: impl Into<Error>);
    /// `other` is consumed. Items already here stay in front (they were discovered first).
    /// A truncated `other` sets this flag even when every copied item fit.
    pub(crate) fn append(&mut self, other: ErrorList);
    pub(crate) fn append_errors(&mut self, errors: Errors);
    /// Fills blank `Location` fields. Fields already set stay set.
    pub(crate) fn fill_missing(&mut self, at: &Location);
    pub(crate) fn is_empty(&self) -> bool;
    /// `None` if empty. `Some` is already capped.
    pub(crate) fn into_errors(self) -> Option<Errors>;
}

pub(crate) type Fallible<T> = std::result::Result<T, Error>;
```

`push` at 64 sets `truncated` and drops the new item. Later pushes do not change `items`. `Error::fill_missing` is the one field-wise implementation; `in_section` / `in_cell` / `on_page_index` / `in_region` become wrappers for hard paths that still return a single `Error` (backend, span checks).

`lower` returns `ErrorList`, not `Errors`. Sealing at every node would have to be undone to append the next sibling. `Errors` is produced only when a value crosses into `quire::Result`.

### 2.3 Signatures

```rust
// builders — local list, drained by value
impl Table {
    pub(crate) fn lower(self) -> FallibleList<FlowNode>; // type below
}
pub(crate) type FallibleList<T> = std::result::Result<T, ErrorList>;

impl Flow {
    /// `Ok` is pushed. `Err` is appended. Later siblings still lower.
    pub(crate) fn push_lowered(&mut self, lowered: FallibleList<FlowNode>);
    pub(crate) fn finish(self) -> FallibleList<Vec<FlowNode>>;
}

// text/measure.rs — soft faults beside the value; hard face/advance is Err(Error)
pub(crate) fn measure_text(
    fonts: &mut FontStore,
    font: FontId,
    text: &str,
) -> Fallible<(MeasuredText, ErrorList)>;

// layout/paragraph.rs — merges measure + bold, then returns. No geometry `?` after that.
pub(crate) fn measure_paragraph(/* … */) -> Fallible<(MeasuredParagraph, ErrorList)>;

// layout — geometry / stall / illegal frame. No error list on LayoutCtx.
pub(crate) fn fit(paragraph: MeasuredParagraph, /* frame */) -> Fallible<Fragment>;

// layout/paginate.rs — owns a local list; matches leaf `Err(Error)`, does not take one
pub(crate) fn layout_pages(/* jobs, fonts */) -> (Vec<PagePlan>, ErrorList);

// document — the seal
impl Document<'_> {
    pub fn section(&mut self, f: impl FnOnce(&mut Section<'_>) -> Result<()>) {}
    pub(crate) fn render(self, built: Result<()>) -> Result<Rendered>;
}

pub fn to_pdf(rendered: Rendered) -> Result<Vec<u8>>; // write_pdf.map_err(Errors::from)
```

`LayoutCtx` keeps fonts and frame. It does not grow an `errors` field, and `measure_text` does not take `&mut ErrorList`. Leaves (`fit`, face/advance inside `measure_text`, `paint_pages`) return `Err(Error)`. `layout_pages` matches those, `push`es onto its local list, and stops, so earlier soft items stay in the list `render` seals. `paint_pages` and `to_pdf` use `map_err(Errors::from)`.

---

## 3. Flow

### 3.1 Order rule

One `ErrorList` per builder. Append at discovery.

1. A setter fault is `push`ed when the setter runs.
2. When a child is attached, the parent `append`s that child's list (the child is finished; its internal order stays). Notes already on the parent stay in front of that append; later notes `push` after it.
3. `lower` then runs checks that need the finished container (spans, column skeleton) and `push`es those. They were discovered now, so they follow (1) and (2).
4. `lower` returns `Err(list)` when the list is non-empty and does not return the node. It never returns a stored container fault *instead of* walking children: the children were appended in (2), before (3).

That is the override fix. A block concat of "every child, then every parent note" at `lower` would move a `cell_padding` fault from before a row to after it. Add-time `append` keeps call order; `lower` only adds what it newly discovers.

`ContentOutsideSection` is one latch on the rejecting flow (`outside_noted`). The latch records the marker once. Later rejected pushes still `append` those children's own lists.

### 3.2 Table (assembly)

```rust
impl Table {
    pub fn cell_padding(&mut self, v: impl IntoLength) {
        if let Err(error) = v.try_positive() {
            self.errors.push(error);
        }
    }

    pub fn row(&mut self, cells: impl IntoRow) {
        for cell in cells.into_row() {
            match cell.into_stored() {
                Ok(stored) => self.rows.push(stored),
                Err(list) => self.errors.append(list), // this cell is not in the grid
            }
        }
    }

    pub(crate) fn lower(mut self) -> FallibleList<FlowNode> {
        if let Err(error) = check_spans(&self.rows) {
            self.errors.push(error);             // after every row() append
        }
        if self.errors.is_empty() {
            Ok(FlowNode::from_rows(self.rows))
        } else {
            Err(self.errors)                     // siblings of this table still lower
        }
    }
}
```

`List` and `Columns` use the same three steps. A bad `col_fixed` / empty link still skips its closure; the fault that caused the skip is the one that was `push`ed. Both sides of `inset` / `line` are `push`ed left then right. Call order `cell_padding(-1)`, then a `NaN` cell, then a span fault at `lower`: `NegativeLength`, `NaN`, `InvalidCellSpan`.

### 3.3 Document

`Document.error: Option<Error>` is deleted. `section` drains the section flow when the closure returns:

```rust
pub fn section(&mut self, f: impl FnOnce(&mut Section<'_>) -> Result<()>) {
    let mut section = Section::new(/* index */);
    let returned = f(&mut section);              // always invoked
    let mut list = section.finish();             // ErrorList, call order inside the section
    if let Err(errors) = returned {
        list.append_errors(errors);              // the inner `?`, after notes that actually ran
    }
    list.fill_missing(&Location::section(index));
    match list.into_errors() {
        None => self.jobs.push(section.into_job()),
        Some(errors) => self.errors.append_errors(errors), // job dropped
    }
}
```

Implicit-flow faults are appended when that flow is closed, before a later `ContentOutsideSection`. `paint` walks jobs in vector order. There is no document slot checked before that walk.

### 3.4 Measure, then fit, then paint

Soft faults stay on `Ok`, so the caller merges them and still sees the next sibling. A hard leaf returns `Err(Error)`; `layout_pages` is the boundary that records it and stops.

```rust
pub(crate) fn measure_text(/* … */) -> Fallible<(MeasuredText, ErrorList)> {
    let mut errors = ErrorList::new();
    let face = match fonts.face(font) {
        Ok(face) => face,
        Err(error) => {
            // UnknownFont: nothing to cmap. Zero-advance text, keep going at the caller.
            errors.push(error);
            return Ok((MeasuredText::empty(), errors));
        }
    };
    for (scalar, gid) in cmap(&face, text) {
        match gid {
            Some(gid) => { /* push advance; Font/advance failure returns Err(error) */ }
            None => {
                errors.push(ErrorKind::MissingGlyph { font: font.as_u32(), scalar });
                /* .notdef advance, next scalar */
            }
        }
    }
    Ok((measured, errors))
}
```

`MissingGlyph`, `BoldFaceRequired`, and `UnknownFont` are the soft set. `BoldFaceRequired` is `push`ed in `measure_paragraph` when `fonts.bold()` fails; non-bold spans of that paragraph still go through `measure_text`. A bold id is not also reported as `UnknownFont`.

`measure_paragraph` returns `(MeasuredParagraph, ErrorList)` and does not call `fit`. The paginator merges, **then** places:

```rust
pub(crate) fn layout_pages(/* … */) -> (Vec<PagePlan>, ErrorList) {
    let mut pages = Vec::new();
    let mut found = ErrorList::new();
    for node in nodes {
        let (measured, soft) = match measure_paragraph(node, fonts) {
            Ok(pair) => pair,
            Err(hard) => {
                found.push(hard);                // face/advance; earlier siblings already in `found`
                return (pages, found);           // stop. Do not `?` — that would drop `found`.
            }
        };
        found.append(soft);
        found.fill_missing(&page_at);            // page + region; cell/section already set stay
        match fit(measured, frame) {
            Ok(fragment) => pages.extend(fragment),
            Err(hard) => {
                found.push(hard);                // geometry last in this walk
                return (pages, found);
            }
        }
    }
    (pages, found)
}
```

`measure_text` returns `Err` only for a face/advance failure, which carries no soft list of its own (glyphs already seen in that string are the accepted loss). `MissingGlyph` / `UnknownFont` are on `Ok`. `render` seals; a non-empty list drops `pages` and skips the canvas. One hard item and an otherwise empty list is `Errors::from` via `into_errors` after `push`.

```rust
pub(crate) fn render(self, built: Result<()>) -> Result<Rendered> {
    let (jobs, mut found) = self.assemble();     // kept jobs + assembly ErrorList
    if let Err(errors) = built {
        found.append_errors(errors);             // caller stopped the closure
        return Err(seal(found));                 // no layout
    }
    let (pages, soft) = layout_pages(jobs, &mut fonts);
    found.append(soft);
    if let Some(errors) = found.into_errors() {
        return Err(errors);                      // plans dropped; canvas not built
    }
    paint_pages(&pages).map_err(Errors::from)    // canvas lifecycle: one Error, From here
}

fn seal(list: ErrorList) -> Errors {
    list.into_errors().expect("seal only after a fault was pushed")
}
```

Bands run only when `layout_pages` returns an empty error list. A font fault in a header is soft: `measure_text`'s list, `fill_missing` with that page and `Header`, append, continue the next region. A decoration that does not fit the margin is `fit`-style `Err(Error)`: push, stop later regions and later pages. Per-page band closures can report the same missing glyph once per page they run; the cap bounds that.

`.notdef` advances exist so one string yields every `MissingGlyph`. `render` skips `paint_pages` whenever `found` is non-empty, so stand-in glyphs are not drawn.

A node whose `lower` returned `Err` is not in `jobs`, so it is not measured. Clean siblings in the same flow are. Their glyph faults follow the whole assembly list (point 5: assembly order, then measure order).

### 3.5 Cap, backend, skips

`ErrorList::push` / `append` are the only cap. Subtree lists cap at 64 and carry `truncated` into the parent. The public value is at most 64 `Error`s plus the display line.

`write_pdf` stays `Result<T, Error>`. `to_pdf` is `map_err(Errors::from)`. No assembly merge on that path.

Unchanged skips: empty link URI, non-finite `Layer::rotated`, bad `col_fixed` / `col_ratio`, `stack` when style `apply` fails, empty band URI.

---

## 4. Deliberately absent

- No error field on `LayoutCtx`, and no `&mut ErrorList` argument on `measure_text`, `fit`, or `split`.
- No second public fetch, no public `push`, no partial `Rendered` / PDF, no parallel collect, no source line numbers.
- No `thiserror` / `anyhow` on the public error, no krilla/skrifa types in `ErrorKind`.
- No dedup by `kind`. Two paragraphs missing the same scalar are two items.
- No soft/hard flag on `Error`. The recording site decides: `Ok` list, or `Err(Error)`.
- `TableNode::new` span tiling stays first-of-kind inside that one check; the check's single `Error` is one `push`.

---

## 5. Red-flag screen

| Flag | Verdict |
| --- | --- |
| Shallow module | Callers learn `Errors` (`first`, `as_slice`, `iter`, `truncated`, `Display`) and the existing `Result` alias. Cap, sibling continuation, assembly-then-measure, and "no paint" sit behind `document`. |
| Information leakage | `ErrorList` and `Fallible` are `pub(crate)`. Page/section policy is `fill_missing` in `error.rs`, applied by the boundary that knows the place. |
| Temporal decomposition | The same `measure_text` runs inside `layout_pages`. Assembly failure does not start a second glyph walk. |
| Pass-through | `render` merges, decides paint vs `Err`, and seals. `lower` adds whole-container checks, then chooses `Ok` node vs `Err` list. |
| Split ownership | The builder owns the list until `lower` / `finish` moves it. Parents `append` that value once. `LayoutCtx` does not alias it. |
| Two ways | Pipeline calls return `Errors`. Single-cause calls return `Error`. Soft faults are the `Ok` list; hard leaves are `Err(Error)`; only `render` / `to_pdf` seal. |
| Importable internals | `use` of `ErrorList` from outside the crate fails. |
| Hand-synced list | The cap is `ErrorList`'s length check. The three soft kinds are the three call sites that `push` onto an `Ok` list (`UnknownFont`, `MissingGlyph`, `BoldFaceRequired`), not a second table. |
