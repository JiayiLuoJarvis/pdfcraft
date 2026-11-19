//! Extended grapheme clusters (UAX #29) as line-break atoms.

use std::ops::Range;

use crate::text::break_class::{self, BreakClass};

/// Tests whether a scalar is a Unicode variation selector (VS1..VS256).
pub(crate) fn is_variation_selector(scalar: char) -> bool {
    matches!(scalar, '\u{FE00}'..='\u{FE0F}' | '\u{E0100}'..='\u{E01EF}')
}

/// Immutable cluster tape over normalized text and BK-free glyph text.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ClusteredText {
    normalized: String,
    glyph_text: String,
    clusters: Box<[ClusterRecord]>,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct TaggedClusteredText<T> {
    clustered: ClusteredText,
    glyph_tags: Box<[T]>,
}

#[derive(Debug, Clone, PartialEq)]
struct ClusterRecord {
    normalized_bytes: Range<usize>,
    start: ClusterCursor,
    end: ClusterCursor,
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
        let (normalized, _) = normalize_runs([((), source)]);
        Self::from_normalized(normalized)
    }

    fn from_normalized(normalized: String) -> Self {
        use unicode_segmentation::UnicodeSegmentation;
        let mut glyph_text = String::new();
        let mut clusters = Vec::new();
        let mut glyph = 0usize;
        let mut glyph_byte = 0usize;
        let mut prev_class = None;

        for (byte_start, grapheme) in normalized.grapheme_indices(true) {
            let byte_end = byte_start + grapheme.len();
            #[expect(
                clippy::expect_used,
                reason = "Unicode grapheme clusters contain at least one scalar"
            )]
            let first = grapheme.chars().next().expect("grapheme non-empty");
            let static_class = break_class::classify_scalar(first);
            let class = break_class::resolve_pair(prev_class, static_class);
            prev_class = match class {
                BreakClass::Pair(pair) => Some(pair),
                BreakClass::Hard => None,
            };
            let start = ClusterCursor {
                cluster: clusters.len(),
                glyph,
                glyph_byte,
            };
            let omit_from_glyphs = matches!(
                static_class,
                break_class::StaticClass::Hard | break_class::StaticClass::Zwsp
            );
            if !omit_from_glyphs {
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

    #[cfg(test)]
    pub(crate) fn is_empty(&self) -> bool {
        self.clusters.is_empty()
    }

    pub(crate) fn cluster(&self, index: usize) -> Cluster<'_> {
        assert!(
            index < self.clusters.len(),
            "cluster index {index} is in range"
        );
        Cluster { owner: self, index }
    }
}

impl<T> TaggedClusteredText<T> {
    pub(crate) fn clustered(&self) -> &ClusteredText {
        &self.clustered
    }

    pub(crate) fn glyph_tags(&self) -> &[T] {
        &self.glyph_tags
    }
}

pub(crate) fn cluster_tagged<T: Copy, S: AsRef<str>>(
    runs: impl IntoIterator<Item = (T, S)>,
) -> TaggedClusteredText<T> {
    let (normalized, normalized_tags) = normalize_runs(runs);
    let clustered = ClusteredText::from_normalized(normalized);
    let mut glyph_tags = Vec::with_capacity(clustered.glyph_text().chars().count());
    let mut tag_cursor = 0usize;
    for index in 0..clustered.len() {
        let cluster = clustered.cluster(index);
        let count = cluster.scalar_count();
        if !cluster.glyph_range().is_empty() {
            if let Some(slice) = normalized_tags.get(tag_cursor..tag_cursor + count) {
                glyph_tags.extend_from_slice(slice);
            }
        }
        tag_cursor += count;
    }
    debug_assert_eq!(
        tag_cursor,
        normalized_tags.len(),
        "every normalized tag must be consumed by clusters"
    );
    debug_assert_eq!(
        glyph_tags.len(),
        clustered.glyph_text().chars().count(),
        "every glyph char must carry exactly one tag"
    );
    TaggedClusteredText {
        clustered,
        glyph_tags: glyph_tags.into_boxed_slice(),
    }
}

impl<'a> Cluster<'a> {
    pub(crate) fn as_str(&self) -> &'a str {
        let Some(record) = self.owner.clusters.get(self.index) else {
            return "";
        };
        self.owner
            .normalized
            .get(record.normalized_bytes.clone())
            .unwrap_or("")
    }

    pub(crate) fn glyph_range(&self) -> Range<usize> {
        let Some(record) = self.owner.clusters.get(self.index) else {
            return 0..0;
        };
        record.start.glyph..record.end.glyph
    }

    pub(crate) fn scalar_count(&self) -> usize {
        self.as_str().chars().count()
    }

    pub(crate) fn class(&self) -> BreakClass {
        self.owner
            .clusters
            .get(self.index)
            .map_or(BreakClass::Hard, |c| c.class)
    }

    pub(crate) fn start_cursor(&self) -> ClusterCursor {
        self.owner.clusters.get(self.index).map_or(
            ClusterCursor {
                cluster: 0,
                glyph: 0,
                glyph_byte: 0,
            },
            |c| c.start,
        )
    }

    pub(crate) fn end_cursor(&self) -> ClusterCursor {
        self.owner.clusters.get(self.index).map_or(
            ClusterCursor {
                cluster: 0,
                glyph: 0,
                glyph_byte: 0,
            },
            |c| c.end,
        )
    }
}

#[derive(Clone, Copy)]
enum ScanState {
    EmptyOrBreak,
    AfterChar(char),
    AfterVs,
}

fn normalize_runs<T: Copy, S: AsRef<str>>(
    runs: impl IntoIterator<Item = (T, S)>,
) -> (String, Vec<T>) {
    let mut text = String::new();
    let mut tags = Vec::new();
    let mut scalars = runs
        .into_iter()
        .flat_map(|(tag, s)| {
            let s = s.as_ref().to_owned();
            s.chars()
                .enumerate()
                .map(move |(i, c)| (c, tag, i == 0))
                .collect::<Vec<_>>()
        })
        .peekable();

    let mut state = ScanState::EmptyOrBreak;
    while let Some((scalar, tag, is_span_start)) = scalars.next() {
        if is_span_start {
            state = ScanState::EmptyOrBreak;
        }
        state = match (state, scalar) {
            (_, '\u{FEFF}' | '\u{200C}' | '\u{200D}') => ScanState::EmptyOrBreak,
            (_, '\r') => {
                if matches!(scalars.peek(), Some(('\n', _, _))) {
                    scalars.next();
                }
                text.push('\n');
                tags.push(tag);
                ScanState::AfterChar('\n')
            }
            (_, '\u{2028}' | '\u{2029}') => {
                text.push('\n');
                tags.push(tag);
                ScanState::AfterChar('\n')
            }
            (state @ ScanState::AfterChar('\n' | ' ' | '\u{3000}' | '\u{200B}'), vs)
                if is_variation_selector(vs) =>
            {
                state
            }
            (ScanState::AfterChar(_), vs) if is_variation_selector(vs) => {
                text.push(vs);
                tags.push(tag);
                ScanState::AfterVs
            }
            (state @ (ScanState::EmptyOrBreak | ScanState::AfterVs), vs)
                if is_variation_selector(vs) =>
            {
                state
            }
            (_, other) => {
                text.push(other);
                tags.push(tag);
                ScanState::AfterChar(other)
            }
        };
    }
    (text, tags)
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

    #[test]
    fn tagged_clusters_keep_each_scalars_source_across_run_boundaries() {
        let text = cluster_tagged([(0, "e"), (1, "\u{0301}X")]);
        assert_eq!(text.clustered().len(), 2);
        assert_eq!(text.clustered().cluster(0).glyph_range(), 0..2);
        assert_eq!(text.glyph_tags(), [0, 1, 1]);
    }

    #[test]
    fn tagged_crlf_across_runs_is_one_bk_without_a_glyph_tag() {
        let text = cluster_tagged([(0, "A\r"), (1, "\nB")]);
        assert_eq!(text.clustered().normalized_text(), "A\nB");
        assert_eq!(text.clustered().glyph_text(), "AB");
        assert_eq!(text.glyph_tags(), [0, 1]);
    }

    #[test]
    fn r3_invisible_characters_stripped() {
        let text = ClusteredText::new("A\u{FEFF}\u{200C}\u{200D}B");
        assert_eq!(text.normalized_text(), "AB");
        assert_eq!(text.glyph_text(), "AB");
    }

    #[test]
    fn r3_line_separators_normalized_to_hard_break() {
        let text = ClusteredText::new("A\u{2028}B\u{2029}C");
        assert_eq!(text.normalized_text(), "A\nB\nC");
        assert_eq!(text.glyph_text(), "ABC");
    }

    #[test]
    fn r3_zero_width_space_becomes_cluster_without_glyphs() {
        let text = ClusteredText::new("A\u{200B}B");
        assert_eq!(text.normalized_text(), "A\u{200B}B");
        assert_eq!(text.glyph_text(), "AB");
        assert_eq!(text.len(), 3);
        assert_eq!(
            text.cluster(1).class(),
            BreakClass::Pair(break_class::PairClass::Sp)
        );
        assert!(text.cluster(1).glyph_range().is_empty());
    }

    #[test]
    fn r3_variation_selectors_handled() {
        // Isolated VS at start stripped
        let text_start = ClusteredText::new("\u{FE00}A");
        assert_eq!(text_start.normalized_text(), "A");

        // VS following base character preserved
        let text_base = ClusteredText::new("A\u{FE00}B");
        assert_eq!(text_base.normalized_text(), "A\u{FE00}B");
        assert_eq!(text_base.glyph_text(), "A\u{FE00}B");

        // Isolated VS after space stripped
        let text_space = ClusteredText::new(" \u{FE00}A");
        assert_eq!(text_space.normalized_text(), " A");

        // Isolated VS after ZWSP stripped
        let text_zwsp = ClusteredText::new("A\u{200B}\u{FE00}B");
        assert_eq!(text_zwsp.normalized_text(), "A\u{200B}B");

        // Isolated VS at span start stripped
        let tagged = cluster_tagged([(0, "A"), (1, "\u{FE00}B")]);
        assert_eq!(tagged.clustered().normalized_text(), "AB");
    }
}
