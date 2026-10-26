# Sibling crate type coarseness and Google-style inventory

Scope: `pdfcraft-forms`, `pdfcraft-post`, `pdfcraft-orgchart`. This is a
documentation cross-check only. No product API changes, no migrations.

Contrast source: `.omo/plans/pdfcraft-table-row-api.md` and the prior
`.omo/plans/pdfcraft-pdf-type-coarseness.md` scan, which dealt with the
`TableSpec` anonymous-axis question in `pdfcraft-pdf`. The sibling crates have
no equivalent authoring axis, so this file records that result per type and
points Bar B at the closed Google-style work instead of re-arguing it.

## Verdict

**No P0 Bar A issue exists on any sibling crate, so no breaking fix and no
migration markdown are required.** Draft IS-2 (a P0 fix plus migration md) is
**vacuous and superseded**: there is no P0, hence there is nothing to migrate.

Positive pattern: orgchart `ChartRequest { nodes, edges }`
(`crates/pdfcraft-orgchart/src/model.rs`) already names both collections with
typed elements (`Vec<Node>`, `Vec<Edge>`). That is the shape the pdf TableRow
work was reaching for, and it already exists here.

## Bar A: anonymous-axis inventory

Bar A asks one question per public authoring type: does it carry an anonymous
intermediate axis of `TableSpec` class (a nested `Vec<Vec<_>>`, an untyped row
bag, or a positional tuple struct that callers must index)? Classes used below:
OK (no such axis), mild (bundled fields worth noting, not actionable),
Too-coarse (needs a breaking fix; none found).

### Bar A: forms (`crates/pdfcraft-forms/src/fields.rs`)

| Type | Site | Class | Reason |
| --- | --- | --- | --- |
| `FieldType` | `fields.rs` | OK | Named enum of widget kinds, `#[non_exhaustive]`, no nesting |
| `FieldInfo` | `fields.rs` | OK | Named struct; `options: Option<Vec<String>>` is a flat choice list, not a nested axis |
| `FieldValue` | `fields.rs` | OK | `Text(String)` / `CheckBox(bool)`; wildcard-arm documented, no row bag |
| `FontBytes<'a>` | `fields.rs` | OK | `bytes` plus `index`; two named fields |
| `MissingName` | `fields.rs` | OK | Binary `Error` / `Ignore` policy, no axis |
| `HAlign` / `VAlign` | `fields.rs` | OK | Small named enums for alignment |
| `AbsoluteLine` | `fields.rs` | mild | Flat named struct; r/g/b triple noted under Mild notes, not a nested axis |
| `FillOptions<'a>` | `fields.rs` | mild | Named struct with parallel per-field override maps; keyed by name, not positional nesting |

No `Too-coarse` entry in forms. The override maps
(`h_align_overrides`, `v_align_overrides`, `font_size_overrides`,
`line_below`) are `Option<&HashMap<String, _>>`: keyed lookups, so they do not
form an anonymous axis. See Non-issues.

### Bar A: post (`crates/pdfcraft-post/src/`)

| Type | Site | Class | Reason |
| --- | --- | --- | --- |
| `SetMetadata` | `metadata.rs` | OK | Eight named `Option` fields; `keywords: Option<Vec<String>>` is a flat list |
| `EncryptionConfig` | `encryption.rs` | OK | Named permission bools mirroring PDF security bits; flat, documented |
| `EncryptionPreset` | `encryption.rs` | OK | Unit struct of named const presets, no axis |
| `Encrypt` | `encryption.rs` | OK | `config` plus per-call password overrides; all named |
| `StampAndEncrypt` | `combo.rs` | OK | Named composition of metadata plus encrypt |
| `Step` | `pipeline.rs` | OK | Named enum (`Metadata` / `Encrypt` / `StampAndEncrypt`) |
| `Pipeline` | `pipeline.rs` | OK | `new(impl IntoIterator<Item = Step>)` over named steps; sequential, not nested |
| `merge_pdfs(parts, preserve_outlines)` | `merge.rs` | OK | `parts` is a flat list of PDFs; the bool is a documented scalar flag, not an axis |

No `Too-coarse` entry in post. `merge_pdfs` takes one flat parts list, so there
is no `Vec<Vec<_>>` authoring shape. The `preserve_outlines` bool stays a bool
per the disposition Q1 decision cited under Bar B.

### Bar A: orgchart (`crates/pdfcraft-orgchart/src/model.rs`, `page.rs`)

| Type | Site | Class | Reason |
| --- | --- | --- | --- |
| `ChartRequest` | `model.rs` | OK | `nodes: Vec<Node>`, `edges: Vec<Edge>`; both named and typed |
| `Node` | `model.rs` | OK | Named struct with named optional facts; no positional tuple |
| `Edge` | `model.rs` | OK | `from_id` / `to_id` plus optional `ratio`; named |
| `ChartOptions` | `model.rs` | OK | Named `Option` fields; `None` means unset through folding |
| `FontBytes` | `model.rs` | OK | Shared `Arc<Vec<u8>>` buffer; single named field |
| `NodeType` | `model.rs` | OK | `Person` / `Company` with `as_str` token |
| `PlacedNode` / `CueSlot` | `page.rs` | mild | Loose `x` / `y` / `w` / `h` scalars; layout output, not an authoring axis |
| `LayoutResult` | `page.rs` | OK | Named `nodes` / `edges` / `slots` plus bounds; flat collections |
| `LaidOutEdge` | `page.rs` | OK | `points: Vec<PointMm>` is a polyline, a geometric path rather than a table axis |
| `ErrorCode` / `Error` | `error.rs` | OK | Stable code plus log-only message; not an authoring type |

No `Too-coarse` entry in orgchart. Internal nesting such as graph adjacency
maps or wrapped text lines never reaches the public authoring surface; see
Non-issues.

## Bar B: Google-style residual (done, cited not re-argued)

Bar B is closed. Both waves below are finished; this file only points at the
record in `docs/google-rust-style-disposition.md`. Do not re-open Q1 through Q3,
t5, or t7 on the basis of this inventory.

- Scope line: disposition covers `pdfcraft-pdf` plus `pdfcraft-forms` in full,
  with `pdfcraft-orgchart` / `pdfcraft-post` production code frozen except for
  the gate runs; language: Google-wins per `AGENTS.md`.
- forms full wave: `fields.rs` public types carry `#[must_use]`; `FieldType`,
  `FieldInfo`, `FieldValue` carry `#[non_exhaustive]`; `AbsoluteLine` gained
  `Default`; `FillOptions` keeps `..Default::default()` FRU shape.
- orgchart and post zero-breaking wave: eight-command gate matrix exit 0 with
  doc zero warning; no-default-features and font/CJK gaps recorded as
  base-identical, per the Known gate gaps section.
- Deferred t5 (`FieldValue::CheckBox(bool)` stays bool), t7 (pdf document
  language enums keep total match), and orgchart/post Q1 through Q3 (post
  `merge_pdfs` bool stays, `EncryptionConfig` bools stay, `ErrorCode` stays
  exhaustive without `#[non_exhaustive]`) remain as decided in the disposition
  Deferred section.
- Known gate gaps (no-default-features dead-code errors, font/CJK coverage
  without local fixtures) are baseline conditions shared with the pristine
  tree, not regressions from this inventory.

## Mild notes (non-actionable, no todos)

These are hygiene observations only. They are out of execution scope and must
not be turned into breaking API tasks.

- `AbsoluteLine` r/g/b: three parallel `f32` channels could one day become a
  color struct, but the flat named fields are clear and stable. Leave as is.
- `FillOptions` parallel override maps: four `Option<&HashMap<String, _>>`
  fields share a key domain, yet each has its own documented `missing` rule.
  Keyed maps are not positional nesting. Leave as is.
- orgchart layout output loose `x` / `y` / `w` / `h`: `PlacedNode`,
  `CueSlot`, and `LayoutResult` bounds expose scalar geometry. That fits a
  layout result type and is not an authoring axis. Leave as is.

## Non-issues (stop re-proposing these)

- Internal nesting is not Bar A: glyph runs, wrapped lines, graph adjacency,
  and page-tree remapping stay inside the crate and never appear as public
  `Vec<Vec<_>>` authoring fields or signatures.
- Keyed `HashMap<String, _>` override maps are not positional rows. Callers
  look up by field name; there is no index-based axis to name.
- Mild field bundling (RGB triples, scalar rect fields, password overrides)
  is not `TableSpec` class. Bundling becomes actionable only when callers must
  pass anonymous nested positions, which no sibling API requires.
- Google-style deferred items (t5, t7, Q1 through Q3) are decided in
  `docs/google-rust-style-disposition.md`. Re-proposing `CheckboxState`,
  `OutlinePolicy`, `EncryptionConfig` enums, or `ErrorCode`
  `#[non_exhaustive]` needs a fresh proposal-driven plan, not this inventory.
