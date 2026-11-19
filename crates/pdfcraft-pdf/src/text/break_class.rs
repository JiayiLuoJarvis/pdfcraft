//! Break-class table for CJK-aware line breaking (not full UAX #14).

/// Cluster-head class. `Hard` is BK and never enters the pair matrix.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum BreakClass {
    /// Forced break (`\n` after CRLF normalization).
    Hard,
    /// Participates in the 6×6 pair table.
    Pair(PairClass),
}

/// Row/column of the pair table. Discriminant equals the matrix index.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub(crate) enum PairClass {
    Id = 0,
    Al = 1,
    Other = 2,
    Sp = 3,
    Op = 4,
    Cl = 5,
}

/// `true` means a break is allowed before `next`.
const ALLOW: [[bool; 6]; 6] = [
    // ID
    [true, true, true, true, true, false],
    // AL
    [true, false, false, true, true, false],
    // Other
    [true, false, false, true, true, false],
    // SP
    [true, true, true, true, true, false],
    // OP
    [false, false, false, false, false, false],
    // CL
    [true, true, true, true, true, false],
];

const OP: &[char] = &[
    '(', '[', '{', '\u{FF08}', '\u{3010}', '\u{300A}', '\u{300C}', '\u{300E}', '\u{3008}',
    '\u{3014}', '\u{FF5B}', '\u{201C}', '\u{2018}',
];

const CL: &[char] = &[
    ')', ']', '}', '\u{FF09}', '\u{FF0C}', '\u{3002}', '\u{3001}', '\u{FF1B}', '\u{FF1A}',
    '\u{FF01}', '\u{FF1F}', '\u{3011}', '\u{300B}', '\u{300D}', '\u{300F}', '\u{3009}', '\u{3015}',
    '\u{FF5D}', '\u{FF0E}', '\u{201D}', '\u{00B7}', '\u{30FB}', '\u{FF5E}',
];

/// Classify a cluster by its first scalar and the previous cluster's pair class.
pub(crate) fn classify(scalar: char, prev_class: Option<PairClass>) -> BreakClass {
    if scalar == '\n' {
        return BreakClass::Hard;
    }
    if scalar == ' ' {
        return BreakClass::Pair(PairClass::Sp);
    }
    if scalar == '\u{2019}' {
        let pair = if prev_class == Some(PairClass::Al) {
            PairClass::Other
        } else {
            PairClass::Cl
        };
        return BreakClass::Pair(pair);
    }
    if prev_class == Some(PairClass::Id)
        && matches!(scalar, ',' | '.' | ';' | ':' | '!' | '?' | '%')
    {
        return BreakClass::Pair(PairClass::Cl);
    }
    if OP.contains(&scalar) {
        return BreakClass::Pair(PairClass::Op);
    }
    if CL.contains(&scalar) {
        return BreakClass::Pair(PairClass::Cl);
    }
    let cp = u32::from(scalar);
    if is_id(cp) {
        return BreakClass::Pair(PairClass::Id);
    }
    if is_al(cp) {
        return BreakClass::Pair(PairClass::Al);
    }
    BreakClass::Pair(PairClass::Other)
}

fn is_id(cp: u32) -> bool {
    matches!(
        cp,
        0x3000
            | 0x3400..=0x4DBF
            | 0x4E00..=0x9FFF
            | 0x3040..=0x309F
            | 0x30A0..=0x30FF
            | 0xFF66..=0xFF9D
            | 0xAC00..=0xD7AF
    )
}

/// Fullwidth opening/closing punctuation: in OP or CL and codepoint ≥ U+3000.
/// ASCII `()[]{}` are in the tables but excluded by the floor.
pub(crate) fn is_fullwidth_op_or_cl(scalar: char) -> bool {
    u32::from(scalar) >= 0x3000 && (OP.contains(&scalar) || CL.contains(&scalar))
}

/// Stretchable cluster head for Justify phase 2 (Word-style inter-ideograph).
/// Han Ext-A / Unified only, or fullwidth Op/Cl. Not [`is_id`] (kana/Hangul/U+3000).
pub(crate) fn is_justify_cluster(scalar: char) -> bool {
    let cp = u32::from(scalar);
    matches!(cp, 0x3400..=0x4DBF | 0x4E00..=0x9FFF) || is_fullwidth_op_or_cl(scalar)
}

fn is_al(cp: u32) -> bool {
    matches!(
        cp,
        0x0030..=0x0039
            | 0x0041..=0x005A
            | 0x0061..=0x007A
            | 0x00C0..=0x00D6
            | 0x00D8..=0x00F6
            | 0x00F8..=0x00FF
    )
}

/// Pair-table lookup. `Hard` cannot be passed.
pub(crate) fn allows(prev: PairClass, next: PairClass) -> bool {
    ALLOW
        .get(prev as usize)
        .and_then(|row| row.get(next as usize))
        .copied()
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_design_edges() {
        assert_eq!(classify('\n', None), BreakClass::Hard);
        assert_eq!(classify(' ', None), BreakClass::Pair(PairClass::Sp));
        assert_eq!(classify('\u{3000}', None), BreakClass::Pair(PairClass::Id));
        assert_eq!(classify('A', None), BreakClass::Pair(PairClass::Al));
        assert_eq!(classify('好', None), BreakClass::Pair(PairClass::Id));
        assert_eq!(classify('（', None), BreakClass::Pair(PairClass::Op));
        assert_eq!(classify('。', None), BreakClass::Pair(PairClass::Cl));
        assert_eq!(
            classify('\u{00D7}', None),
            BreakClass::Pair(PairClass::Other)
        );
        assert_eq!(
            classify('\u{00F7}', None),
            BreakClass::Pair(PairClass::Other)
        );
        assert_eq!(
            classify('\u{F900}', None),
            BreakClass::Pair(PairClass::Other)
        );
        assert_eq!(
            classify('\u{FF21}', None),
            BreakClass::Pair(PairClass::Other)
        );
        assert_eq!(
            classify('\u{20000}', None),
            BreakClass::Pair(PairClass::Other)
        );
        assert_eq!(classify('\r', None), BreakClass::Pair(PairClass::Other));
        assert_eq!(classify('\t', None), BreakClass::Pair(PairClass::Other));
    }

    #[test]
    fn curved_quotes_classification() {
        assert_eq!(classify('“', None), BreakClass::Pair(PairClass::Op));
        assert_eq!(classify('‘', None), BreakClass::Pair(PairClass::Op));
        assert_eq!(classify('”', None), BreakClass::Pair(PairClass::Cl));
        assert_eq!(
            classify('’', Some(PairClass::Al)),
            BreakClass::Pair(PairClass::Other)
        );
        assert_eq!(
            classify('’', Some(PairClass::Id)),
            BreakClass::Pair(PairClass::Cl)
        );
        assert_eq!(classify('’', None), BreakClass::Pair(PairClass::Cl));
    }

    #[test]
    fn pair_table_matches_design() {
        assert!(allows(PairClass::Id, PairClass::Op));
        assert!(!allows(PairClass::Op, PairClass::Id));
        assert!(!allows(PairClass::Id, PairClass::Cl));
        assert!(allows(PairClass::Cl, PairClass::Id));
        assert!(!allows(PairClass::Al, PairClass::Al));
        assert!(allows(PairClass::Al, PairClass::Sp));
        assert!(!allows(PairClass::Other, PairClass::Other));
        assert!(allows(PairClass::Id, PairClass::Other));
        assert!(!allows(PairClass::Op, PairClass::Cl));
    }

    #[test]
    fn justify_cluster_is_han_or_fullwidth_op_cl_only() {
        assert!(is_justify_cluster('好'));
        assert!(is_justify_cluster('\u{3400}'));
        assert!(is_justify_cluster('\u{4E00}'));
        assert!(is_justify_cluster('\u{9FFF}'));
        assert!(is_justify_cluster('（'));
        assert!(is_justify_cluster('。'));
        assert!(is_justify_cluster('，'));
        assert!(!is_justify_cluster('A'));
        assert!(!is_justify_cluster(' '));
        assert!(!is_justify_cluster('('));
        assert!(!is_justify_cluster(')'));
        assert!(!is_justify_cluster('\u{3000}'));
        assert!(!is_justify_cluster('あ'));
        assert!(!is_justify_cluster('ア'));
        assert!(!is_justify_cluster('한'));
        assert!(!is_justify_cluster('\u{FF21}'));
        assert!(!is_justify_cluster('\u{20000}'));
        assert!(!is_justify_cluster('\u{F900}'));
        assert!(is_id(u32::from('あ')));
        assert!(!is_justify_cluster('あ'));
    }

    #[test]
    fn r2_chinese_break_rules_middle_dots_and_tilde() {
        assert_eq!(classify('·', None), BreakClass::Pair(PairClass::Cl));
        assert_eq!(classify('・', None), BreakClass::Pair(PairClass::Cl));
        assert_eq!(classify('～', None), BreakClass::Pair(PairClass::Cl));
        assert_eq!(
            classify('·', Some(PairClass::Id)),
            BreakClass::Pair(PairClass::Cl)
        );
        assert_eq!(
            classify('·', Some(PairClass::Al)),
            BreakClass::Pair(PairClass::Cl)
        );
    }

    #[test]
    fn r2_halfwidth_punctuation_after_id() {
        let halfwidth = [',', '.', ';', ':', '!', '?', '%'];
        for p in halfwidth {
            assert_eq!(
                classify(p, Some(PairClass::Id)),
                BreakClass::Pair(PairClass::Cl)
            );
            assert_eq!(
                classify(p, Some(PairClass::Al)),
                BreakClass::Pair(PairClass::Other)
            );
            assert_eq!(classify(p, None), BreakClass::Pair(PairClass::Other));
        }
    }

    #[test]
    fn r2_closing_brackets_remain_cl() {
        for b in [')', ']', '}'] {
            assert_eq!(
                classify(b, Some(PairClass::Id)),
                BreakClass::Pair(PairClass::Cl)
            );
            assert_eq!(
                classify(b, Some(PairClass::Al)),
                BreakClass::Pair(PairClass::Cl)
            );
        }
    }
}
