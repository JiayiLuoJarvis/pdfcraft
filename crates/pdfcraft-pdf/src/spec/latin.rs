//! Caller-side split of a string into Latin-class and other [`InlineRun`]s.

use std::ops::RangeInclusive;

use super::{InlineRun, RunStyle};

/// Blocks where an alphabetic scalar is Latin class.
/// ASCII U+0020..=U+007E is Latin regardless of `is_alphabetic`.
const LATIN_LETTER_BLOCKS: [RangeInclusive<char>; 7] = [
    '\u{00C0}'..='\u{00FF}',
    '\u{0100}'..='\u{017F}',
    '\u{0180}'..='\u{024F}',
    '\u{1E00}'..='\u{1EFF}',
    '\u{2C60}'..='\u{2C7F}',
    '\u{A720}'..='\u{A7FF}',
    '\u{AB30}'..='\u{AB6F}',
];

fn is_latin(first: char) -> bool {
    if ('\u{0020}'..='\u{007E}').contains(&first) {
        return true;
    }
    first.is_alphabetic()
        && LATIN_LETTER_BLOCKS
            .iter()
            .any(|block| block.contains(&first))
}

impl<K: Clone + PartialEq> InlineRun<K> {
    /// Splits `text` into [`InlineRun::Text`] runs.
    ///
    /// Latin-class grapheme clusters use `latin`. Every other cluster uses `cjk`.
    /// A cluster's class is its first scalar, and the rest of the cluster follows it.
    /// Consecutive clusters that share a key share one run, so equal keys yield one run.
    /// Every run has [`RunStyle::default`]. An empty `text` yields an empty [`Vec`].
    ///
    /// No font is read. A missing glyph still fails later in [`crate::layout_spec`]
    /// under that run's key. A key that is not in the registry does not fail here.
    ///
    /// Each run stores a clone of its key. The keys are taken by value so a caller can pass temporaries.
    #[allow(clippy::needless_pass_by_value)]
    #[must_use]
    pub fn split_latin(text: impl AsRef<str>, cjk: K, latin: K) -> Vec<Self> {
        use unicode_segmentation::UnicodeSegmentation;

        let text = text.as_ref();
        let mut runs = Vec::new();
        for (_, cluster) in text.grapheme_indices(true) {
            let key = if cluster.chars().next().is_some_and(is_latin) {
                &latin
            } else {
                &cjk
            };
            if let Some(InlineRun::Text {
                text: last, font, ..
            }) = runs.last_mut()
            {
                if font == key {
                    last.push_str(cluster);
                    continue;
                }
            }
            runs.push(InlineRun::Text {
                text: cluster.to_owned(),
                font: key.clone(),
                style: RunStyle::default(),
            });
        }
        runs
    }
}

#[cfg(test)]
mod tests {
    use super::super::{InlineRun, RunStyle};

    #[derive(Clone, PartialEq, Debug)]
    struct Unregistered;

    fn texts<K: Clone + PartialEq + std::fmt::Debug>(runs: Vec<InlineRun<K>>) -> Vec<(String, K)> {
        runs.into_iter()
            .map(|run| match run {
                InlineRun::Text { text, font, style } => {
                    assert_eq!(style, RunStyle::default());
                    (text, font)
                }
                InlineRun::Link { .. } | InlineRun::Blank(_) => {
                    panic!("split_latin returns only text runs")
                }
            })
            .collect()
    }

    #[test]
    fn empty_text_is_an_empty_vec() {
        assert!(InlineRun::<u8>::split_latin("", 0, 1).is_empty());
    }

    #[test]
    fn equal_keys_stay_one_run() {
        assert_eq!(
            texts(InlineRun::split_latin("A中B", 7_u8, 7)),
            vec![("A中B".to_owned(), 7)]
        );
    }

    #[test]
    fn amount_line_splits_after_the_cjk_prefix() {
        assert_eq!(
            texts(InlineRun::split_latin("合同金额 USD 100", "cjk", "latin")),
            vec![
                ("合同金额".to_owned(), "cjk"),
                (" USD 100".to_owned(), "latin"),
            ]
        );
    }

    #[test]
    fn line_feed_is_its_own_cjk_run() {
        assert_eq!(
            texts(InlineRun::split_latin("A\nB", 0_u8, 1)),
            vec![
                ("A".to_owned(), 1),
                ("\n".to_owned(), 0),
                ("B".to_owned(), 1)
            ]
        );
    }

    #[test]
    fn combining_acute_stays_with_its_base() {
        let text = "e\u{0301}";
        assert_eq!(
            texts(InlineRun::split_latin(text, 0_u8, 1)),
            vec![(text.to_owned(), 1)]
        );
    }

    #[test]
    fn curly_quotes_stay_cjk_around_ascii() {
        assert_eq!(
            texts(InlineRun::split_latin("\u{201C}ABC\u{201D}", 0_u8, 1)),
            vec![
                ("\u{201C}".to_owned(), 0),
                ("ABC".to_owned(), 1),
                ("\u{201D}".to_owned(), 0),
            ]
        );
    }

    #[test]
    fn crlf_is_preserved_inside_the_cjk_run() {
        let runs = texts(InlineRun::split_latin("A\r\nB", 0_u8, 1));
        assert_eq!(
            runs,
            vec![
                ("A".to_owned(), 1),
                ("\r\n".to_owned(), 0),
                ("B".to_owned(), 1),
            ]
        );
        assert_eq!(runs[1].0.as_bytes(), b"\r\n");
    }

    #[test]
    fn an_unregistered_key_does_not_fail_the_split() {
        let runs = InlineRun::split_latin("A", Unregistered, Unregistered);
        assert_eq!(runs.len(), 1);
        assert!(matches!(
            &runs[0],
            InlineRun::Text { text, style, .. } if text == "A" && *style == RunStyle::default()
        ));
    }

    #[test]
    fn latin_1_operators_and_the_ipa_boundary_follow_the_table() {
        assert_eq!(
            texts(InlineRun::split_latin("\u{00D7}\u{00F7}", 0_u8, 1)),
            vec![("\u{00D7}\u{00F7}".to_owned(), 0)]
        );
        assert_eq!(
            texts(InlineRun::split_latin("\u{024F}\u{0250}", 0_u8, 1)),
            vec![("\u{024F}".to_owned(), 1), ("\u{0250}".to_owned(), 0)]
        );
    }
}
