//! Pure line-breaking over clustered text.

#![cfg_attr(not(test), allow(dead_code))]

use std::ops::Range;

use crate::error::{Error, ErrorKind, Fallible as Result, GeometryKind};
use crate::text::break_class::{allows, BreakClass, PairClass};
use crate::text::cluster::{Cluster, ClusterCursor, ClusteredText};

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum Advance {
    Fixed(f32),
    Fill,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum Span<T> {
    Group(usize),
    Blank { advance: Advance, payload: T },
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum Seg<T> {
    Glyphs {
        group: usize,
        start: ClusterCursor,
        end: ClusterCursor,
    },
    Blank {
        width: f32,
        payload: T,
    },
}

#[derive(Debug, Clone, Copy)]
enum Atom<T> {
    Hard {
        group: usize,
        cluster: usize,
    },
    Ink {
        pair: PairClass,
        width: f32,
        group: usize,
        cluster: usize,
    },
    Blank {
        advance: Advance,
        width: f32,
        payload: T,
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

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct BrokenSpans<T> {
    lines: Box<[SpanLine<T>]>,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct SpanLine<T> {
    segs: Vec<Seg<T>>,
    width: f32,
    empty_at: Option<(usize, ClusterCursor)>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct LineBudget {
    pub(crate) first: f32,
    pub(crate) rest: f32,
}

impl LineBudget {
    pub(crate) const fn uniform(width: f32) -> Self {
        Self {
            first: width,
            rest: width,
        }
    }

    fn for_line(self, index: usize) -> f32 {
        if index == 0 {
            self.first
        } else {
            self.rest
        }
    }

    fn validate(self) -> Result<()> {
        for width in [self.first, self.rest] {
            if !width.is_finite() || width <= 0.0 {
                return Err(GeometryKind::LineWidth { width }.into());
            }
        }
        Ok(())
    }
}

/// Break `text` to fit `budget` using `width_of` for each non-BK cluster.
pub(crate) fn break_lines(
    text: &ClusteredText,
    budget: LineBudget,
    mut width_of: impl FnMut(Cluster<'_>) -> f32,
) -> Result<BrokenLines<'_>> {
    budget.validate()?;

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

    let spans = [Span::<()>::Group(0)];
    let broken = break_spans(std::slice::from_ref(&text), &spans, budget, |_, cluster| {
        width_of(cluster)
    })?;
    let records = broken
        .lines
        .iter()
        .map(facade_record)
        .collect::<Vec<_>>()
        .into_boxed_slice();
    Ok(BrokenLines { text, records })
}

pub(crate) fn break_spans<T: Copy>(
    groups: &[&ClusteredText],
    spans: &[Span<T>],
    budget: LineBudget,
    mut width_of: impl FnMut(usize, Cluster<'_>) -> f32,
) -> Result<BrokenSpans<T>> {
    budget.validate()?;
    if let Some(pos) = spans.iter().position(|span| {
        matches!(
            span,
            Span::Blank {
                advance: Advance::Fill,
                ..
            }
        )
    }) {
        if pos + 1 != spans.len() {
            return Err(Error::from(ErrorKind::RunAfterFillBlank));
        }
    }

    let atoms = project_atoms(groups, spans, &mut width_of)?;
    let lines = pack_lines(groups, atoms, budget)?;
    Ok(BrokenSpans {
        lines: lines.into_boxed_slice(),
    })
}

fn project_atoms<T: Copy>(
    groups: &[&ClusteredText],
    spans: &[Span<T>],
    width_of: &mut impl FnMut(usize, Cluster<'_>) -> f32,
) -> Result<Vec<Atom<T>>> {
    let mut atoms = Vec::new();
    for span in spans {
        match *span {
            Span::Group(group) => {
                let text = groups[group];
                for index in 0..text.len() {
                    let cluster = text.cluster(index);
                    match cluster.class() {
                        BreakClass::Hard => atoms.push(Atom::Hard {
                            group,
                            cluster: index,
                        }),
                        BreakClass::Pair(pair) => {
                            let width = width_of(group, cluster);
                            if !width.is_finite() || width < 0.0 {
                                return Err(GeometryKind::AdvanceWidth { width }.into());
                            }
                            atoms.push(Atom::Ink {
                                pair,
                                width,
                                group,
                                cluster: index,
                            });
                        }
                    }
                }
            }
            Span::Blank { advance, payload } => {
                let width = match advance {
                    Advance::Fixed(width) => {
                        if !width.is_finite() || width < 0.0 {
                            return Err(GeometryKind::AdvanceWidth { width }.into());
                        }
                        width
                    }
                    Advance::Fill => 0.0,
                };
                atoms.push(Atom::Blank {
                    advance,
                    width,
                    payload,
                });
            }
        }
    }
    Ok(atoms)
}

fn pack_lines<T: Copy>(
    groups: &[&ClusteredText],
    mut atoms: Vec<Atom<T>>,
    budget: LineBudget,
) -> Result<Vec<SpanLine<T>>> {
    let mut records = Vec::new();
    let mut line_start = 0usize;
    let mut i = 0usize;
    let mut line_width = 0.0f32;

    while i < atoms.len() {
        let avail = budget.for_line(records.len());
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
            Atom::Hard { .. } => {
                records.push(emit_line(groups, &atoms, line_start, i, line_width));
                i += 1;
                line_start = i;
                line_width = 0.0;
            }
            Atom::Blank {
                advance: Advance::Fill,
                ..
            } => {
                debug_assert!(line_width <= avail);
                let remain = avail - line_width;
                let width = if remain.is_finite() && remain > 0.0 {
                    remain
                } else {
                    0.0
                };
                if let Atom::Blank { width: slot, .. } = &mut atoms[i] {
                    *slot = width;
                }
                line_width = avail;
                i += 1;
            }
            Atom::Ink { width, .. } | Atom::Blank { width, .. } => {
                if line_width + width <= avail {
                    line_width += width;
                    i += 1;
                    continue;
                }
                match nearest_break(&atoms, line_start, i) {
                    Some(split) => {
                        let committed: f32 = (line_start..split)
                            .filter_map(|idx| atom_width(&atoms[idx]))
                            .sum();
                        records.push(emit_line(groups, &atoms, line_start, split, committed));
                        line_start = split;
                        i = split;
                        line_width = 0.0;
                    }
                    None => {
                        return Err(Error::from(ErrorKind::UnbreakableWiderThanFrame {
                            text: overflow_text(groups, &atoms, line_start, i),
                            width: line_width + width,
                            avail,
                        }));
                    }
                }
            }
        }
    }

    if line_start < atoms.len() {
        records.push(emit_line(
            groups,
            &atoms,
            line_start,
            atoms.len(),
            line_width,
        ));
    } else {
        records.push(emit_line(groups, &atoms, line_start, line_start, 0.0));
    }
    Ok(records)
}

fn overflow_text<T>(
    groups: &[&ClusteredText],
    atoms: &[Atom<T>],
    start: usize,
    end: usize,
) -> Option<String> {
    let mut text = String::new();
    for atom in &atoms[start..=end] {
        match *atom {
            Atom::Ink { group, cluster, .. } | Atom::Hard { group, cluster } => {
                text.push_str(groups[group].cluster(cluster).as_str());
            }
            Atom::Blank { .. } => {}
        }
    }
    let text = crate::error::text_snippet(&text);
    if text.is_empty() {
        None
    } else {
        Some(text)
    }
}

fn nearest_break<T>(atoms: &[Atom<T>], line_start: usize, overflow_at: usize) -> Option<usize> {
    for k in (line_start + 1..=overflow_at).rev() {
        if is_fixed_blank(&atoms[k]) {
            return Some(k);
        }
        let Some(next) = ink_pair(&atoms[k]) else {
            continue;
        };
        match left_ink(atoms, line_start, k) {
            LeftInk::Class(prev) if allows(prev, next) => return Some(k),
            LeftInk::LineStart => return Some(k),
            LeftInk::Class(_) | LeftInk::Blocked => {}
        }
    }
    None
}

enum LeftInk {
    Class(PairClass),
    Blocked,
    LineStart,
}

fn left_ink<T>(atoms: &[Atom<T>], line_start: usize, k: usize) -> LeftInk {
    let mut index = k;
    while index > line_start {
        index -= 1;
        match atoms[index] {
            Atom::Blank { .. } => {}
            Atom::Ink { pair, .. } => return LeftInk::Class(pair),
            Atom::Hard { .. } => return LeftInk::Blocked,
        }
    }
    LeftInk::LineStart
}

fn is_fixed_blank<T>(atom: &Atom<T>) -> bool {
    matches!(
        atom,
        Atom::Blank {
            advance: Advance::Fixed(_),
            ..
        }
    )
}

fn ink_pair<T>(atom: &Atom<T>) -> Option<PairClass> {
    match *atom {
        Atom::Ink { pair, .. } => Some(pair),
        Atom::Hard { .. } | Atom::Blank { .. } => None,
    }
}

fn atom_width<T>(atom: &Atom<T>) -> Option<f32> {
    match *atom {
        Atom::Ink { width, .. } | Atom::Blank { width, .. } => Some(width),
        Atom::Hard { .. } => None,
    }
}

fn emit_line<T: Copy>(
    groups: &[&ClusteredText],
    atoms: &[Atom<T>],
    start: usize,
    end: usize,
    width: f32,
) -> SpanLine<T> {
    if start >= end {
        return SpanLine {
            segs: Vec::new(),
            width: 0.0,
            empty_at: Some(eof_cursor(groups, atoms, start)),
        };
    }

    let mut segs = Vec::new();
    let mut index = start;
    while index < end {
        match atoms[index] {
            Atom::Hard { .. } => unreachable!("hard breaks are not emitted inside a line"),
            Atom::Blank { width, payload, .. } => {
                segs.push(Seg::Blank { width, payload });
                index += 1;
            }
            Atom::Ink { group, cluster, .. } => {
                let start_cursor = groups[group].cluster(cluster).start_cursor();
                let mut last = cluster;
                index += 1;
                while index < end {
                    match atoms[index] {
                        Atom::Ink {
                            group: next_group,
                            cluster: next_cluster,
                            ..
                        } if next_group == group => {
                            last = next_cluster;
                            index += 1;
                        }
                        _ => break,
                    }
                }
                segs.push(Seg::Glyphs {
                    group,
                    start: start_cursor,
                    end: groups[group].cluster(last).end_cursor(),
                });
            }
        }
    }
    SpanLine {
        segs,
        width,
        empty_at: None,
    }
}

fn eof_cursor<T>(
    groups: &[&ClusteredText],
    atoms: &[Atom<T>],
    atom_index: usize,
) -> (usize, ClusterCursor) {
    let origin = ClusterCursor {
        cluster: 0,
        glyph: 0,
        glyph_byte: 0,
    };
    if atom_index == 0 || groups.is_empty() {
        return (0, origin);
    }
    let mut index = atom_index;
    while index > 0 {
        index -= 1;
        match atoms[index] {
            Atom::Hard { group, cluster } | Atom::Ink { group, cluster, .. } => {
                return (group, groups[group].cluster(cluster).end_cursor());
            }
            Atom::Blank { .. } => {}
        }
    }
    (0, origin)
}

fn facade_record(line: &SpanLine<()>) -> LineRecord {
    let mut glyphs = line.segs.iter().filter_map(|seg| match *seg {
        Seg::Glyphs { start, end, .. } => Some((start, end)),
        Seg::Blank { .. } => None,
    });
    if let Some((start, end)) = glyphs.next() {
        debug_assert!(glyphs.next().is_none());
        return LineRecord {
            start,
            end,
            width: line.width,
        };
    }
    let cursor = line.empty_at.map_or(
        ClusterCursor {
            cluster: 0,
            glyph: 0,
            glyph_byte: 0,
        },
        |(_, cursor)| cursor,
    );
    LineRecord {
        start: cursor,
        end: cursor,
        width: 0.0,
    }
}

impl<T> BrokenSpans<T> {
    pub(crate) fn len(&self) -> usize {
        self.lines.len()
    }

    pub(crate) fn get(&self, index: usize) -> &SpanLine<T> {
        &self.lines[index]
    }

    pub(crate) fn iter(&self) -> impl ExactSizeIterator<Item = &SpanLine<T>> + '_ {
        self.lines.iter()
    }
}

impl<T> SpanLine<T> {
    pub(crate) fn segs(&self) -> &[Seg<T>] {
        &self.segs
    }

    pub(crate) fn width(&self) -> f32 {
        self.width
    }

    pub(crate) fn empty_at(&self) -> Option<(usize, ClusterCursor)> {
        self.empty_at
    }
}

impl BrokenLines<'_> {
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

    pub(crate) fn width(&self) -> f32 {
        self.record.width
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn break_owned(source: &str, avail: f32) -> Result<Vec<(String, f32, Range<usize>)>> {
        let text = ClusteredText::new(source);
        let broken = break_lines(&text, LineBudget::uniform(avail), |cluster| {
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
            Error::from(ErrorKind::UnbreakableWiderThanFrame {
                text: Some("hello".to_owned()),
                width: 5.0,
                avail: 4.0
            })
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
            Error::from(ErrorKind::UnbreakableWiderThanFrame {
                text: Some("字。".to_owned()),
                width: 2.0,
                avail: 1.0
            })
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
            Error::from(ErrorKind::UnbreakableWiderThanFrame {
                text: Some("e\u{0301}".to_owned()),
                width: 2.0,
                avail: 1.0
            })
        );
    }

    #[test]
    fn other_run_unbreakable() {
        let err = break_owned("\u{FF21}\u{FF22}", 1.0).unwrap_err();
        assert_eq!(
            err,
            Error::from(ErrorKind::UnbreakableWiderThanFrame {
                text: Some("\u{FF21}\u{FF22}".to_owned()),
                width: 2.0,
                avail: 1.0
            })
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
        assert_eq!(
            break_owned("A", 0.0).unwrap_err(),
            GeometryKind::LineWidth { width: 0.0 }.into()
        );
        assert!(matches!(
            break_owned("A", f32::NAN).unwrap_err().kind,
            ErrorKind::InvalidGeometry(GeometryKind::LineWidth { width }) if width.is_nan()
        ));
    }

    #[test]
    fn first_and_rest_budgets_follow_visual_lines() {
        let text = ClusteredText::new("你你好");
        let broken = break_lines(
            &text,
            LineBudget {
                first: 1.0,
                rest: 2.0,
            },
            |_| 1.0,
        )
        .unwrap();
        assert_eq!(
            broken
                .iter()
                .map(|line| line.text().to_owned())
                .collect::<Vec<_>>(),
            ["你", "你好"]
        );

        let hard = ClusteredText::new("你\n你好");
        let broken = break_lines(
            &hard,
            LineBudget {
                first: 1.0,
                rest: 1.0,
            },
            |_| 1.0,
        )
        .unwrap();
        assert_eq!(
            broken
                .iter()
                .map(|line| line.text().to_owned())
                .collect::<Vec<_>>(),
            ["你", "你", "好"]
        );
    }

    #[test]
    fn first_line_overflow_reports_the_first_budget() {
        let text = ClusteredText::new("你");
        let error = break_lines(
            &text,
            LineBudget {
                first: 0.5,
                rest: 10.0,
            },
            |_| 1.0,
        )
        .unwrap_err();
        assert_eq!(
            error,
            Error::from(ErrorKind::UnbreakableWiderThanFrame {
                text: Some("你".to_owned()),
                width: 1.0,
                avail: 0.5,
            })
        );
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

    fn break_groups(
        groups: &[ClusteredText],
        spans: &[Span<()>],
        budget: LineBudget,
    ) -> Result<BrokenSpans<()>> {
        let refs: Vec<&ClusteredText> = groups.iter().collect();
        break_spans(&refs, spans, budget, |_, cluster| {
            f32::from(u16::try_from(cluster.scalar_count()).expect("stub cluster scalar count"))
        })
    }

    fn glyph_text(groups: &[ClusteredText], seg: &Seg<()>) -> String {
        match *seg {
            Seg::Glyphs { group, start, end } => {
                groups[group].glyph_text()[start.glyph_byte..end.glyph_byte].to_owned()
            }
            Seg::Blank { .. } => String::new(),
        }
    }

    #[test]
    fn fixed_blank_sits_between_glyph_segments() {
        let groups = [ClusteredText::new("AB"), ClusteredText::new("C")];
        let spans = [
            Span::Group(0),
            Span::Blank {
                advance: Advance::Fixed(3.0),
                payload: (),
            },
            Span::Group(1),
        ];
        let broken = break_groups(&groups, &spans, LineBudget::uniform(10.0)).unwrap();
        assert_eq!(broken.len(), 1);
        assert_width(broken.get(0).width(), 6.0);
        let segs = broken.get(0).segs();
        assert!(matches!(
            segs[1],
            Seg::Blank { width, .. } if width.to_bits() == 3.0f32.to_bits()
        ));
        assert_eq!(glyph_text(&groups, &segs[0]), "AB");
        assert_eq!(glyph_text(&groups, &segs[2]), "C");
    }

    #[test]
    fn fixed_blank_moves_whole_when_the_line_is_short() {
        let left = ClusteredText::new("AA");
        let groups = [left];
        let spans = [
            Span::Group(0),
            Span::Blank {
                advance: Advance::Fixed(4.0),
                payload: (),
            },
        ];
        let broken = break_groups(
            &groups,
            &spans,
            LineBudget {
                first: 2.0,
                rest: 5.0,
            },
        )
        .unwrap();
        assert_eq!(broken.len(), 2);
        assert!(matches!(broken.get(0).segs(), [Seg::Glyphs { .. }]));
        assert!(matches!(
            broken.get(1).segs(),
            [Seg::Blank { width, .. }] if width.to_bits() == 4.0f32.to_bits()
        ));
    }

    #[test]
    fn fixed_blank_wider_than_first_budget_uses_that_budget() {
        let groups = [ClusteredText::new("")];
        let spans = [Span::Blank {
            advance: Advance::Fixed(4.0),
            payload: (),
        }];
        let error = break_groups(
            &groups,
            &spans,
            LineBudget {
                first: 2.0,
                rest: 5.0,
            },
        )
        .unwrap_err();
        assert_eq!(
            error,
            Error::from(ErrorKind::UnbreakableWiderThanFrame {
                text: None,
                width: 4.0,
                avail: 2.0,
            })
        );
    }

    #[test]
    fn opening_paren_stays_when_the_following_blank_does_not_fit() {
        let groups = [ClusteredText::new("（")];
        let spans = [
            Span::Group(0),
            Span::Blank {
                advance: Advance::Fixed(5.0),
                payload: (),
            },
        ];
        let broken = break_groups(
            &groups,
            &spans,
            LineBudget {
                first: 2.0,
                rest: 10.0,
            },
        )
        .unwrap();
        assert_eq!(glyph_text(&groups, &broken.get(0).segs()[0]), "（");
        assert!(matches!(broken.get(1).segs(), [Seg::Blank { .. }]));
    }

    #[test]
    fn fill_sets_the_line_width_to_the_budget() {
        let groups = [ClusteredText::new("ABC")];
        let spans = [
            Span::Group(0),
            Span::Blank {
                advance: Advance::Fill,
                payload: (),
            },
        ];
        let broken = break_groups(&groups, &spans, LineBudget::uniform(10.0)).unwrap();
        assert_eq!(broken.len(), 1);
        assert_width(broken.get(0).width(), 10.0);
        assert!(matches!(
            broken.get(0).segs()[1],
            Seg::Blank { width, .. } if width.to_bits() == 7.0f32.to_bits()
        ));
    }

    #[test]
    fn fill_on_a_full_line_keeps_the_budget_and_zero_width() {
        let groups = [ClusteredText::new("ABCDE")];
        let spans = [
            Span::Group(0),
            Span::Blank {
                advance: Advance::Fill,
                payload: (),
            },
        ];
        let broken = break_groups(&groups, &spans, LineBudget::uniform(5.0)).unwrap();
        assert_eq!(broken.len(), 1);
        assert_width(broken.get(0).width(), 5.0);
        assert!(matches!(
            broken.get(0).segs()[1],
            Seg::Blank { width, .. } if width.to_bits() == 0.0f32.to_bits()
        ));
    }

    #[test]
    fn fill_after_a_hard_break_uses_the_rest_budget() {
        let groups = [ClusteredText::new("A\n")];
        let spans = [
            Span::Group(0),
            Span::Blank {
                advance: Advance::Fill,
                payload: (),
            },
        ];
        let broken = break_groups(
            &groups,
            &spans,
            LineBudget {
                first: 4.0,
                rest: 9.0,
            },
        )
        .unwrap();
        assert_eq!(broken.len(), 2);
        assert_width(broken.get(0).width(), 1.0);
        assert_width(broken.get(1).width(), 9.0);
        assert!(matches!(
            broken.get(1).segs(),
            [Seg::Blank { width, .. }] if width.to_bits() == 9.0f32.to_bits()
        ));
    }

    #[test]
    fn hard_cursor_after_a_blank_uses_the_stored_cluster() {
        let groups = [ClusteredText::new("A"), ClusteredText::new("B\n")];
        let spans = [
            Span::Group(0),
            Span::Blank {
                advance: Advance::Fixed(1.0),
                payload: (),
            },
            Span::Group(1),
        ];
        let broken = break_groups(&groups, &spans, LineBudget::uniform(10.0)).unwrap();
        assert_eq!(broken.len(), 2);
        let hard_end = groups[1].cluster(1).end_cursor();
        assert_eq!(broken.get(1).empty_at(), Some((1, hard_end)));
        assert!(broken.get(1).segs().is_empty());
    }

    #[test]
    fn blank_is_transparent_to_a_forbidden_pair() {
        let groups = [ClusteredText::new("："), ClusteredText::new("。")];
        let spans = [
            Span::Group(0),
            Span::Blank {
                advance: Advance::Fixed(1.0),
                payload: (),
            },
            Span::Group(1),
        ];
        let broken = break_groups(&groups, &spans, LineBudget::uniform(2.0)).unwrap();
        assert_eq!(broken.len(), 2);
        assert_eq!(glyph_text(&groups, &broken.get(0).segs()[0]), "：");
        assert!(matches!(broken.get(1).segs()[0], Seg::Blank { .. }));
        assert_eq!(glyph_text(&groups, &broken.get(1).segs()[1]), "。");
    }
}
