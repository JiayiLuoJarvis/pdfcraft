# Candidate 2 — normalized cluster tape with range-only lines

## Problem

Knife 5 must turn source text into UAX #29 grapheme atoms, apply the fixed seven-class pair table, and break by a caller-supplied width without coupling to skrifa, Canvas, pagination, or the public DSL. The awkward boundary is knife 4: `MeasuredText` is deliberately scalar-based, maps every scalar through cmap, and uses byte ranges into its own text. A newline is `BK` and must not be drawn, but measuring it through the existing cmap may fail. This design therefore keeps line breaking pure and leaves `measure.rs` unchanged while giving it a newline-free scalar stream whose glyph indices remain directly addressable.

## Usage (caller's view)

### Pair-table behavior in a unit test

```rust
use crate::text::cluster::ClusteredText;
use crate::text::line_break::break_lines;

let text = ClusteredText::new("好（世");
let lines = break_lines(&text, 20.0, |cluster| {
    10.0 * cluster.scalar_count() as f32
})?;

assert_eq!(
    lines.iter().map(|line| line.text()).collect::<Vec<_>>(),
    ["好", "（世"],
);
assert_eq!(
    lines.iter().map(|line| line.width()).collect::<Vec<_>>(),
    [10.0, 20.0],
);
```

The test never constructs a `BreakClass` or a partly valid atom. Classification and the pair matrix stay behind `break_lines`.

### Normalization, forced breaks, and discarded leading spaces

```rust
let text = ClusteredText::new("A\r\n B");

assert_eq!(text.normalized_text(), "A\n B");
assert_eq!(text.glyph_text(), "A B"); // BK is not sent through cmap

let lines = break_lines(&text, 100.0, |cluster| {
    10.0 * cluster.scalar_count() as f32
})?;

assert_eq!(
    lines.iter().map(|line| line.text()).collect::<Vec<_>>(),
    ["A", "B"],
);
```

The same callback makes a base plus combining mark one indivisible cluster with width 20, because `scalar_count()` is two.

### Future paragraph flow using the existing scalar measurement

```rust
let clustered = ClusteredText::new(paragraph_text);
let measured = measure_text(fonts, font, clustered.glyph_text())?;
let size = font_size.as_pt();

let lines = break_lines(&clustered, frame_width.as_pt(), |cluster| {
    measured.glyphs[cluster.glyph_range()]
        .iter()
        .map(|glyph| glyph.advance_norm * size)
        .sum()
})?;

for line in lines.iter() {
    let glyphs = &measured.glyphs[line.glyph_range()];
    let draw_text = line.text();
    let source_bytes = line.glyph_byte_range();
    // PR8 rebases the selected glyph byte ranges to `draw_text` and places them.
    todo!("assemble the positioned glyph run");
}
```

`glyph_text`, `Cluster::glyph_range`, and `BrokenLine::glyph_byte_range` are one coordinate system. The caller does not remap scalars, classify punctuation, remove `BK`, or manufacture line-break atoms.

## Shape

### Core data

`ClusteredText` is an immutable cluster tape. It owns both normalized source and the exact newline-free string consumed by knife 4 measurement. Private cursors record cluster, glyph-scalar, and glyph-text byte positions once; line results are pairs of those cursors plus width.

```rust
// src/text/cluster.rs

use std::ops::Range;

pub(crate) struct ClusteredText {
    normalized: String,
    glyph_text: String,
    clusters: Box<[ClusterRecord]>,
}

struct ClusterRecord {
    normalized_bytes: Range<usize>,
    start: ClusterCursor,
    end: ClusterCursor,
    first_scalar: char,
}

#[derive(Clone, Copy)]
pub(super) struct ClusterCursor {
    pub(super) cluster: usize,
    pub(super) glyph: usize,
    pub(super) glyph_byte: usize,
}

#[derive(Clone, Copy)]
pub(crate) struct Cluster<'a> {
    owner: &'a ClusteredText,
    index: usize,
}

impl ClusteredText {
    /// Normalizes CRLF and CR, segments extended grapheme clusters, and builds
    /// `glyph_text` by omitting BK clusters. Infallible.
    pub(crate) fn new(source: &str) -> Self {
        todo!("normalize once; segment with UnicodeSegmentation::grapheme_indices")
    }

    pub(crate) fn normalized_text(&self) -> &str {
        &self.normalized
    }

    /// Text to pass unchanged to `measure_text`; contains every non-BK scalar
    /// in source order and no BK scalar.
    pub(crate) fn glyph_text(&self) -> &str {
        &self.glyph_text
    }

    fn cluster(&self, index: usize) -> Cluster<'_> {
        todo!("private indexed view")
    }
}

impl Cluster<'_> {
    /// Normalized grapheme text, including "\n" for a BK cluster.
    pub(crate) fn as_str(&self) -> &str {
        todo!()
    }

    /// Indices into `MeasuredText::glyphs` after measuring `glyph_text`.
    /// Empty for BK.
    pub(crate) fn glyph_range(&self) -> Range<usize> {
        todo!()
    }

    pub(crate) fn scalar_count(&self) -> usize {
        todo!()
    }

    pub(super) fn first_scalar(&self) -> char {
        todo!()
    }

    pub(super) fn start_cursor(&self) -> ClusterCursor {
        todo!()
    }

    pub(super) fn end_cursor(&self) -> ClusterCursor {
        todo!()
    }
}
```

The duplicated strings are intentional. `normalized` is the classification/source domain; `glyph_text` is the cmap/drawing domain. Their relation is captured only by private cursors, so no caller keeps parallel offset tables synchronized. Segmentation happens before `BK` removal, preventing text on opposite sides of a newline from accidentally forming a grapheme.

### Classification as data

`BreakClass` and its matrix are not general text API. They are policy owned by `break_class.rs` and consumed only by `line_break.rs` plus focused table tests.

```rust
// src/text/break_class.rs

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum BreakClass {
    Bk,
    Id,
    Al,
    Other,
    Sp,
    Op,
    Cl,
}

// Index order: ID, AL, Other, SP, OP, CL.
const PAIR_BREAKS: [[bool; 6]; 6] = [
    [true,  true,  true,  true,  true,  false], // ID
    [true,  false, false, true,  true,  false], // AL
    [true,  false, false, true,  true,  false], // Other
    [true,  true,  true,  true,  true,  false], // SP
    [false, false, false, false, false, false], // OP
    [true,  true,  true,  true,  true,  false], // CL
];

pub(super) fn classify(first_scalar: char) -> BreakClass {
    todo!("only the exact ranges and literals fixed in the design")
}

/// Looks up the matrix. BK is a control token and is never accepted here.
pub(super) fn allows_break(left: BreakClass, right: BreakClass) -> bool {
    todo!("map the six pair classes to indices and perform one lookup")
}
```

There is one source of truth for pair policy: `PAIR_BREAKS`. Matches may classify code points and map enum variants to table indices, but may not encode break decisions.

### One pure breaking operation

The public-in-crate operation accepts clusters, available width, and one width projection. It calls the projection exactly once for each non-BK cluster, caches the resulting private atom tape, validates that each projected width is finite and non-negative, and runs the fixed algorithm. `BK` has implicit zero width and never reaches the callback.

```rust
// src/text/line_break.rs

use std::ops::Range;

use crate::error::Result;
use crate::text::cluster::{Cluster, ClusterCursor, ClusteredText};

pub(crate) fn break_lines<'text>(
    text: &'text ClusteredText,
    avail: f32,
    width_of: impl FnMut(Cluster<'text>) -> f32,
) -> Result<BrokenLines<'text>> {
    todo!("validate avail, project private atoms once, then apply fixed steps 2–9")
}

pub(crate) struct BrokenLines<'text> {
    text: &'text ClusteredText,
    records: Box<[LineRecord]>,
}

struct LineRecord {
    start: ClusterCursor,
    end: ClusterCursor,
    width: f32,
}

#[derive(Clone, Copy)]
pub(crate) struct BrokenLine<'a> {
    text: &'a ClusteredText,
    record: &'a LineRecord,
}

impl BrokenLines<'_> {
    pub(crate) fn iter(
        &self,
    ) -> impl ExactSizeIterator<Item = BrokenLine<'_>> + '_ {
        todo!()
    }
}

impl BrokenLine<'_> {
    pub(crate) fn text(&self) -> &str {
        todo!("slice ClusteredText::glyph_text")
    }

    pub(crate) fn glyph_range(&self) -> Range<usize> {
        todo!()
    }

    pub(crate) fn glyph_byte_range(&self) -> Range<usize> {
        todo!()
    }

    pub(crate) fn width(&self) -> f32 {
        self.record.width
    }
}
```

`BrokenLines` always contains at least one record, encoding the empty-paragraph rule without an `Option`. A record contains no copied text or glyphs. Text slicing and glyph slicing are O(1); width projection is O(number of clusters); the nearest-break search sees only a private compact atom tape. `ClusterCursor` is the sole boundary coordinate type shared inside `text`; line breaking stores cursor pairs but cannot construct inconsistent cursor fields through its API.

The only new error is the fixed overflow error:

```rust
// src/error.rs

pub enum Error {
    // existing variants...
    UnbreakableWiderThanFrame {
        width: f32,
        avail: f32,
    },
}

// Display:
// "unbreakable width {width} exceeds available width {avail}"
```

Invalid `avail` remains `Error::InvalidGeometry`. No alignment enum belongs here: Start/Center/End consumes completed line widths in PR8 and does not affect this operation.

### Module map

```text
text::cluster
  └─ unicode-segmentation = "=1.13.3"

text::break_class
  └─ exact scalar ranges + the sole 6×6 pair matrix

text::line_break
  ├─ text::cluster (cluster views and cursor ranges)
  ├─ text::break_class (private classification policy)
  └─ error (InvalidGeometry, UnbreakableWiderThanFrame)

text::measure                    # unchanged in PR7
  └─ measures ClusteredText::glyph_text() one scalar per glyph

layout::paragraph                # PR8, not touched
  └─ joins measured glyphs to clusters through glyph_range()
```

This is a deep interface: callers perform one clustering construction and one breaking call; the modules hide CR normalization, BK elision, UAX #29 boundaries, classification, pair lookup, retry order, leading-space removal, and range bookkeeping. The caller exposes only the one fact line breaking cannot own—cluster width in the caller's chosen font and size. Per boundary-discipline, source normalization occurs at `ClusteredText::new`, available-width validation occurs at `break_lines`, and all internal records are trusted afterward. Per encode-lessons-in-structure, callers cannot construct `BreakClass`, `LineRecord`, cursors, or a malformed measured cluster. Per single-source-of-truth, the break matrix exists once and line slices derive from private cursor pairs.

Red-flag screen:

- **Shallow module:** avoided; there is no public atom builder or multi-step breaker protocol.
- **Information leakage:** avoided; the matrix and class ranges are private to one policy module, while newline removal and offset mapping are private to the cluster tape.
- **Temporal decomposition:** avoided; modules are split by owned knowledge (Unicode clustering, break policy, line selection), not by generic “prepare/run/finalize” stages.
- **Pass-through methods:** avoided; each exposed method either creates an invariant-bearing value or projects an O(1) view needed by measurement/drawing.

## Synthesis decision

N/A for runner candidate.

## Tradeoffs accepted

- We accept storing normalized text and newline-free glyph text in exchange for leaving knife 4 untouched, never cmap-mapping `BK`, and keeping glyph/byte lookup O(1).
- We accept a width callback tied to `Cluster` views in exchange for not exposing a reusable but weakly constrained `MeasuredCluster` construction protocol.
- We accept immutable derived byte and glyph cursors in each private record in exchange for avoiding rescans and caller-maintained offset maps.
- We accept losing byte offsets into the caller's pre-normalized `&str` in exchange for making normalized text the sole semantic source after the boundary.
- We accept raw `f32` at the internal breaking boundary in exchange for preserving the specified `InvalidGeometry` behavior and avoiding a PR7-only public length wrapper.

## Alternatives considered

- **Expose `Vec<MeasuredCluster<P>>` and return `Vec<Vec<MeasuredCluster<P>>>`.** This makes the algorithm implementation straightforward, but it is a shallow interface: every caller must normalize, segment, classify, validate widths, preserve payload identity, and decide how BK maps to glyphs. It leaks all internal stages while hiding little.
- **Add cluster arrays directly to `MeasuredText`.** This can make the eventual paragraph call short, but couples pure line breaking to skrifa's storage shape, forces PR7 to rewrite knife 4, and still leaves the newline-cmap problem to measurement. The chosen tape hides more policy with less cross-module knowledge.
- **Make `break_lines` generic over an `Atom` trait.** This appears flexible, but the trait necessarily exposes class, width, and identity methods that mirror the algorithm. Callers would need to understand implementation rules to implement it correctly; a single width projection is the only capability actually required.

## Open questions and risks

- Should a terminal `BK` produce a final empty line after the line it forces, or does step 3's emitted line exhaust the paragraph? The fixed prose specifies empty input but should pin this terminal case before tests encode one interpretation.
- Will later diagnostics require offsets into the original pre-CR-normalized input? If yes, should `ClusterRecord` retain an original-byte cursor now, or should diagnostics consistently name normalized offsets?
- Should a non-finite or negative callback result be treated as `InvalidGeometry`, or is the callback a trusted crate-private contract guarded only by `debug_assert!`? The choice affects only defensive behavior, not valid layout.

## Next implementation step

Implement `ClusteredText::new` and cursor invariants first, with tests for CRLF/CR normalization, BK-free `glyph_text`, combining clusters, and emoji ZWJ clusters before writing the class matrix or breaker.
