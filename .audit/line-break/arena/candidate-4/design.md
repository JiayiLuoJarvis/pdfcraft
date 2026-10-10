# Candidate 4: Contiguous Line Spans over Immutable Clusters with Direct Matrix Backtracking

## Problem

Knife 5 must turn raw or measured text into line boundaries under a strict 7-class Unicode partition and a 6×6 pair table, with no shaping, bidi, or UAX #14. Line-break atoms are extended grapheme clusters whose widths are the exact sum of Knife 4 scalar advances, while `line_break` itself must remain a pure algorithm with zero skrifa or krilla awareness. The architectural challenge is defining the data shape of break atoms and line outputs: callers (both PR7 stub tests and future PR8 paragraph flow) need a frictionless path from text or `MeasuredText` to positioned glyph runs, without two-phase mutation, without redundant intermediate tapes, and without allocating auxiliary boolean arrays for decisions that are easily queried from a static matrix.

## Usage (caller's view)

Callers never orchestrate classification, grapheme segmentation, and matrix queries manually. They either break monospace stub strings directly in unit tests or pass `MeasuredText` in layout flow. Break outputs provide contiguous slice projections into both clusters and measured glyphs.

**Call site A — PR7 monospace stub tests (no `FontStore`):**

```rust
use quire::text::line_break;

// Monospace stub: every BMP scalar = 10 font units, upem 1000 -> 0.01 norm per scalar.
let (clusters, lines) = line_break::break_stub("好（世界", 0.01, 0.05)?;

// W = 0.05 fits 5 scalars. "好（世" would be 3 scalars, but "（世" cannot be broken (OP|ID forbidden).
// Therefore, the break occurs before "（" (ID|OP allowed).
assert_eq!(lines.len(), 2);
assert_eq!(lines[0].text(&clusters, "好（世界"), "好");
assert_eq!(lines[0].width, 0.01);
assert_eq!(lines[1].text(&clusters, "好（世界"), "（世界");
assert_eq!(lines[1].width, 0.03);
```

**Call site B — Decision-table boundary tests (pure class and matrix assertions):**

```rust
use quire::text::break_class::{BreakClass, BreakPairTable};

// Classification:
assert_eq!(BreakClass::classify('好'), BreakClass::Id);
assert_eq!(BreakClass::classify('（'), BreakClass::Op);
assert_eq!(BreakClass::classify('）'), BreakClass::Cl);
assert_eq!(BreakClass::classify('A'), BreakClass::Al);
assert_eq!(BreakClass::classify(' '), BreakClass::Sp);
assert_eq!(BreakClass::classify('\n'), BreakClass::Bk);

// Pair rules:
assert!(BreakPairTable::can_break(BreakClass::Id, BreakClass::Op));  // 好（ -> allowed before （
assert!(!BreakPairTable::can_break(BreakClass::Op, BreakClass::Id)); // （世 -> forbidden before 世
assert!(!BreakPairTable::can_break(BreakClass::Id, BreakClass::Cl)); // 字。 -> forbidden before 。
assert!(BreakPairTable::can_break(BreakClass::Cl, BreakClass::Id));  // 。字 -> allowed before 字
assert!(!BreakPairTable::can_break(BreakClass::Al, BreakClass::Al)); // AB   -> forbidden
assert!(BreakPairTable::can_break(BreakClass::Al, BreakClass::Sp));  // A_   -> allowed before space
```

**Call site C — Line-break behavioral edge cases (spaces, CRLF, unbreakable overflow):**

```rust
use quire::error::Error;
use quire::text::line_break;

// 1. CRLF is normalized to a single BK; empty lines generate 0-width line spans.
let (_, lines) = line_break::break_stub("line 1\r\n\r\nline 2", 0.01, 1.0)?;
assert_eq!(lines.len(), 3);
assert_eq!(lines[1].width, 0.0);
assert_eq!(lines[1].cluster_range.len(), 0);

// 2. Trailing spaces fit on current line; overflow space drops at start of next line.
let (clusters, lines) = line_break::break_stub("hello world", 0.01, 0.055)?;
assert_eq!(lines[0].text(&clusters, "hello world"), "hello");
assert_eq!(lines[1].text(&clusters, "hello world"), "world");

// 3. Overflow without break point returns UnbreakableWiderThanFrame.
let err = line_break::break_stub("unbreakable", 0.01, 0.05).unwrap_err();
assert_eq!(err, Error::UnbreakableWiderThanFrame { width: 0.11, avail: 0.05 });
```

**Call site D — Future PR8 paragraph flow (contiguous glyph slice into `GlyphRun`):**

```rust
use quire::text::line_break;
use quire::text::measure::measure_text;

let measured = measure_text(&mut fonts, font_id, user_text)?;
let (clusters, lines) = line_break::break_measured(&measured, avail_width_pt)?;

for line in &lines {
    // Zero-copy: glyphs in a broken line form a single contiguous slice.
    let line_glyphs = &measured.glyphs[line.glyph_range.clone()];
    
    // PR8 flow can place glyphs directly:
    // let x_origin = align.offset(avail_width_pt, line.width);
    // canvas.draw_glyph_run(&GlyphRun { ... });
}
```

## Shape

### Module map

```text
src/text/
  break_class.rs    // 7 BreakClass variants, scalar classification, const 6x6 BreakPairTable
  cluster.rs        // CRLF normalization, UAX #29 graphemes, immutable Cluster construction
  line_break.rs     // Pure 9-step greedy packer, backtracking, LineSpan output
  measure.rs        // Knife 4 scalar measurement (unchanged)
  mod.rs            // Internal module re-exports
```

```text
Dependency DAG:
  cluster    ──► break_class
  line_break ──► cluster + break_class
  (PR8 flow) ──► measure + cluster + line_break
```

`line_break` imports `BreakPairTable` directly to evaluate breaks on demand during backtracking, eliminating the need to preallocate or synchronize an intermediate seams vector. Neither `cluster`, `break_class`, nor `line_break` imports skrifa, krilla, or canvas types.

### Data structures (load-bearing)

#### 1. `BreakClass` and `BreakPairTable` (`src/text/break_class.rs`)

`BreakClass` is a compact `#[repr(u8)]` enum covering the 7 categories defined in the design specification. Only the 6 non-`BK` classes map to indices in the 6×6 decision table.

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub(crate) enum BreakClass {
    Bk,
    Sp,
    Id,
    Al,
    Op,
    Cl,
    Other,
}

impl BreakClass {
    /// Classifies a Unicode scalar according to the design specification ranges.
    pub(crate) const fn classify(scalar: char) -> Self {
        // TODO: Matches:
        // '\n' => Bk
        // ' ' => Sp
        // U+3400..=U+4DBF | U+4E00..=U+9FFF | U+3040..=U+309F | U+30A0..=U+30FF
        // | U+FF66..=U+FF9D | U+AC00..=U+D7AF | '\u{3000}' => Id
        // '0'..='9' | 'A'..='Z' | 'a'..='z'
        // | '\u{00C0}'..='\u{00D6}' | '\u{00D8}'..='\u{00F6}' | '\u{00F8}'..='\u{00FF}' => Al
        // '(' | '[' | '{' | '（' | '【' | '《' | '「' | '『' | '〈' | '〔' | '｛' => Op
        // ')' | ']' | '}' | '）' | '，' | '。' | '、' | '；' | '：' | '！' | '？' | ... => Cl
        // _ => Other
        todo!()
    }

    /// Index into the 6×6 pair matrix. `None` for `Bk` (which never enters the matrix).
    #[inline]
    pub(crate) const fn matrix_index(self) -> Option<usize> {
        match self {
            Self::Id => Some(0),
            Self::Al => Some(1),
            Self::Other => Some(2),
            Self::Sp => Some(3),
            Self::Op => Some(4),
            Self::Cl => Some(5),
            Self::Bk => None,
        }
    }
}

/// Fixed 6×6 break pair decision matrix.
///
/// Encodes the design specification table directly as data.
/// Rows represent `prev`, columns represent `next`.
pub(crate) struct BreakPairTable;

impl BreakPairTable {
    pub(crate) const MATRIX: [[bool; 6]; 6] = [
        // ID:    ID=T, AL=T, Other=T, SP=T, OP=T, CL=F
        [true,  true,  true,  true,  true,  false],
        // AL:    ID=T, AL=F, Other=F, SP=T, OP=T, CL=F
        [true,  false, false, true,  true,  false],
        // Other: ID=T, AL=F, Other=F, SP=T, OP=T, CL=F
        [true,  false, false, true,  true,  false],
        // SP:    ID=T, AL=T, Other=T, SP=T, OP=T, CL=F
        [true,  true,  true,  true,  true,  false],
        // OP:    ID=F, AL=F, Other=F, SP=F, OP=F, CL=F
        [false, false, false, false, false, false],
        // CL:    ID=T, AL=T, Other=T, SP=T, OP=T, CL=F
        [true,  true,  true,  true,  true,  false],
    ];

    /// Queries whether a line break is allowed between `prev` and `next` (before `next`).
    #[inline]
    pub(crate) const fn can_break(prev: BreakClass, next: BreakClass) -> bool {
        match (prev.matrix_index(), next.matrix_index()) {
            (Some(p), Some(n)) => Self::MATRIX[p][n],
            _ => false,
        }
    }
}
```

#### 2. `Cluster` and Cluster Construction (`src/text/cluster.rs`)

An extended grapheme cluster is the atomic unit of line breaking. In this candidate, clusters are **immutable and complete upon construction**: widths are fully calculated up front, eliminating two-phase stamping and zero-width placeholder states.

```rust
use std::ops::Range;
use crate::error::Result;
use crate::text::break_class::BreakClass;
use crate::text::measure::MeasuredText;

/// Atomic unit of line breaking (one UAX #29 extended grapheme cluster).
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Cluster {
    /// Break class derived from the first scalar in the cluster.
    pub(crate) class: BreakClass,
    /// Total width of this cluster (sum of scalar advances).
    /// Always 0.0 for `Bk`.
    pub(crate) width: f32,
    /// Byte range in the normalized source string.
    pub(crate) byte_range: Range<usize>,
    /// Index range into `MeasuredText::glyphs` (empty for stub tests or `Bk`).
    pub(crate) glyph_range: Range<usize>,
}

impl Cluster {
    #[inline]
    pub(crate) fn is_hard_break(&self) -> bool {
        self.class == BreakClass::Bk
    }

    #[inline]
    pub(crate) fn is_space(&self) -> bool {
        self.class == BreakClass::Sp
    }
}

/// Normalizes `\r\n` and isolated `\r` into a single `\n` (`BK`).
pub(crate) fn normalize_crlf(text: &str) -> String {
    // TODO: Single-pass scan replacing \r\n and \r with \n
    todo!()
}

/// Constructs measured clusters using a uniform scalar width (for PR7 stub tests).
pub(crate) fn cluster_stub(text: &str, scalar_width: f32) -> (String, Vec<Cluster>) {
    // TODO:
    // 1. let normalized = normalize_crlf(text);
    // 2. Segment with unicode_segmentation::UnicodeSegmentation::graphemes(&normalized, true).
    // 3. For each grapheme:
    //    first scalar -> BreakClass::classify(first)
    //    width = if class == Bk { 0.0 } else { grapheme.chars().count() as f32 * scalar_width }
    //    glyph_range = 0..0
    // 4. Return (normalized, clusters)
    todo!()
}

/// Constructs measured clusters from knife 4 `MeasuredText`.
pub(crate) fn cluster_measured(
    normalized_text: &str,
    measured: &MeasuredText,
) -> Result<Vec<Cluster>> {
    // TODO:
    // 1. Walk graphemes on normalized_text.
    // 2. Map grapheme byte ranges to measured.glyphs.
    // 3. Bk clusters carry width 0.0 and glyph_range 0..0 (Bk is never drawn).
    // 4. Printable clusters aggregate advance_norm from corresponding glyphs.
    todo!()
}
```

#### 3. `LineSpan` and `break_lines` (`src/text/line_break.rs`)

`line_break` is a pure function over a slice `&[Cluster]`. It outputs `LineSpan`s which identify contiguous sub-slices of clusters, bytes, and glyphs.

```rust
use std::ops::Range;
use crate::error::{Error, Result};
use crate::text::break_class::{BreakClass, BreakPairTable};
use crate::text::cluster::{Cluster, cluster_stub, cluster_measured, normalize_crlf};
use crate::text::measure::MeasuredText;

/// Sliced projection of a broken line.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct LineSpan {
    /// Half-open range into the input `&[Cluster]` slice.
    pub(crate) cluster_range: Range<usize>,
    /// Byte range in the normalized source string.
    pub(crate) byte_range: Range<usize>,
    /// Contiguous glyph range in `MeasuredText::glyphs`.
    pub(crate) glyph_range: Range<usize>,
    /// Total formatted width of the line (sum of cluster widths in cluster_range).
    pub(crate) width: f32,
}

impl LineSpan {
    /// Helper to extract the line's text slice from the normalized string.
    pub(crate) fn text<'a>(&self, _clusters: &[Cluster], source: &'a str) -> &'a str {
        &source[self.byte_range.clone()]
    }
}

/// Pure line breaking algorithm adhering to steps 1–9 of the specification.
pub(crate) fn break_lines(clusters: &[Cluster], avail_width: f32) -> Result<Vec<LineSpan>> {
    if !avail_width.is_finite() || avail_width <= 0.0 {
        return Err(Error::InvalidGeometry);
    }

    // Step 9: Empty text yields one empty line with zero width.
    if clusters.is_empty() {
        return Ok(vec![LineSpan {
            cluster_range: 0..0,
            byte_range: 0..0,
            glyph_range: 0..0,
            width: 0.0,
        }]);
    }

    let mut lines = Vec::new();
    let mut line_start = 0;
    let mut current_idx = 0;
    let mut line_width = 0.0;

    while current_idx < clusters.len() {
        // Step 4: Drop leading U+0020 SP when current line is empty.
        if line_start == current_idx && clusters[current_idx].is_space() {
            current_idx += 1;
            line_start = current_idx;
            continue;
        }

        let c = &clusters[current_idx];

        // Step 3: Hard break (BK) terminates current line immediately; BK is not drawn.
        if c.is_hard_break() {
            lines.push(build_line_span(clusters, line_start..current_idx, line_width));
            current_idx += 1;
            line_start = current_idx;
            line_width = 0.0;
            continue;
        }

        // Step 5: Fits within available width.
        if line_width + c.width <= avail_width {
            line_width += c.width;
            current_idx += 1;
            continue;
        }

        // Step 6: c does not fit. Backtrack to find the nearest allowed break point.
        // Candidates: between clusters inside the line, and between current line end and c.
        let mut found_break = None;
        for k in (line_start + 1..=current_idx).rev() {
            let prev_class = clusters[k - 1].class;
            let next_class = clusters[k].class;
            if BreakPairTable::can_break(prev_class, next_class) {
                found_break = Some(k);
                break;
            }
        }

        if let Some(split_k) = found_break {
            // Step 7: Left of split_k is emitted as current line.
            // Right of split_k (including c) becomes input for the next line.
            let committed_width: f32 = clusters[line_start..split_k].iter().map(|cl| cl.width).sum();
            lines.push(build_line_span(clusters, line_start..split_k, committed_width));
            line_start = split_k;
            current_idx = split_k;
            line_width = 0.0;
        } else {
            // Step 8: No break point found. Content is unbreakable and wider than frame.
            let unbreakable_width = line_width + c.width;
            return Err(Error::UnbreakableWiderThanFrame {
                width: unbreakable_width,
                avail: avail_width,
            });
        }
    }

    // Flush final line if any uncommitted clusters remain.
    if line_start < clusters.len() {
        lines.push(build_line_span(clusters, line_start..clusters.len(), line_width));
    }

    Ok(lines)
}

fn build_line_span(clusters: &[Cluster], range: Range<usize>, width: f32) -> LineSpan {
    if range.is_empty() {
        return LineSpan {
            cluster_range: range.clone(),
            byte_range: 0..0,
            glyph_range: 0..0,
            width: 0.0,
        };
    }
    let byte_range = clusters[range.start].byte_range.start..clusters[range.end - 1].byte_range.end;
    let glyph_range = clusters[range.start].glyph_range.start..clusters[range.end - 1].glyph_range.end;
    LineSpan {
        cluster_range: range,
        byte_range,
        glyph_range,
        width,
    }
}

/// Convenience runner for stub tests.
pub(crate) fn break_stub(
    text: &str,
    scalar_width: f32,
    avail_width: f32,
) -> Result<(Vec<Cluster>, Vec<LineSpan>)> {
    let (normalized, clusters) = cluster_stub(text, scalar_width);
    let lines = break_lines(&clusters, avail_width)?;
    Ok((clusters, lines))
}

/// Convenience runner for measured text in paragraph flow.
pub(crate) fn break_measured(
    measured: &MeasuredText,
    avail_width: f32,
) -> Result<(Vec<Cluster>, Vec<LineSpan>)> {
    let normalized = normalize_crlf(&measured.text);
    let clusters = cluster_measured(&normalized, measured)?;
    let lines = break_lines(&clusters, avail_width)?;
    Ok((clusters, lines))
}
```

#### 4. Error surface addition (`src/error.rs`)

Only the single variant mandated by Knife 5 is added:

```rust
    /// An unbreakable sequence of clusters is wider than the available width.
    UnbreakableWiderThanFrame {
        /// Width of the unbreakable sequence that exceeded the frame.
        width: f32,
        /// Available frame width.
        avail: f32,
    },
```

Display formatting:
```rust
Self::UnbreakableWiderThanFrame { width, avail } => {
    write!(f, "unbreakable content width {width} exceeds available width {avail}")
}
```

### Invariants encoded in types

1. **No half-initialized cluster states**: `Cluster` has no mutable `width` field and requires all measurements at construction. A cluster cannot exist in an unmeasured state (per *encode-lessons-in-structure*).
2. **Contiguous ink spans**: `BK` (hard break) and dropped `SP` (leading spaces) are consumed outside line boundaries. All clusters in `LineSpan::cluster_range` are contiguous ink elements. Thus, `line.glyph_range` is a strictly contiguous slice into `MeasuredText::glyphs`.
3. **Stateless pair queries**: `BreakPairTable` is a const type with zero runtime allocations. Pair checks are pure functions of `(prev, next)` with O(1) table lookup complexity.
4. **Boundary validation**: Available width `W` is validated on function entry (`is_finite() && W > 0.0`) returning `Error::InvalidGeometry`. Inside `break_lines`, geometry is trusted (per *boundary-discipline*).

### Interface depth

- **What the public/caller surface hides**:
  - The entire UAX #29 grapheme cluster boundary logic and CRLF state handling.
  - The Unicode scalar classification tables and 6×6 pair matrix.
  - The 9-step line packing, backtracking search, and leading space suppression.
- **What remains exposed**:
  - `break_stub` and `break_measured` convenience runners.
  - Pure `break_lines` accepting `&[Cluster]` for custom pipelines.
  - `LineSpan` returning indices and aggregated line widths.
- **Why the interface is deep**:
  Callers provide text and a width budget, receiving fully partitioned lines and contiguous glyph ranges. Callers do not loop over internal tokens, coordinate multi-stage pipelines, or stitch fragmented sub-slices.

## Synthesis decision

N/A (arena runner candidate).

## Tradeoffs accepted

- **We accept storing `glyph_range: Range<usize>` on each `Cluster` (adding 16 bytes per atom)**, in exchange for O(1) contiguous glyph slicing in PR8 paragraph layout. The paragraph builder extracts `&measured.glyphs[line.glyph_range]` as a single slice without iterating over individual clusters.
- **We accept evaluating `can_break` during backtracking rather than precomputing a parallel `seams: Vec<bool>`**, in exchange for zero heap allocations for seam tables, lower memory bandwidth, and eliminating synchronization bugs between cells and seams.
- **We accept returning `(Vec<Cluster>, Vec<LineSpan>)` from convenience wrappers**, in exchange for keeping `break_lines` an entirely borrowed slice operation `&[Cluster] -> Result<Vec<LineSpan>>`.
- **We accept eager normalization of CRLF before grapheme segmentation**, in exchange for guaranteeing that `\r\n` is always a single `BK` atom and never leaks `\r` into font cmap lookups or produces two consecutive break events.
- **We accept leaving alignment (`Start`, `Center`, `End`) out of Knife 5**, in exchange for zero coupling with vertical flow, frame origins, or Canvas drawing (as dictated by the arena frame constraints).

## Alternatives considered

- **BreakTape with precomputed seams and mutable stamping (Candidate 1)**:
  Precomputes an auxiliary `Vec<bool>` for all adjacent pairs and creates unmeasured cells that must be mutated in-place by a stamping pass.
  *Why it lost on interface depth and structural purity*: Stamping introduces temporal decomposition (holding half-initialized zero-width tapes). The parallel `Vec<bool>` allocates extra heap memory for decisions that are rarely queried. Returning tape cell ranges forces the downstream paragraph layouter to iterate through every cell in every line to gather glyphs, scattering line-assembly logic across layers.
- **Lazy pull iterator / Streaming line breaker**:
  Streams clusters via an iterator and yields lines on demand.
  *Why it lost*: Step 6 requires backwards backtracking across the current line when an atom overflows. A pure pull iterator would have to buffer the active line internally anyway, adding ring-buffer complexity without reducing memory usage. Slice-based inputs provide direct random access during backtracking with zero runtime abstraction penalty.
- **In-place string slicing without dedicated `Cluster` structs**:
  Attempting to break lines directly on string slices or `MeasuredText` offsets.
  *Why it lost*: Grapheme clusters can span multiple Unicode scalars (e.g., base character + combining marks, emoji ZWJ sequences). Intertwining grapheme boundary logic with the 9-step packing state machine inside a single loop creates a monolithic, fragile function that is hard to test and verify against the specification table.

## Open questions and risks

- Does PR8 paragraph flow need `LineMetrics` attached directly to each `LineSpan`, or should `LineMetrics` remain independently calculated via `measure::line_metrics` per paragraph block?
- If documents contain exceptionally long paragraphs (e.g., 50,000 words in a single block), will storing `Cluster` structs with two `Range<usize>` fields incur measurable memory pressure, or is the flat `Vec<Cluster>` cache-locality benefit sufficient?
- Should `Cluster::class` be exposed as `pub(crate)` or behind an accessor method if future knives introduce inline styling runs that split clusters?

## Next implementation step

Add `unicode-segmentation = "=1.13.3"` to `Cargo.toml`, add `Error::UnbreakableWiderThanFrame` to `src/error.rs`, and write unit tests in `src/text/break_class.rs` for scalar classification and the 6×6 pair matrix against the design specification table.
