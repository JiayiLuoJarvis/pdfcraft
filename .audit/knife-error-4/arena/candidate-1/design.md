# Candidate 1 — bag-centric multi-error collection (knife 4)

Shape in one line: **one private accumulator, `ErrorBag`, lives in `error`. Every builder value
owns exactly one bag; children merge into their parent at the lower boundary. Layout borrows the
single paint-time bag through `LayoutCtx`. Hard failures stay on the `Err(Error)` channel and are
recorded once at the top. The only place a bag turns into the public `Errors` is the document
entry point.**

Confirmed constraints from `docs/error-diagnosability.md` 刀 4 (points 1–8) are taken as given.

---

## 1. Usage first (the spec)

### 1.1 README excerpt

```rust
use quire::{document, Errors, ErrorKind, A4};

let result = document(A4, |doc| {
    doc.font(noto_sans_bytes())?;          // single-cause call: Err(Error), `?` lifts into Errors
    doc.paragraph(|p| {
        p.text("Hello 世界");              // 世 missing from the font: recorded, layout continues
    });
    doc.table(|t| {
        t.cell_padding(-1.0);             // recorded; closure keeps running
        t.row(["a", "b"]);
        t.row([Cell::spacer(f32::NAN)]);  // recorded too (sibling of the bad padding)
    });
    Ok(())
});

match result {
    Ok(rendered) => write(quire::to_pdf(rendered)?),
    Err(errors) => {
        eprintln!("{errors}");                 // multi-line, numbered
        for error in &errors {                 // each item is today's `Error`
            if matches!(error.kind, ErrorKind::MissingGlyph { .. }) { /* … */ }
        }
        if errors.truncated() { /* 64 shown, more were found */ }
    }
}
```

Display of the `Err` above (discovery order: assembly in document order, then measurement):

```text
3 errors:
  1. -1 is a negative length
  2. NaN is not a finite length
  3. font 0 has no glyph for U+4E16 (section 0, page 1, body)
```

A single error prints exactly as `Error` does today (no "1 errors:" header), so existing
single-error log lines and snapshot tests keep their text.

### 1.2 Call site: an app that only wants "did it work"

```rust
fn invoice_pdf(data: &Invoice) -> quire::Result<Vec<u8>> {
    let rendered = quire::document(A4, |doc| build_invoice(doc, data))?;
    quire::to_pdf(rendered)                    // backend failure: an `Errors` of length 1
}
```

Nothing new to learn: `?` still works, `Errors: std::error::Error + Send + Sync + 'static`.

### 1.3 Call site: a test asserting several mistakes at once

```rust
let errors = quire::document(A4, |doc| {
    doc.font(fixture_font())?;
    doc.paragraph(|p| p.text("\u{4E16}"));
    doc.spacer(-3.0);
    Ok(())
})
.unwrap_err();

assert_eq!(
    errors.as_slice(),
    &[
        Error::from(ErrorKind::NegativeLength { value: -3.0 }),
        on_first_page(ErrorKind::MissingGlyph { font: 0, scalar: '\u{4E16}' }, Region::Body),
    ],
);
assert!(!errors.truncated());
```

Note the order: the spacer is an assembly error, the glyph is a measure error, so the spacer
comes first even though it was written second. That is "assembly order, then measure order".

### 1.4 Call site: sections

```rust
quire::document(A4, |doc| {
    doc.section(|s| {                          // returns (); never short-circuits the build
        s.paragraph(|p| p.font_size(-1.0));    // recorded with section 0
        Ok(())
    });
    doc.section(|s| {
        s.margins(Margins::try_uniform(-5.0)?); // caller's own `?`: aborts this closure only,
        Ok(())                                  // recorded with section 1
    });
    Ok(())
})
```

`Document::section` no longer returns `Result`. Point 6 says builder closures finish after
notes; a `section(..)?` would let the caller's build stop early and would also double-report
the same errors (once through `?`, once from the document's bag). The rule that removes both
problems: **an error either goes into a bag or back to the caller, never both.**

---

## 2. Public surface (derived from the usage)

```rust
// src/error.rs — public

/// One or more failures from one `document` run, in discovery order.
///
/// Never empty. At most 64 items; `truncated` says more were found.
/// Equality compares each item by `kind` and `at` (like `Error`) and the truncation flag.
#[derive(Debug, Clone, PartialEq)]
pub struct Errors {
    items: Vec<Error>,      // invariant: !items.is_empty() && items.len() <= MAX_ERRORS
    truncated: bool,
}

impl Errors {
    /// The first failure found.
    pub fn first(&self) -> &Error { /* items.first(): proven non-empty */ todo!() }
    /// All kept failures, in discovery order.
    pub fn as_slice(&self) -> &[Error] { &self.items }
    /// Same as `as_slice().iter()`.
    pub fn iter(&self) -> std::slice::Iter<'_, Error> { self.items.iter() }
    /// More failures were found than kept.
    pub fn truncated(&self) -> bool { self.truncated }
}

impl From<Error> for Errors { /* items: vec![e], truncated: false */ }
impl<'a> IntoIterator for &'a Errors { type Item = &'a Error; /* slice::Iter */ }
impl IntoIterator for Errors { type Item = Error; /* vec::IntoIter */ }
impl fmt::Display for Errors { /* 1 item: Error's Display verbatim; else numbered list + omitted line */ }
impl std::error::Error for Errors { /* source(): None — see open questions */ }

/// Result of running a document or writing its PDF.
pub type Result<T> = std::result::Result<T, Errors>;
```

Which public functions return which:

| Returns `quire::Result<T>` (list) | Returns `std::result::Result<T, Error>` (one) |
| --- | --- |
| `document`, `to_pdf`, the `build` closure, the `section` closure | `geom` constructors and `Into*` traits, `LinkTarget::uri`, `Canvas` and `ImageRegistry` trait methods, `Document::font` / `bold_font` / `load_image` |

Rule: a call that runs caller closures or the pipeline returns the list; a call that can
only ever fail one way returns `Error`. `?` bridges the two through `From<Error>`.
No `len()` on `Errors` (non-empty type; `as_slice().len()` is enough and avoids clippy's
`len_without_is_empty` dance).

`ErrorBag` is **not** public. Callers never see it.

---

## 3. Internal types

### 3.1 `ErrorBag` (src/error.rs, `pub(crate)`)

```rust
const MAX_ERRORS: usize = 64;

/// Append-only accumulator. One owner at a time; merge with `absorb`.
#[derive(Debug, Default)]
pub(crate) struct ErrorBag {
    items: Vec<Error>,   // <= MAX_ERRORS
    seen: u64,           // every record, kept or dropped, including absorbed counts
}

/// Position in a bag, for "anything since here?" and "locate everything since here".
#[derive(Debug, Clone, Copy)]
pub(crate) struct Mark { index: usize, seen: u64 }

impl ErrorBag {
    /// Appends `error` unless the bag is full. Always counts it.
    pub(crate) fn record(&mut self, error: impl Into<Error>);

    /// Appends `other`'s items after this bag's, then counts `other`'s dropped ones.
    /// Idempotent per value: `other` is consumed, so a child can't be merged twice.
    pub(crate) fn absorb(&mut self, other: ErrorBag);

    pub(crate) fn mark(&self) -> Mark;

    /// Nothing was recorded (kept or dropped) since `mark`.
    pub(crate) fn clean_since(&self, mark: Mark) -> bool;  // seen == mark.seen

    /// Fills unknown location fields of every item kept since `mark`. Inner wins:
    /// fields already set by a deeper scope are left alone.
    pub(crate) fn locate_since(&mut self, mark: Mark, at: &Location);

    pub(crate) fn is_empty(&self) -> bool;                  // seen == 0

    /// `Ok(value)` when nothing was recorded; otherwise the list.
    pub(crate) fn finish<T>(self, value: T) -> Result<T>;

    /// Records `hard` last and returns the list. Used where layout hard-stops.
    pub(crate) fn fail(self, hard: Error) -> Errors;
}

impl From<Errors> for ErrorBag { /* items + seen = len + (truncated as u64) */ }
```

`truncated` is derived, not stored: `seen > items.len()`. One source of truth.

`seen` instead of `items.len()` in `clean_since` matters once the bag is full: a container
lowered after the 64th error must still know its own children failed.

### 3.2 `Error` location helper (src/error.rs)

```rust
impl Error {
    /// Sets each field of `at` that this error does not know yet.
    pub(crate) fn fill_missing(&mut self, at: &Location);
}
impl Location {
    pub(crate) fn section(index: usize) -> Self;
    pub(crate) fn page_index(index: usize) -> Self;  // 1-based conversion, u32 overflow -> None
    pub(crate) fn cell(row: usize, col: usize) -> Self;
    pub(crate) fn with_region(self, region: Region) -> Self;
}
```

Existing `in_section` / `on_page_index` / `in_region` / `in_cell` become one-line wrappers over
`fill_missing`, so the inner-wins rule has one implementation. They stay for hard-only paths
that have no bag (backend `emit_page`, `TableNode::new` span checks).

### 3.3 `Fallible` (src/error.rs, `pub(crate)`)

```rust
/// One failure. Internal code and single-cause public calls use this.
pub(crate) type Fallible<T> = std::result::Result<T, Error>;
```

Every internal `use crate::error::Result` becomes `Fallible`. That is a mechanical rename with
no behavior change, done as the first commit so the semantic diff stays small.

### 3.4 Owners

| Owner | Bag field | Lifetime of the bag | Merged into |
| --- | --- | --- | --- |
| `Paragraph`, `Cell`, `HLine`, `ImageBox`, `TextProps` | `errors` | the value | parent bag when added / stored / applied |
| `Flow` (body, column, item, keep-together) | `errors` | the flow | parent at `Flow::nested` return, or section/document at lower |
| `List`, `Table`, `Columns` | `errors` | the builder | parent `Flow`'s bag in `lower` |
| `Band`, `Layer` (built per page) | `errors` | one page's build | paint-time bag in `lower` |
| `Document` | `errors` | `document()` call | — root assembly bag |
| paint run | local in `Document::paint` | one paint | — root, lent to layout as `&mut` |

All `error: Option<Error>` fields and every per-type `note()` (eleven copies of the same
first-wins rule today) are deleted. Call sites become `self.errors.record(e)`. The policy
(cap, order, counting) exists only in `ErrorBag`.

No public builder gains a lifetime: `Flow` cannot borrow its parent's bag without becoming
`Flow<'b>`, which would leak into every closure signature. So each owns, and merges.

---

## 4. Module map

```text
error      Error, ErrorKind, Location, Errors (pub), Result (pub, = Errors)
           ErrorBag, Mark, Fallible (pub(crate))          ← no new deps; still only geom-free std
text       measure_text(fonts, errors: &mut ErrorBag, …)  ← soft MissingGlyph / UnknownFont
fonts      unchanged API, returns Fallible
canvas     Canvas methods: Fallible<()>                    ← spelled Result<(), Error> publicly
layout     LayoutCtx { fonts, errors: &mut ErrorBag } + LayoutCtx::located
           paragraph: soft BoldFaceRequired / UnknownFont line metrics
           paginate: split into layout_pages (plan+bands) and paint_pages
document   owners above; lower(self, &mut ErrorBag) -> Option<FlowNode>
           Document::paint is the only bag → Errors conversion
backend    unchanged, Fallible; to_pdf wraps with From<Error>
lib        document(), to_pdf() signatures use quire::Result
```

DAG unchanged: `error` gains no dependency; `text`, `layout`, `document` already depend on it.

Call chain for any error is at most three files: the recording site → its owner's `lower` →
`document_ty.rs`.

---

## 5. Walkthroughs

### 5.1 `note` (leaf builder)

```rust
// document/hline.rs (today: match … Err(e) => self.note(e))
pub fn thickness(&mut self, width: impl IntoPositive) {
    match width.into_positive() {
        Ok(width) => self.thickness = width,
        Err(error) => self.errors.record(error),
    }
}
```

Every mistake is kept. The value keeps its default so later calls still validate.

### 5.2 `Flow::push` and adding items

```rust
impl Flow {
    /// Appends a lowered node. On a closed document flow, records
    /// `ContentOutsideSection` once and drops the node.
    pub(crate) fn push(&mut self, node: FlowNode);

    /// Runs `f` on a child flow; merges its bag into `into`.
    /// `Some(nodes)` only when the child recorded nothing.
    pub(crate) fn nested(&self, into: &mut ErrorBag, f: impl FnOnce(&mut Flow))
        -> Option<Vec<FlowNode>>;

    /// Hands the bag and nodes to the section at lower time.
    pub(crate) fn into_parts(self) -> (Vec<FlowNode>, ErrorBag);
}

impl IntoFlowItem for Table {
    fn add_to(self, flow: &mut Flow) {
        if let Some(node) = self.lower(&mut flow.errors) {
            flow.push(node);
        }
    }
}
```

`push` takes a `FlowNode`, not a `Result`. "Ignore everything after the first error" is gone;
a bad sibling no longer hides the next one. A failed node is excluded from the success tree
because `lower` returned `None`; its siblings are still lowered and pushed.

`written()` becomes `!nodes.is_empty() || !errors.is_empty()`.

### 5.3 Container lower: collect siblings, then decide

```rust
impl Table {
    pub(crate) fn lower(self, into: &mut ErrorBag) -> Option<FlowNode> {
        let mark = into.mark();
        into.absorb(self.errors);          // own notes + cell errors, already in discovery order

        let widths = match resolve_widths(self.tracks, cols) {
            Ok(w) => Some(w),
            Err(e) => { into.record(e); None }   // keep going: lower cells to collect
        };
        let header = lower_rows(self.header, &self.parent, into);  // each cell: Option
        let body   = lower_rows(self.body,   &self.parent, into);

        if !into.clean_since(mark) { return None; }  // anything below failed: not in the tree
        let spec = TableSpec { header: header?, body: body?, ..TableSpec::new(widths?) };
        TableNode::new(spec).map_err(|e| into.record(e)).ok().map(FlowNode::Table)
    }
}
```

This is where the "container overrides earlier child" bug disappears structurally: the
container never chooses between its own error and a child's. There is only one ordered list,
and the container's lower-time errors are appended after everything its children recorded.

`List::lower` and `Columns::lower` follow the same five lines. `Columns.cols` becomes
`Vec<Option<(ColumnWidth, Vec<FlowNode>)>>`; a bad `gap` / `col_fixed` is recorded straight
into `columns.errors` instead of being smuggled in as an `Err` column.

### 5.4 Document assembly

```rust
pub struct Document<'a> {
    // …
    pub(crate) errors: ErrorBag,          // replaces `error: Option<Error>`
    outside_reported: bool,               // ContentOutsideSection at most once per document
}

impl<'a> Document<'a> {
    pub fn section(&mut self, f: impl FnOnce(&mut Section<'a>) -> Result<()>) {
        let index = /* next section index */;
        let mark = self.errors.mark();
        if lead_written {                       // implicit section already used
            self.absorb_implicit();             // its errors happened first
            self.report_outside();
        }
        let mut section = Section::new(style, self.doc);
        let returned = f(&mut section);         // always runs: point 6
        self.errors.absorb(section.take_errors());
        if let Err(errors) = returned { self.errors.absorb(errors.into()); }
        self.errors.locate_since(mark, &Location::section(index));
        if self.errors.clean_since(mark) { /* adopt section as today */ }
    }
}
```

Discovery order inside a section: the flow's bag (recorded while the closure ran), then
whatever the closure returned (the point where the caller's `?` stopped it, so last).

Implicit-section mode has one bag in the implicit flow; it is absorbed with section 0 at render
time, or at the moment it is discarded by an explicit `section()`. In explicit mode every
document-level recording (`fail_outside`, closed-flow pushes) first absorbs the closed flow's
bag, so doc-level and flow-level mistakes interleave in true order.

### 5.5 Measure: record and continue

```rust
// text/measure.rs
pub(crate) fn measure_text(
    fonts: &mut FontStore,
    errors: &mut ErrorBag,
    font: FontId,
    text: &str,
) -> Fallible<MeasuredText> {
    // face lookup: UnknownFont -> errors.record(e); return zero-advance glyphs (gid 0)
    for (start, scalar) in text.char_indices() {
        let gid = match charmap.map(scalar) {
            Some(gid) => gid,
            None => {
                errors.record(ErrorKind::MissingGlyph { font: font.as_u32(), scalar });
                GlyphId::NOTDEF            // .notdef advance keeps line breaking plausible
            }
        };
        // advance lookup failure (ErrorKind::Font) stays hard: `?`
    }
}

// layout/paragraph.rs, measure_inline
let bold = if needs_bold {
    match fonts.bold() {
        Ok(id) => id,
        Err(e) => { ctx.errors.record(e); body }   // BoldFaceRequired once per paragraph group
    }
} else { body };
```

Soft kinds are exactly the three confirmed ones, and the decision lives at the three call
sites above (all in `text/measure.rs` and `layout/paragraph.rs`). Everything else keeps `?`
and hard-stops. Nothing that is measured soft is ever drawn: once the bag is non-empty, paint
is skipped (5.6), so a `.notdef` stand-in can't reach a page.

Measure runs once per `TextNode`: nodes move through `split` by value and a page move hands
back the already-laid-out `ParagraphNode`, so soft errors are not recorded twice for the same
text. Band and layer closures run per page, so a missing glyph in a footer is reported once per
page with that page's location (bounded by the cap).

### 5.6 Location in layout: one boundary helper

```rust
impl LayoutCtx<'_> {
    /// Runs `f`; every error it records, and the hard error it returns, gets `at`
    /// filled in where the deeper code left it unknown.
    pub(crate) fn located<T>(
        &mut self,
        at: Location,
        f: impl FnOnce(&mut Self) -> Fallible<T>,
    ) -> Fallible<T> {
        let mark = self.errors.mark();
        let out = f(self);
        self.errors.locate_since(mark, &at);
        out.map_err(|mut e| { e.fill_missing(&at); e })
    }
}
```

Soft errors are recorded deep inside measurement, where page and region are unknown. Because
`located` locates everything recorded since its mark on the way out, the innermost scope fills
first and outer scopes only fill blanks: the same inner-wins rule `Location` has today. Every
`map_err(|e| e.in_region(..).on_page_index(..))` in `paginate.rs`, `decorate.rs` and the
`in_cell` in `layout/table.rs`'s cell measure becomes a `ctx.located(..)` call, so soft and
hard errors are located by the same code.

### 5.7 Paint → `Err`

```rust
// document_ty.rs — the single bag → Errors conversion
pub(crate) fn render(self, built: Result<()>) -> Result<Rendered> {
    let (mut errors, jobs, fonts, images) = self.lower_all();  // assembly bag, sections lowered
    if let Err(returned) = built {
        errors.absorb(returned.into());     // the caller aborted the build: last, and stop
        return Err(errors.finish(()).expect_err("absorbed a non-empty Errors"));
    }
    let mut ctx = LayoutCtx { fonts: &mut fonts, errors: &mut errors };
    let pages = match layout_pages(jobs, &mut ctx) {
        Ok(pages) => pages,
        Err(hard) => return Err(errors.fail(hard)),  // geometry/stalled: record, stop
    };
    let pages = errors.finish(pages)?;      // any soft or assembly error: no paint, no Rendered
    let mut recording = RecordingCanvas::new();
    paint_pages(&pages, &mut recording)?;   // canvas lifecycle: single Error -> From
    Ok(Rendered { recording, fonts, images })
}

pub fn document<'a>(page_size: PageSize, build: impl FnOnce(&mut Document<'a>) -> Result<()>)
    -> Result<Rendered>
{
    let mut doc = Document::new(page_size);
    // image registry swap as today
    let built = build(&mut doc);
    doc.render(built)
}
```

Decisions encoded here:

- Assembly errors don't stop layout. Lower already excluded the failed nodes; laying out the
  rest finds measure errors in other paragraphs (point 2: same bag, assembly first).
- The caller's own `Err` from `build` does stop layout. The caller asked to stop; the
  half-built document isn't worth measuring.
- Hard layout errors: recorded last, nothing after them.
- Paint only runs on a clean bag. Painting is the only step that can't be undone, and its
  failures (canvas lifecycle) are single by nature.
- `paginate_sections` is split into `layout_pages` (plan + `measure_bands`) and the existing
  `paint_pages`, because "stop before paint if the bag is dirty" needs a seam between them.

`to_pdf(rendered) -> Result<Vec<u8>>` is `backend::write_pdf(..).map_err(Errors::from)`.
Backend stays single-error.

---

## 6. What this deliberately does not do

- No dedup. Two paragraphs on page 3 missing the same glyph give two equal items. Discovery
  order is the contract; filtering is the caller's.
- No severity / soft-vs-hard flag on `ErrorKind`. Softness is decided by the recording site.
- No public builder for `Errors` and no way for callers to append. `From<Error>` is the only
  public constructor.
- No partial `Rendered`, no partial PDF, no parallel collection.
- No multi-error from inside a single check that today returns its first violation
  (`TableNode::new` span tiling, `measure_bands` span tiling). Those stay first-of-kind.

---

## 7. Red-flag screen

| Flag | Verdict |
| --- | --- |
| Shallow module | `Errors` is four accessors plus std traits; cap, ordering, counting, location filling and merge are hidden in `ErrorBag`. Callers learn one type. |
| Information leakage | The cap and the "inner wins" rule live once in `error.rs`. `Location` filling has one implementation (`fill_missing`); the old `in_*` builders wrap it. |
| Temporal decomposition | Bag is organized around ownership (who built what), not around assembly/measure/paint phases. Same `record` in all three. |
| Pass-through | `LayoutCtx::located` adds policy (mark + locate + map_err). `Flow::nested` adds merge + success decision. No forwarding-only method. |
| Split ownership | One owner per bag at any time; layout gets `&mut` lent from `Document::paint`. Child bags are moved into the parent, never shared. **Watch:** `in_*` wrappers survive for bag-less hard paths — two spellings of one rule, one implementation. |
| Two ways to do one task | One public result type for pipeline calls; single-cause calls return `Error` by rule (§2 table). Inside layout, location goes only through `ctx.located`. **Risk:** primitives returning `Error` while `quire::Result` is `Errors` could read as two ways; see open questions. |
| Importable internals | `ErrorBag`, `Mark`, `Fallible` are `pub(crate)`. |
| Hand-synced list | Eleven `note()` copies are deleted, not kept in sync. The soft set is not a list; it is three call sites. |
