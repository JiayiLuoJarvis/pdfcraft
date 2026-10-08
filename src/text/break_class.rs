//! Break-class table for CJK-aware line breaking (not full UAX #14).

#![allow(dead_code)]

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
    '\u{3014}', '\u{FF5B}',
];

const CL: &[char] = &[
    ')', ']', '}', '\u{FF09}', '\u{FF0C}', '\u{3002}', '\u{3001}', '\u{FF1B}', '\u{FF1A}',
    '\u{FF01}', '\u{FF1F}', '\u{3011}', '\u{300B}', '\u{300D}', '\u{300F}', '\u{3009}', '\u{3015}',
    '\u{FF5D}', '\u{FF0E}',
];

/// Classify a cluster by its first scalar.
pub(crate) fn classify(scalar: char) -> BreakClass {
    if scalar == '\n' {
        return BreakClass::Hard;
    }
    if scalar == ' ' {
        return BreakClass::Pair(PairClass::Sp);
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
    ALLOW[prev as usize][next as usize]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_design_edges() {
        assert_eq!(classify('\n'), BreakClass::Hard);
        assert_eq!(classify(' '), BreakClass::Pair(PairClass::Sp));
        assert_eq!(classify('\u{3000}'), BreakClass::Pair(PairClass::Id));
        assert_eq!(classify('A'), BreakClass::Pair(PairClass::Al));
        assert_eq!(classify('好'), BreakClass::Pair(PairClass::Id));
        assert_eq!(classify('（'), BreakClass::Pair(PairClass::Op));
        assert_eq!(classify('。'), BreakClass::Pair(PairClass::Cl));
        assert_eq!(classify('\u{00D7}'), BreakClass::Pair(PairClass::Other));
        assert_eq!(classify('\u{00F7}'), BreakClass::Pair(PairClass::Other));
        assert_eq!(classify('\u{F900}'), BreakClass::Pair(PairClass::Other));
        assert_eq!(classify('\u{FF21}'), BreakClass::Pair(PairClass::Other));
        assert_eq!(classify('\u{20000}'), BreakClass::Pair(PairClass::Other));
        assert_eq!(classify('\r'), BreakClass::Pair(PairClass::Other));
        assert_eq!(classify('\t'), BreakClass::Pair(PairClass::Other));
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
}
