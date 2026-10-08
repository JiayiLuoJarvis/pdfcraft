//! Extended grapheme clusters (UAX #29) as line-break atoms.

#![allow(dead_code)]

use std::ops::Range;

use unicode_segmentation::UnicodeSegmentation;

use crate::text::break_class::{classify, BreakClass};

/// Immutable cluster tape over normalized text and BK-free glyph text.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ClusteredText {
    normalized: String,
    glyph_text: String,
    clusters: Box<[ClusterRecord]>,
}

#[derive(Debug, Clone, PartialEq)]
struct ClusterRecord {
    normalized_bytes: Range<usize>,
    start: ClusterCursor,
    end: ClusterCursor,
    first_scalar: char,
    class: BreakClass,
}

/// Boundary into cluster / glyph-scalar / glyph-byte space.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ClusterCursor {
    pub(crate) cluster: usize,
    pub(crate) glyph: usize,
    pub(crate) glyph_byte: usize,
}

/// Borrowed view of one cluster.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Cluster<'a> {
    owner: &'a ClusteredText,
    index: usize,
}

impl ClusteredText {
    /// Normalize CRLF, segment graphemes, omit BK from `glyph_text`.
    pub(crate) fn new(source: &str) -> Self {
        let normalized = normalize_line_endings(source);
        let mut glyph_text = String::new();
        let mut clusters = Vec::new();
        let mut glyph = 0usize;
        let mut glyph_byte = 0usize;

        for (byte_start, grapheme) in normalized.grapheme_indices(true) {
            let byte_end = byte_start + grapheme.len();
            let first = grapheme.chars().next().expect("grapheme non-empty");
            let class = classify(first);
            let start = ClusterCursor {
                cluster: clusters.len(),
                glyph,
                glyph_byte,
            };
            let is_hard = matches!(class, BreakClass::Hard);
            if !is_hard {
                glyph_text.push_str(grapheme);
                glyph += grapheme.chars().count();
                glyph_byte += grapheme.len();
            }
            let end = ClusterCursor {
                cluster: clusters.len() + 1,
                glyph,
                glyph_byte,
            };
            clusters.push(ClusterRecord {
                normalized_bytes: byte_start..byte_end,
                start,
                end,
                first_scalar: first,
                class,
            });
        }

        Self {
            normalized,
            glyph_text,
            clusters: clusters.into_boxed_slice(),
        }
    }

    pub(crate) fn normalized_text(&self) -> &str {
        &self.normalized
    }

    /// Scalars for `measure_text`: every non-BK scalar, source order.
    pub(crate) fn glyph_text(&self) -> &str {
        &self.glyph_text
    }

    pub(crate) fn len(&self) -> usize {
        self.clusters.len()
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.clusters.is_empty()
    }

    pub(crate) fn cluster(&self, index: usize) -> Cluster<'_> {
        assert!(index < self.clusters.len());
        Cluster { owner: self, index }
    }
}

impl<'a> Cluster<'a> {
    pub(crate) fn as_str(&self) -> &'a str {
        let record = &self.owner.clusters[self.index];
        &self.owner.normalized[record.normalized_bytes.clone()]
    }

    pub(crate) fn glyph_range(&self) -> Range<usize> {
        let record = &self.owner.clusters[self.index];
        record.start.glyph..record.end.glyph
    }

    pub(crate) fn scalar_count(&self) -> usize {
        self.as_str().chars().count()
    }

    pub(crate) fn first_scalar(&self) -> char {
        self.owner.clusters[self.index].first_scalar
    }

    pub(crate) fn class(&self) -> BreakClass {
        self.owner.clusters[self.index].class
    }

    pub(crate) fn start_cursor(&self) -> ClusterCursor {
        self.owner.clusters[self.index].start
    }

    pub(crate) fn end_cursor(&self) -> ClusterCursor {
        self.owner.clusters[self.index].end
    }
}

fn normalize_line_endings(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        match ch {
            '\r' => {
                if chars.peek() == Some(&'\n') {
                    chars.next();
                }
                out.push('\n');
            }
            other => out.push(other),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crlf_becomes_single_bk_and_glyph_text_omits_it() {
        let text = ClusteredText::new("A\r\nB");
        assert_eq!(text.normalized_text(), "A\nB");
        assert_eq!(text.glyph_text(), "AB");
        assert_eq!(text.len(), 3);
        assert!(matches!(text.cluster(1).class(), BreakClass::Hard));
        assert!(text.cluster(1).glyph_range().is_empty());
    }

    #[test]
    fn combining_mark_is_one_cluster() {
        let text = ClusteredText::new("e\u{0301}X");
        assert_eq!(text.len(), 2);
        assert_eq!(text.cluster(0).as_str(), "e\u{0301}");
        assert_eq!(text.cluster(0).scalar_count(), 2);
        assert_eq!(text.cluster(0).glyph_range(), 0..2);
        assert_eq!(text.glyph_text(), "e\u{0301}X");
    }
}
