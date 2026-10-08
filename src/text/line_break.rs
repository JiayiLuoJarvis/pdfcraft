//! Pure line-breaking over clustered text.

#![allow(dead_code)]

use std::ops::Range;

use crate::error::{Error, Result};
use crate::text::break_class::{allows, BreakClass, PairClass};
use crate::text::cluster::{Cluster, ClusterCursor, ClusteredText};

#[derive(Debug, Clone, Copy)]
enum Atom {
    Hard,
    Ink {
        pair: PairClass,
        width: f32,
        cluster: usize,
    },
}

/// Lines produced by [`break_lines`].
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct BrokenLines<'text> {
    text: &'text ClusteredText,
    records: Box<[LineRecord]>,
}

#[derive(Debug, Clone, PartialEq)]
struct LineRecord {
    start: ClusterCursor,
    end: ClusterCursor,
    width: f32,
}

/// One broken line over `glyph_text` coordinates.
#[derive(Debug, Clone, Copy)]
pub(crate) struct BrokenLine<'a> {
    text: &'a ClusteredText,
    record: &'a LineRecord,
}

/// Break `text` to fit `avail` using `width_of` for each non-BK cluster.
pub(crate) fn break_lines<'text>(
    text: &'text ClusteredText,
    avail: f32,
    mut width_of: impl FnMut(Cluster<'text>) -> f32,
) -> Result<BrokenLines<'text>> {
    if !avail.is_finite() || avail <= 0.0 {
        return Err(Error::InvalidGeometry);
    }

    if text.is_empty() {
        let origin = ClusterCursor {
            cluster: 0,
            glyph: 0,
            glyph_byte: 0,
        };
        return Ok(BrokenLines {
            text,
            records: Box::new([LineRecord {
                start: origin,
                end: origin,
                width: 0.0,
            }]),
        });
    }

    let atoms = project_atoms(text, &mut width_of)?;
    let records = pack_lines(text, &atoms, avail)?;
    Ok(BrokenLines {
        text,
        records: records.into_boxed_slice(),
    })
}

fn project_atoms<'text>(
    text: &'text ClusteredText,
    width_of: &mut impl FnMut(Cluster<'text>) -> f32,
) -> Result<Vec<Atom>> {
    let mut atoms = Vec::with_capacity(text.len());
    for index in 0..text.len() {
        let cluster = text.cluster(index);
        match cluster.class() {
            BreakClass::Hard => atoms.push(Atom::Hard),
            BreakClass::Pair(pair) => {
                let width = width_of(cluster);
                if !width.is_finite() || width < 0.0 {
                    return Err(Error::InvalidGeometry);
                }
                atoms.push(Atom::Ink {
                    pair,
                    width,
                    cluster: index,
                });
            }
        }
    }
    Ok(atoms)
}

fn pack_lines(text: &ClusteredText, atoms: &[Atom], avail: f32) -> Result<Vec<LineRecord>> {
    let mut records = Vec::new();
    let mut line_start = 0usize;
    let mut i = 0usize;
    let mut line_width = 0.0f32;

    while i < atoms.len() {
        if line_start == i {
            if let Atom::Ink {
                pair: PairClass::Sp,
                ..
            } = atoms[i]
            {
                i += 1;
                line_start = i;
                continue;
            }
        }

        match atoms[i] {
            Atom::Hard => {
                records.push(emit_line(text, atoms, line_start, i, line_width));
                i += 1;
                line_start = i;
                line_width = 0.0;
            }
            Atom::Ink { width, .. } => {
                if line_width + width <= avail {
                    line_width += width;
                    i += 1;
                    continue;
                }
                match nearest_break(atoms, line_start, i) {
                    Some(split) => {
                        let committed: f32 = (line_start..split)
                            .filter_map(|idx| ink_width(&atoms[idx]))
                            .sum();
                        records.push(emit_line(text, atoms, line_start, split, committed));
                        line_start = split;
                        i = split;
                        line_width = 0.0;
                    }
                    None => {
                        return Err(Error::UnbreakableWiderThanFrame {
                            width: line_width + width,
                            avail,
                        });
                    }
                }
            }
        }
    }

    if line_start < atoms.len() {
        records.push(emit_line(text, atoms, line_start, atoms.len(), line_width));
    } else {
        // Empty current line at EOF: trailing BK, or overflow SP discarded.
        records.push(emit_line(text, atoms, line_start, line_start, 0.0));
    }
    Ok(records)
}

fn nearest_break(atoms: &[Atom], line_start: usize, overflow_at: usize) -> Option<usize> {
    for k in (line_start + 1..=overflow_at).rev() {
        let Some(prev) = ink_pair(&atoms[k - 1]) else {
            continue;
        };
        let Some(next) = ink_pair(&atoms[k]) else {
            continue;
        };
        if allows(prev, next) {
            return Some(k);
        }
    }
    None
}

fn ink_pair(atom: &Atom) -> Option<PairClass> {
    match *atom {
        Atom::Ink { pair, .. } => Some(pair),
        Atom::Hard => None,
    }
}

fn ink_width(atom: &Atom) -> Option<f32> {
    match *atom {
        Atom::Ink { width, .. } => Some(width),
        Atom::Hard => None,
    }
}

fn emit_line(
    text: &ClusteredText,
    atoms: &[Atom],
    start: usize,
    end: usize,
    width: f32,
) -> LineRecord {
    if start >= end {
        let cursor = eof_cursor(text, atoms, start);
        return LineRecord {
            start: cursor,
            end: cursor,
            width: 0.0,
        };
    }

    let first_cluster = match atoms[start] {
        Atom::Ink { cluster, .. } => cluster,
        Atom::Hard => unreachable!("hard breaks are not emitted inside a line"),
    };
    let last_cluster = match atoms[end - 1] {
        Atom::Ink { cluster, .. } => cluster,
        Atom::Hard => unreachable!("hard breaks are not emitted inside a line"),
    };
    LineRecord {
        start: text.cluster(first_cluster).start_cursor(),
        end: text.cluster(last_cluster).end_cursor(),
        width,
    }
}

fn eof_cursor(text: &ClusteredText, atoms: &[Atom], atom_index: usize) -> ClusterCursor {
    if atom_index == 0 {
        return ClusterCursor {
            cluster: 0,
            glyph: 0,
            glyph_byte: 0,
        };
    }
    match atoms[atom_index - 1] {
        Atom::Hard => text.cluster(atom_index - 1).end_cursor(),
        Atom::Ink { cluster, .. } => text.cluster(cluster).end_cursor(),
    }
}

impl BrokenLines<'_> {
    pub(crate) fn len(&self) -> usize {
        self.records.len()
    }

    pub(crate) fn get(&self, index: usize) -> BrokenLine<'_> {
        BrokenLine {
            text: self.text,
            record: &self.records[index],
        }
    }

    pub(crate) fn iter(&self) -> impl ExactSizeIterator<Item = BrokenLine<'_>> + '_ {
        self.records.iter().map(|record| BrokenLine {
            text: self.text,
            record,
        })
    }
}

impl BrokenLine<'_> {
    pub(crate) fn text(&self) -> &str {
        let start = self.record.start.glyph_byte;
        let end = self.record.end.glyph_byte;
        &self.text.glyph_text()[start..end]
    }

    pub(crate) fn glyph_range(&self) -> Range<usize> {
        self.record.start.glyph..self.record.end.glyph
    }

    pub(crate) fn glyph_byte_range(&self) -> Range<usize> {
        self.record.start.glyph_byte..self.record.end.glyph_byte
    }

    pub(crate) fn width(&self) -> f32 {
        self.record.width
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn break_owned(source: &str, avail: f32) -> Result<Vec<(String, f32, Range<usize>)>> {
        let text = ClusteredText::new(source);
        let broken = break_lines(&text, avail, |cluster| {
            f32::from(u16::try_from(cluster.scalar_count()).expect("stub cluster scalar count"))
        })?;
        Ok(broken
            .iter()
            .map(|line| (line.text().to_owned(), line.width(), line.glyph_range()))
            .collect())
    }

    fn assert_width(got: f32, want: f32) {
        assert_eq!(got.to_bits(), want.to_bits());
    }

    #[test]
    fn ascii_breaks_at_spaces() {
        let lines = break_owned("hello world", 5.0).unwrap();
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].0, "hello");
        assert_eq!(lines[1].0, "world");
    }

    #[test]
    fn wide_al_errors() {
        let err = break_owned("hello", 4.0).unwrap_err();
        assert_eq!(
            err,
            Error::UnbreakableWiderThanFrame {
                width: 5.0,
                avail: 4.0
            }
        );
    }

    #[test]
    fn id_breaks_between() {
        let lines = break_owned("你好", 1.0).unwrap();
        assert_eq!(lines[0].0, "你");
        assert_eq!(lines[1].0, "好");
    }

    #[test]
    fn break_before_opening_paren() {
        let lines = break_owned("好（世", 2.0).unwrap();
        assert_eq!(lines[0].0, "好");
        assert_eq!(lines[1].0, "（世");
    }

    #[test]
    fn no_break_after_opening_paren() {
        let lines = break_owned("（世界", 2.0).unwrap();
        assert_eq!(lines[0].0, "（世");
        assert_eq!(lines[1].0, "界");
    }

    #[test]
    fn no_break_before_period() {
        let err = break_owned("字。", 1.0).unwrap_err();
        assert_eq!(
            err,
            Error::UnbreakableWiderThanFrame {
                width: 2.0,
                avail: 1.0
            }
        );
    }

    #[test]
    fn break_after_period() {
        let lines = break_owned("字。下", 2.0).unwrap();
        assert_eq!(lines[0].0, "字。");
        assert_eq!(lines[1].0, "下");
    }

    #[test]
    fn crlf_is_one_hard_break() {
        let lines = break_owned("A\r\nB", 10.0).unwrap();
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].0, "A");
        assert_eq!(lines[1].0, "B");
    }

    #[test]
    fn leading_spaces_dropped() {
        let lines = break_owned("  A", 10.0).unwrap();
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].0, "A");
        assert_eq!(lines[0].2, 2..3);
    }

    #[test]
    fn overflow_space_dropped_word_kept() {
        let lines = break_owned("hello ", 5.0).unwrap();
        assert_eq!(lines[0].0, "hello");
        assert_eq!(lines[1].0, "");
        assert_width(lines[1].1, 0.0);
    }

    #[test]
    fn ideographic_space_kept() {
        let lines = break_owned("A\u{3000}B", 1.0).unwrap();
        assert_eq!(lines.len(), 3);
        assert_eq!(lines[1].0, "\u{3000}");
    }

    #[test]
    fn combining_cluster_unbreakable() {
        let err = break_owned("e\u{0301}", 1.0).unwrap_err();
        assert_eq!(
            err,
            Error::UnbreakableWiderThanFrame {
                width: 2.0,
                avail: 1.0
            }
        );
    }

    #[test]
    fn other_run_unbreakable() {
        let err = break_owned("\u{FF21}\u{FF22}", 1.0).unwrap_err();
        assert_eq!(
            err,
            Error::UnbreakableWiderThanFrame {
                width: 2.0,
                avail: 1.0
            }
        );
    }

    #[test]
    fn id_other_can_break() {
        let lines = break_owned("字\u{FF21}", 1.0).unwrap();
        assert_eq!(lines[0].0, "字");
        assert_eq!(lines[1].0, "\u{FF21}");
    }

    #[test]
    fn empty_paragraph_one_empty_line() {
        let lines = break_owned("", 1.0).unwrap();
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].0, "");
        assert_width(lines[0].1, 0.0);
    }

    #[test]
    fn invalid_avail() {
        assert!(matches!(break_owned("A", 0.0), Err(Error::InvalidGeometry)));
        assert!(matches!(
            break_owned("A", f32::NAN),
            Err(Error::InvalidGeometry)
        ));
    }

    #[test]
    fn consecutive_hard_breaks_yield_empty_line() {
        let lines = break_owned("A\n\nB", 10.0).unwrap();
        assert_eq!(lines.len(), 3);
        assert_eq!(lines[0].0, "A");
        assert_eq!(lines[1].0, "");
        assert_eq!(lines[2].0, "B");
    }

    #[test]
    fn plain_word_no_extra_empty_line() {
        let lines = break_owned("A", 10.0).unwrap();
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].0, "A");
    }
}
