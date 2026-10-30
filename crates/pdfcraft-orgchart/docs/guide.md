# pdfcraft-orgchart guide

How to build equity and outbound-investment charts with `pdfcraft-orgchart`:
request shape, fonts, validation, titles, options, errors, and testing. For a
one-page overview see the [crate README](../README.md). For every public item
signature see [rustdoc](https://docs.rs/pdfcraft-orgchart)
(`cargo doc -p pdfcraft-orgchart --open` locally).

Maintainer behavior contracts live in the workspace
[`docs/pdfcraft-orgchart-spec.md`](../../../docs/pdfcraft-orgchart-spec.md). This
guide is the caller-facing how-to.

---

## Mental model

You pass a graph protocol (subject, nodes, holding edges, options) and one font
face as bytes. The library validates, folds optional `top_n` shareholders,
measures with skrifa, paginates, and draws with krilla.

```text
ChartRequest + FontBytes
  →  validate_request   (optional; no font)
  →  generate_pdf       →  PDF bytes

ChartRequest
  →  document_title     →  String (you store PDF /Info yourself)
```

**Dual cone.** Edges mean `from_id` holds `to_id`. Shareholders sit **above**
the subject. Outbound investments sit **below**.

This crate does not depend on `pdfcraft-pdf` or `pdfcraft-forms`.

```rust
use pdfcraft_orgchart::{
    document_title, generate_pdf, validate_request, ChartRequest, Edge, FontBytes, Node,
};

fn chart(font_data: Vec<u8>) -> Result<Vec<u8>, pdfcraft_orgchart::Error> {
    let font = FontBytes::new(font_data);
    let request = ChartRequest::new(
        "root",
        vec![
            Node::new("holder", "甲股东"),
            Node::new("root", "主体公司"),
            Node::new("invest", "乙投资"),
        ],
        vec![Edge::new("holder", "root"), Edge::new("root", "invest")],
    );
    validate_request(&request)?;
    let _title = document_title(&request)?;
    generate_pdf(&request, &font)
}
```

---

## ChartRequest

| Field | Role |
| --- | --- |
| `subject_id` | Id of the company the chart is about |
| `nodes` | Parties. Order is the tie-break order for a `top_n` fold |
| `edges` | Holding edges. `from_id` holds `to_id` |
| `options` | Width, margin, title, `max_nodes`, `top_n` |

`ChartRequest::new(subject_id, nodes, edges)` starts with default options.

### Node

| Field | Role |
| --- | --- |
| `id` / `name` | Required non-blank after strip |
| `legal_person` | Optional. Affects display and company fill |
| `ratio` | Optional `0..=100` on the node |
| `node_type` | `Person` or `Company` when set |
| `paid_in_amount` / `currency` / `location` / `approval_date` | Investment detail lines (not parsed) |

`Node::new(id, name)` leaves the optional fields as `None`.

### Edge

| Field | Role |
| --- | --- |
| `from_id` / `to_id` | Holder → held |
| `ratio` | Optional `0..=100` on the edge |

`Edge::new(from, to)` leaves ratio as `None`.

### ChartOptions

| Field | Default behavior |
| --- | --- |
| `node_width_mm` | Unset → `38` mm at draw time. Legal set range `[20, 80]` |
| `margin_mm` | Unset → `8` mm. Legal set range `(0, 40]` |
| `title` | Used by `document_title` only. Does **not** override page headers |
| `max_nodes` | Unset → `MAX_NODES` (`100`). Set values must be in `1..=100` |
| `top_n` | `None` / `Some(0)` do not fold. `Some(n)` for `n >= 1` keeps at most `n` direct holders per shareholder parent. Dropped ratios are summed like CPython `math.fsum` |

Constants: `MAX_NODES`, `MIN_NODE_WIDTH_MM`, `MAX_NODE_WIDTH_MM`, `MAX_MARGIN_MM`.

---

## Fonts

```rust
use std::sync::Arc;

use pdfcraft_orgchart::FontBytes;

let owned = FontBytes::new(std::fs::read("face.ttf")?);
let shared = Arc::new(std::fs::read("face.ttf")?);
let reused = FontBytes::from_shared(shared);
```

- `FontBytes::new` wraps owned bytes. `FontBytes::from_shared` reuses an `Arc<Vec<u8>>`.
- Paths are not accepted.
- One face per call. TrueType collections / OTC are rejected (`FONT_ERROR`).
- Measurement and drawing share the same glyph pipeline. Missing cmap entries
  fail with `MISSING_GLYPH`.
- Fonts are not vendored in the published package. Tests and the example need
  `PDFCRAFT_ORGCHART_FONT` or `PDFCRAFT_CJK_FONT`.

---

## validate_request

Runs the prepare step (including `top_n` fold rules) and drops the folded
request. It does **not** open a font. It does **not** re-validate the folded
graph.

A fold that adds a synthetic node can succeed here even when the folded node
list would exceed `max_nodes`. `generate_pdf` folds again and validates that
expanded graph, so the overflow fails at draw time.

Use `validate_request` for cheap protocol checks before you load a font.

---

## document_title

Looks up `subject_id` in `nodes` and returns a title string. It does not run
the full protocol check.

| `options.title` | Result |
| --- | --- |
| `None` | `{subject name}股权架构图` |
| `Some("")` | empty string, kept |
| `Some(other)` | that string |

`generate_pdf` does **not** write a PDF document info dictionary. If you need
`/Title`, call `document_title` and write info in your own PDF layer.

Page headers always come from chart role captions, not from `options.title`.

---

## generate_pdf

Pipeline (observable stages):

```text
ChartRequest
  → prepare (top_n fold)
  → validate graph
  → display + ratio labels
  → measure node boxes
  → paginate (may split, continue, annex)
  → render → PDF bytes
```

Paper is A4. Shareholders above the subject, investments below. The main
title starts at the page margin. The subtitle sits 1.5 mm below that line. A
header line that does not fit the printable width ends with `…`. Chinese page
headers and footers follow the behavior contract in the workspace spec.

---

## Display rules (caller-visible)

- **Roles.** Each node is subject, shareholder (inbound cone), or investment
  (outbound cone).
- **Box lines.** Ratios do **not** appear inside boxes. Shareholder / subject
  boxes show name and optional legal person. Investment boxes show name,
  paid-in amount, location, and approval date when present.
- **Ratio labels.** Text is `xx.xx%`. The label sits 1.2 mm to the right of a
  stub that hugs the box. A node with no hugging stub is unlabeled, and so is
  the page subject (including a promoted annex subject). Shareholders sit just
  below the box: baseline is the box bottom plus the 9 pt ascent plus 0.4 mm.
  Investments sit just above the box: baseline is the box top minus the 9 pt
  descent minus 0.4 mm. That gap is not a full line height. The first fact for
  a node is the one that is drawn.
- **Strokes.** Node boxes and continuation boxes are black. The subject border
  is 1.5 pt. An ordinary node border is 0.8 pt. Edges, edge-arrow fills,
  continuation leaders, and leader-arrow fills are 0.2 gray.
- **Continuation marks.** The dashed box uses a 1.0 pt stroke, a 1.0 mm corner,
  and a 4 on / 3 off dash. Its text is centered in the slot. A mark placed
  beside its anchor draws a 4.5 pt leader at 30 degrees to that anchor.
- **Company fill.** Gray fill when `node_type == Company`, or when `node_type`
  is unset and `legal_person` is non-blank. `Person` is always white.
- **`top_n` fold.** Excess direct shareholders become a synthetic
  `其他股东(k家)` node. Investments are not folded by `top_n`. The dropped
  ratios use the same finite sum as CPython `math.fsum`, so six copies of
  `100/6` fold to 100. A sum outside `0..=100` is `InvalidProtocol`.

---

## Errors

Match on `Error::code` (`ErrorCode`). `Error::message` is for logs. Its wording
is not a stable contract. `ErrorCode::as_str` matches the stable protocol
strings (`INVALID_PROTOCOL`, …).

| Code | When |
| --- | --- |
| `InvalidProtocol` | Protocol or `top_n` fold rule broken |
| `CycleDetected` | Directed holding cycle |
| `SubjectNotFound` | `subject_id` missing from nodes (also `document_title`) |
| `TooManyNodes` | Node list over the active cap (including after fold) |
| `FontError` | Face bytes unusable or a collection |
| `MissingGlyph` | Character absent from the font |
| `NodeTooTall` | A node box exceeds the portrait height limit |
| `LayoutFailed` | Internal layout invariant broken |

There is no HTTP-oriented `to_dict`. Map codes in your application layer.

---

## Examples and tests

```bash
# Needs PDFCRAFT_ORGCHART_FONT or PDFCRAFT_CJK_FONT (single-face)
cargo run -p pdfcraft-orgchart --example equity_chart
cargo test -p pdfcraft-orgchart
```

Prefer assertions on PDF magic, error codes, fold behavior, and extractable
caption text. Geometry screenshot gates are out of v1.

OFL notes for a common Source Han reference face are in
[tests/fixtures/fonts/](../tests/fixtures/fonts/).

---

## What this crate does not do

- Depend on `pdfcraft-pdf` or `pdfcraft-forms`
- Circular cross-holdings as a product feature, cross-page connector lines, or
  custom node row layouts
- JSON / serde as the primary API
- Process-wide default font paths
- Write PDF `/Info` for you
- Accept font collections

---

## Crate map

| Path | Role |
| --- | --- |
| `validate_request` / `generate_pdf` / `document_title` | Public entrypoints |
| `ChartRequest`, `Node`, `Edge`, `ChartOptions`, `FontBytes` | Values |
| `Error` / `ErrorCode` | Failures |
| `examples/` | Runnable demos |
| `docs/guide.md` | This guide |
| Workspace `docs/pdfcraft-orgchart-spec.md` | Maintainer reference |
