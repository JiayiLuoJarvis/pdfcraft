# PR7 candidate 1: BreakTape with precomputed seams

## Problem

Knife 5 must turn normalized Unicode text into lines under a fixed seven-class model and a 6×6 pair matrix, without shaping, bidi, or UAX #14. Knife 4 already measures **per scalar** and will keep doing so; break atoms are **extended grapheme clusters**, and cluster width is the sum of scalar advances already stored in `MeasuredText`. The non-obvious shape question is where `BreakClass` and the pair table live relative to the width-packing loop: callers (PR7 tests, later `layout/paragraph`) need one coherent story from `&str` or `MeasuredText` to line boundaries, while `line_break` must stay free of skrifa/krilla and the algorithm steps 1–9 must remain pure. This candidate pushes **all class/table knowledge into tape construction** and makes `line_break` a seam-only packer that never imports classification rules.

## Usage (caller's view)

PR7 tests and future paragraph flow should not orchestrate “cluster, then classify each head, then look up pairs while breaking.” They build or receive a **`BreakTape`**, optionally stamp widths from measurement, then call one breaker.

**Quickstart (monospace stub tests — no `FontStore`):**

```rust
use quire::text::cluster;
use quire::text::line_break;

let tape = cluster::break_tape("好（世界");
// Every BMP scalar = 10 font units, upem 1000 → advance_norm 0.01 per scalar.
cluster::stamp_uniform_norm(&mut tape, 0.01);
let plan = line_break::break_tape(&tape, 0.05)?; // W = 5× stub unit
// plan.lines[0].cells → ink cells before `（`; seam before `（` was allowed (ID|OP).
```

**Call site A — PR7 decision-table tests (class + seam assertions):**

```rust
use quire::text::break_class::BreakClass;
use quire::text::cluster;

let tape = cluster::break_tape("字。");
assert_eq!(tape.head_class(0), BreakClass::Id);
assert_eq!(tape.head_class(1), BreakClass::Cl);
assert!(!tape.may_break_after(0)); // ID|CL 禁止：句号不能离开「字」
// No seam after the final cell; CL|ID allowed on a longer string after 「。」
```

**Call site B — line-break behavior with stub widths (`hello `, CRLF, empty):**

```rust
use quire::text::cluster;
use quire::text::line_break;

let mut tape = cluster::break_tape("hello \r\nworld");
cluster::stamp_uniform_norm(&mut tape, 0.01);
let plan = line_break::break_tape(&tape, 0.055)?;
assert_eq!(plan.lines.len(), 2);
// First line ends before trailing SP; next line drops leading SP per algorithm step 4.
```

**Call site C — future paragraph (knife 4 measure unchanged, glue in `line_break`):**

```rust
use quire::text::line_break;
use quire::text::measure;

let measured = measure::measure_text(&mut fonts, font_id, user_text)?;
let plan = line_break::break_measured(&measured, avail_width_pt)?;
for line in &plan.lines {
    for cell in line.ink_cells(&plan.tape) {
        let glyphs = &measured.glyphs[cell.glyph_range.clone()];
        // PR8: place glyphs at x += sum(advance_norm) * font_size …
    }
}
```

Imports stay under `crate::text::*`; nothing here is public DSL. `break_measured` is the paragraph-facing convenience; tests may use `break_tape` directly to isolate the packer.

## Shape

### Module map

| Module | Owns | Does not |
| --- | --- | --- |
| `text::break_class` | `BreakClass`, scalar→class ranges, `BreakPairTable` + `allowed(prev, next)` | Grapheme boundaries, widths, line assembly |
| `text::cluster` | CRLF normalization, UAX #29 segmentation, `BreakTape` + seam precomputation, width stamping helpers | Pair-table logic duplicated elsewhere, breaking |
| `text::line_break` | Validate `W`, steps 1–9 over `BreakTape`, `LineBreakPlan`, `break_measured` adapter | `BreakClass` classification, unicode-segmentation |
| `text::measure` | Unchanged knife 4 scalar measure | Clusters (no rewrite in PR7) |
| `error` | Add `UnbreakableWiderThanFrame { width, avail }` | — |

Dependency DAG: `cluster → break_class`; `line_break → cluster` (tape type only); `line_break::break_measured → measure + cluster`. **`line_break` does not `use break_class`.**

### Data structures (load-bearing)

**`BreakClass`** — `#[repr(u8)]` enum with variants `Bk, Sp, Id, Al, Op, Cl, Other`. Only six variants participate in the pair matrix; `Bk` is handled as a tape opcode, not a matrix row/column.

```rust
// break_class.rs
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub(crate) enum BreakClass { Bk, Sp, Id, Al, Op, Cl, Other }

impl BreakClass {
    /// Index into the 6×6 matrix; `None` for `Bk`.
    pub(crate) const fn matrix_index(self) -> Option<usize> { todo!() }
}

pub(crate) fn classify_first(scalar: char) -> BreakClass { todo!() }

/// Zero-sized token; all policy in private const data.
pub(crate) struct BreakPairTable;

impl BreakPairTable {
    /// Fixed design table; `debug_assert` valid indices in debug builds.
    pub(crate) const fn allowed(prev: BreakClass, next: BreakClass) -> bool { todo!() }
}
```

Table storage (implementation detail, not public): `const MATRIX: [[bool; 6]; 6]` or equivalent bit-packed `u64`, keyed only through `matrix_index()`. **Single source of truth** for “允许/禁止” (per encode-lessons-in-structure).

**`BreakTape`** — contiguous break atom stream plus **precomputed seams** (whether a break may occur after cell `i` and before cell `i+1`).

```rust
// cluster.rs
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum TapeCell {
    /// BK: forces line end; not drawn; zero width.
    HardBreak,
    /// Printable cluster: one UAX #29 grapheme after normalization.
    Ink {
        head_class: BreakClass,
        /// UTF-8 range in the normalized source string used to build this tape.
        byte_range: std::ops::Range<usize>,
        /// Sum of `advance_norm` for scalars in `byte_range`; 0 until stamped.
        width_norm: f32,
        /// Inclusive index range into `MeasuredText::glyphs` after stamping from measure.
        glyph_range: std::ops::Range<usize>,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct BreakTape {
    /// Normalized text (`\r\n` / `\r` → `\n`). Cells index into this string.
    pub(crate) source: String,
    pub(crate) cells: Vec<TapeCell>,
    /// `seams[i]` == may break after `cells[i]` before `cells[i+1]`.
    /// Length `cells.len().saturating_sub(1)`. For HardBreak boundaries, seams are ignored (HardBreak always breaks).
    seams: Vec<bool>,
}

impl BreakTape {
    pub(crate) fn head_class(&self, cell: usize) -> BreakClass { todo!() }
    pub(crate) fn may_break_after(&self, cell: usize) -> bool { todo!() }
    pub(crate) fn ink_cell(&self, cell: usize) -> Option<&TapeCell> { todo!() }
}

pub(crate) fn normalize_line_endings(text: &str) -> String { todo!() }

/// Normalize, segment with `unicode_segmentation::UnicodeSegmentation`, classify heads, build seams via `BreakPairTable`.
pub(crate) fn break_tape(text: &str) -> BreakTape { todo!() }

/// Test helper: each scalar in each ink cell gets the same `width_per_scalar_norm`.
pub(crate) fn stamp_uniform_norm(tape: &mut BreakTape, width_per_scalar_norm: f32) { todo!() }

/// Production: walk ink `byte_range`s against `measured.text` / `measured.glyphs` (scalar order must match).
pub(crate) fn stamp_from_measured(tape: &mut BreakTape, measured: &measure::MeasuredText) -> Result<()> { todo!() }
```

Seams are computed once when cells are appended: for adjacent ink cells `a`, `b`, `seam = BreakPairTable::allowed(a.head_class, b.head_class)`. Adjacency involving `HardBreak` does not use the matrix (step 3). **Dominant read pattern for breaking:** sequential scan of `cells` + random access to `seams[i]` when backtracking — both O(1) (per boundary-discipline: validate tape invariants at build, trust inside breaker).

**`LineBreakPlan`** — output is **indices into the tape**, not copied substring/glyph data.

```rust
// line_break.rs
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct BrokenLine {
    /// Half-open range of `BreakTape::cells` belonging to this line (ink only; HardBreak never inside).
    pub(crate) cell_range: std::ops::Range<usize>,
    /// Sum of `width_norm` for ink cells in range (excludes dropped leading SP on next line).
    pub(crate) width_norm: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct LineBreakPlan {
    pub(crate) tape: BreakTape,
    pub(crate) lines: Vec<BrokenLine>,
}

pub(crate) fn break_tape(tape: &BreakTape, avail_width: f32) -> Result<LineBreakPlan> { todo!() }

/// Normalize+segment+stamp from `measured.text`, then break. Paragraph default entry.
pub(crate) fn break_measured(measured: &measure::MeasuredText, avail_width: f32) -> Result<LineBreakPlan> { todo!() }
```

`break_tape` validates `avail_width` (`is_finite() && avail_width > 0`) → `Error::InvalidGeometry` (per design). On failure step 8 → `Error::UnbreakableWiderThanFrame { width, avail }`.

```rust
// error.rs (addition only)
UnbreakableWiderThanFrame {
    width: f32,
    avail: f32,
},
```

### Algorithm placement

| Step | Where |
| --- | --- |
| 1 CRLF normalize | `cluster::break_tape` (via `normalize_line_endings`) |
| 2–5, 7–9 width pack + SP drop + backtrack | `line_break::break_tape` — uses `seams`, `HardBreak`, `head_class == Sp` only for step 4 |
| 6 pair lookup | Already materialized in `seams` at tape build |
| 8 error width | Running sum from line start through overflow cell (ink widths only) |
| 9 empty paragraph | `break_tape` on `""` → one `BrokenLine { cell_range: 0..0, width_norm: 0.0 }` |

`line_break` still reads `BreakClass::Sp` on ink cells for the leading-space discard rule; it **does not** call `classify_first` or `BreakPairTable`.

### `text/mod.rs` re-exports (crate-internal)

```rust
pub(crate) use break_class::{BreakClass, classify_first, BreakPairTable};
pub(crate) use cluster::{BreakTape, TapeCell, break_tape, normalize_line_endings, stamp_from_measured, stamp_uniform_norm};
pub(crate) use line_break::{LineBreakPlan, BrokenLine, break_tape, break_measured};
```

No new public `quire` exports until DSL PRs; PR7 tests live in `tests/` or `src/text/*/tests` via `pub(crate)` paths.

### Interface depth

- **Callers** see: build tape (or `break_measured`), pass `W`, get `LineBreakPlan` with cell ranges to map back to glyphs. They do **not** see the 6×6 matrix, grapheme rules, or backtracking policy.
- **`break_class`** hides entire range taxonomy and table encoding behind `classify_first` + `BreakPairTable::allowed`.
- **`cluster`** hides `unicode-segmentation` and seam derivation behind `break_tape`.
- **`line_break`** hides the nine-step state machine; the only “knob” is `avail_width`.

Complexity deliberately **not** exposed: matrix indices, per-step pipeline hooks, optional justify/alignment (deferred to PR8 flow).

### Invariants encoded in types

- `HardBreak` vs `Ink` separates BK from printable atoms (BK never carries width).
- `seams.len() == cells.len().saturating_sub(1)` enforced in `break_tape` debug assertions.
- `stamp_from_measured` returns `Err` if glyph count / byte ranges drift (missing glyph already failed in measure).
- Illegal `W` rejected at `break_tape` / `break_measured` entry only.

### Deliberately out of scope (PR7)

Alignment (`Start`/`Center`/`End`), pagination, DSL, rewriting `measure_text` to emit clusters, full UAX #14 classes, hyphenation, trimming trailing spaces globally.

## Synthesis decision

N/A (arena runner candidate).

## Tradeoffs accepted

- We accept **two-phase width attachment** (build tape, then stamp) in exchange for **tests that never touch `FontStore`** while keeping `measure.rs` untouched.
- We accept **duplicating scalar iteration** in `stamp_from_measured` (walk ink byte ranges + glyph slice) in exchange for **not** folding measurement into cluster types.
- We accept **`BreakTape` owning a copy of normalized `String`** in exchange for stable `byte_range` and simple paragraph glue without lifetime parameters on plans.
- We accept **precomputed `seams` memory (`O(n)`)** in exchange for a **`line_break` module with zero pair-table code** and fewer branches during backtracking.
- We accept **`line_break` still knowing `BreakClass::Sp`** for step 4 in exchange for not inventing a second “leading ignorable” flag on cells.

## Alternatives considered

- **Live pair lookup during break** (`Vec<(BreakClass, f32)>` + `BreakPairTable` inside `line_break`): smaller tape, but leaks table policy into the packer and duplicates class visibility at the hottest loop; rejected for shallower `line_break` and single seam derivation site.
- **Measured cluster structs inside `measure.rs`**: one type for glyph span + class + width, but forces knife 4 rewrite and couples skrifa path to UAX #29; rejected per arena frame (“do not rewrite measure unless necessary”).
- **Caller-orchestrated pipeline** (`clusters()`, then `classify()`, then `break()`): exposes temporal stages and scatters invariants; rejected as temporal decomposition on the test/paragraph surface.

## Open questions and risks

- Should `stamp_from_measured` live in `cluster` (as sketched) or in `line_break` beside `break_measured` to keep `cluster` free of `MeasuredText` imports?
- For emoji ZWJ clusters classified as `Other`, confirm stub tests use uniform width per scalar (not per cluster) so width equals sum of stub advances — does `stamp_uniform_norm` count scalars via `byte_range` char iteration?
- `LineBreakPlan` embeds `BreakTape` by value for convenience; is cloning plans in PR8 acceptable, or should lines hold only `Range<usize>` into an external tape?
- Grapheme segmentation on normalized string: confirm `break_tape` segments the owned normalized buffer so `byte_range` indices match `measured.text` when paragraph passes original text that needed CRLF fix — does `break_measured` re-normalize `measured.text` and require callers to have measured the same normalized string?

## Next implementation step

Add `BreakClass`, `BreakPairTable`, and `classify_first` with unit tests for documented edge scalars (×, ÷, U+3000, U+FF21, CJK ext B surrogate pairs as `Other`), then implement `break_tape` + seam construction before any `line_break` logic.
