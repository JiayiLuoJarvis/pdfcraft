//! Extended grapheme clusters (UAX #29) as line-break atoms.

#![cfg_attr(not(test), allow(dead_code))]

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
        let (normalized, _) = normalize_tagged(source.chars().map(|scalar| (scalar, ())));
        Self::from_normalized(normalized)
    }

    fn from_normalized(normalized: String) -> Self {
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
    let tagged_scalars = runs.into_iter().flat_map(|(tag, text)| {
        text.as_ref()
            .chars()
            .map(move |scalar| (scalar, tag))
            .collect::<Vec<_>>()
    });
    let (normalized, normalized_tags) = normalize_tagged(tagged_scalars);
    let clustered = ClusteredText::from_normalized(normalized);
    let mut glyph_tags = Vec::with_capacity(clustered.glyph_text().chars().count());
    let mut tag_cursor = 0usize;
    for index in 0..clustered.len() {
        let cluster = clustered.cluster(index);
        let count = cluster.scalar_count();
        if !cluster.glyph_range().is_empty() {
            glyph_tags.extend_from_slice(&normalized_tags[tag_cursor..tag_cursor + count]);
        }
        tag_cursor += count;
    }
    debug_assert_eq!(tag_cursor, normalized_tags.len());
    debug_assert_eq!(glyph_tags.len(), clustered.glyph_text().chars().count());
    TaggedClusteredText {
        clustered,
        glyph_tags: glyph_tags.into_boxed_slice(),
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

fn normalize_tagged<T: Copy>(
    tagged_scalars: impl IntoIterator<Item = (char, T)>,
) -> (String, Vec<T>) {
    let mut text = String::new();
    let mut tags = Vec::new();
    let mut scalars = tagged_scalars.into_iter().peekable();
    while let Some((scalar, tag)) = scalars.next() {
        match scalar {
            '\r' => {
                if matches!(scalars.peek(), Some(('\n', _))) {
                    scalars.next();
                }
                text.push('\n');
                tags.push(tag);
            }
            other => {
                text.push(other);
                tags.push(tag);
            }
        }
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
}
