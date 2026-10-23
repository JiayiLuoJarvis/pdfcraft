//! Pure line-breaking over clustered text.

#[cfg(test)]
use std::ops::Range;

use crate::error::{Error, ErrorKind, Fallible as Result, GeometryKind};
use crate::geom::{Finite, Length, NonNegative, Positive};
use crate::text::break_class::{self, BreakClass, PairClass};
use crate::text::cluster::{Cluster, ClusterCursor, ClusteredText};

/// Width of one text cluster. `gap` is tracking that counts only before more text.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct InkWidth {
    pub(crate) body: Length<NonNegative>,
    pub(crate) gap: Length<Finite>,
}

/// One atom on a line, in line order, as [`draw_widths`] sees it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum WidthPart {
    Ink(InkWidth),
    Blank(Length<NonNegative>),
}

/// Drawn width of each part. A cluster's `gap` counts only when the next part is
/// also a cluster, so a blank, a hard break, or the line end drops it.
pub(crate) fn draw_widths(parts: &[WidthPart]) -> impl Iterator<Item = Length<NonNegative>> + '_ {
    parts
        .iter()
        .enumerate()
        .map(move |(index, part)| match *part {
            WidthPart::Blank(width) => width,
            WidthPart::Ink(ink) => match parts.get(index + 1) {
                Some(WidthPart::Ink(_)) => {
                    Length::from_trusted_pt((ink.body.as_pt() + ink.gap.as_pt()).max(0.0))
                }
                Some(WidthPart::Blank(_)) | None => ink.body,
            },
        })
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum Advance {
    Fixed(Length<NonNegative>),
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
        width: Length<NonNegative>,
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
        width: InkWidth,
        group: usize,
        cluster: usize,
    },
    Blank {
        advance: Advance,
        width: Length<NonNegative>,
        payload: T,
    },
}

/// Lines produced by [`break_lines`].
#[cfg(test)]
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct BrokenLines<'text> {
    text: &'text ClusteredText,
    records: Box<[LineRecord]>,
}

#[cfg(test)]
#[derive(Debug, Clone, PartialEq)]
struct LineRecord {
    start: ClusterCursor,
    end: ClusterCursor,
    width: Length<NonNegative>,
}

/// One broken line over `glyph_text` coordinates.
#[cfg(test)]
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
    width: Length<NonNegative>,
    empty_at: Option<(usize, ClusterCursor)>,
    /// Closed because the next atom was Hard (`\n`). Hard itself is not in segs.
    ended_by_hard_break: bool,
    /// Contained `Advance::Fill` before width resolve (even when remainder is 0).
    has_fill: bool,
}

#[derive(Clone, Copy)]
struct EmitFlags {
    ended_by_hard_break: bool,
    has_fill: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct LineBudget {
    pub(crate) first: Length<Positive>,
    pub(crate) rest: Length<Positive>,
}

impl LineBudget {
    #[cfg(test)]
    pub(crate) const fn uniform(width: Length<Positive>) -> Self {
        Self {
            first: width,
            rest: width,
        }
    }

    fn for_line(self, index: usize) -> Length<Positive> {
        if index == 0 {
            self.first
        } else {
            self.rest
        }
    }
}

/// Break `text` to fit `budget` using `width_of` for each non-BK cluster.
#[cfg(test)]
pub(crate) fn break_lines(
    text: &ClusteredText,
    budget: LineBudget,
    mut width_of: impl FnMut(Cluster<'_>) -> Length<NonNegative>,
) -> Result<BrokenLines<'_>> {
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
                width: NonNegative::ZERO,
            }]),
        });
    }

    let spans = [Span::<()>::Group(0)];
    let broken = break_spans(std::slice::from_ref(&text), &spans, budget, |_, cluster| {
        Ok(InkWidth {
            body: width_of(cluster),
            gap: Finite::ZERO,
        })
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
    mut width_of: impl FnMut(usize, Cluster<'_>) -> Result<InkWidth>,
) -> Result<BrokenSpans<T>> {
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
    width_of: &mut impl FnMut(usize, Cluster<'_>) -> Result<InkWidth>,
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
                            let width = width_of(group, cluster)?;
                            let body = width.body.as_pt();
                            if !body.is_finite() || body < 0.0 || !width.gap.as_pt().is_finite() {
                                return Err(GeometryKind::AdvanceWidth { width: body }.into());
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
                        if !width.as_pt().is_finite() || width.as_pt() < 0.0 {
                            return Err(GeometryKind::AdvanceWidth {
                                width: width.as_pt(),
                            }
                            .into());
                        }
                        width
                    }
                    Advance::Fill => NonNegative::ZERO,
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

// Every emit path stamps hard/fill flags; extracting arms would obscure the scan.
#[allow(clippy::too_many_lines)]
fn pack_lines<T: Copy>(
    groups: &[&ClusteredText],
    mut atoms: Vec<Atom<T>>,
    budget: LineBudget,
) -> Result<Vec<SpanLine<T>>> {
    let mut records = Vec::new();
    let mut line_start = 0usize;
    let mut i = 0usize;
    // Drawn widths of every line atom before `tail`. `tail`'s gap waits for the next atom.
    let mut settled = NonNegative::ZERO;
    let mut tail: Option<WidthPart> = None;
    let mut line_has_fill = false;

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
                let width = closed_width(settled, tail);
                records.push(emit_line(
                    groups,
                    &atoms,
                    line_start,
                    i,
                    width,
                    EmitFlags {
                        ended_by_hard_break: true,
                        has_fill: line_has_fill,
                    },
                ));
                i += 1;
                line_start = i;
                settled = NonNegative::ZERO;
                tail = None;
                line_has_fill = false;
            }
            Atom::Blank {
                advance: Advance::Fill,
                ..
            } => {
                let line_width = closed_width(settled, tail);
                debug_assert!(
                    line_width.as_pt() <= avail.as_pt(),
                    "closed line {line_width:?} must fit in avail {avail:?}"
                );
                let remain = (avail - line_width).as_pt();
                let width = if remain.is_finite() && remain > 0.0 {
                    Length::from_trusted_pt(remain)
                } else {
                    NonNegative::ZERO
                };
                if let Atom::Blank { width: slot, .. } = &mut atoms[i] {
                    *slot = width;
                }
                settled = avail.into();
                tail = None;
                line_has_fill = true;
                i += 1;
            }
            Atom::Ink { .. } | Atom::Blank { .. } => {
                let part = width_part(&atoms[i]).expect("ink and blank atoms have widths");
                let (before, trial) = append_width(settled, tail, part);
                if trial.as_pt() <= avail.as_pt() {
                    settled = before;
                    tail = Some(part);
                    i += 1;
                    continue;
                }
                match nearest_break(&atoms, line_start, i) {
                    Some(split) => {
                        let committed = line_width(&atoms[line_start..split]);
                        records.push(emit_line(
                            groups,
                            &atoms,
                            line_start,
                            split,
                            committed,
                            EmitFlags {
                                ended_by_hard_break: false,
                                has_fill: line_has_fill,
                            },
                        ));
                        line_start = split;
                        i = split;
                        settled = NonNegative::ZERO;
                        tail = None;
                        line_has_fill = false;
                    }
                    None => {
                        return Err(Error::from(ErrorKind::UnbreakableWiderThanFrame {
                            text: overflow_text(groups, &atoms, line_start, i),
                            width: trial.as_pt(),
                            avail: avail.as_pt(),
                        }));
                    }
                }
            }
        }
    }

    let soft = EmitFlags {
        ended_by_hard_break: false,
        has_fill: line_has_fill,
    };
    if line_start < atoms.len() {
        records.push(emit_line(
            groups,
            &atoms,
            line_start,
            atoms.len(),
            closed_width(settled, tail),
            soft,
        ));
    } else {
        // Trailing empty after Hard is not itself hard-ended.
        records.push(emit_line(
            groups,
            &atoms,
            line_start,
            line_start,
            NonNegative::ZERO,
            EmitFlags {
                ended_by_hard_break: false,
                has_fill: false,
            },
        ));
    }
    Ok(records)
}

/// Settled width once `tail` is followed by `part`, and the line width if
/// `part` then ends the line.
fn append_width(
    settled: Length<NonNegative>,
    tail: Option<WidthPart>,
    part: WidthPart,
) -> (Length<NonNegative>, Length<NonNegative>) {
    let Some(prev) = tail else {
        return (settled, closed_width(settled, Some(part)));
    };
    let pair = [prev, part];
    let mut drawn = draw_widths(&pair);
    let before = settled + drawn.next().unwrap_or(NonNegative::ZERO);
    (before, before + drawn.next().unwrap_or(NonNegative::ZERO))
}

/// Line width when `tail` ends the line, so its gap is dropped.
fn closed_width(settled: Length<NonNegative>, tail: Option<WidthPart>) -> Length<NonNegative> {
    match tail {
        Some(part) => settled + draw_widths(&[part]).sum::<Length<NonNegative>>(),
        None => settled,
    }
}

fn line_width<T>(atoms: &[Atom<T>]) -> Length<NonNegative> {
    let parts: Vec<_> = atoms.iter().filter_map(width_part).collect();
    draw_widths(&parts).sum()
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
            LeftInk::Class(prev) if break_class::allows(prev, next) => return Some(k),
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

fn width_part<T>(atom: &Atom<T>) -> Option<WidthPart> {
    match *atom {
        Atom::Ink { width, .. } => Some(WidthPart::Ink(width)),
        Atom::Blank { width, .. } => Some(WidthPart::Blank(width)),
        Atom::Hard { .. } => None,
    }
}

fn emit_line<T: Copy>(
    groups: &[&ClusteredText],
    atoms: &[Atom<T>],
    start: usize,
    end: usize,
    width: Length<NonNegative>,
    flags: EmitFlags,
) -> SpanLine<T> {
    if start >= end {
        return SpanLine {
            segs: Vec::new(),
            width: NonNegative::ZERO,
            empty_at: Some(eof_cursor(groups, atoms, start)),
            ended_by_hard_break: flags.ended_by_hard_break,
            has_fill: flags.has_fill,
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
        ended_by_hard_break: flags.ended_by_hard_break,
        has_fill: flags.has_fill,
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

#[cfg(test)]
fn facade_record(line: &SpanLine<()>) -> LineRecord {
    let mut glyphs = line.segs.iter().filter_map(|seg| match *seg {
        Seg::Glyphs { start, end, .. } => Some((start, end)),
        Seg::Blank { .. } => None,
    });
    if let Some((start, end)) = glyphs.next() {
        debug_assert!(
            glyphs.next().is_none(),
            "facade line holds a single glyph span"
        );
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
        width: NonNegative::ZERO,
    }
}

impl<T> BrokenSpans<T> {
    pub(crate) fn len(&self) -> usize {
        self.lines.len()
    }

    #[cfg(test)]
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

    pub(crate) fn width(&self) -> Length<NonNegative> {
        self.width
    }

    pub(crate) fn ended_by_hard_break(&self) -> bool {
        self.ended_by_hard_break
    }

    pub(crate) fn has_fill(&self) -> bool {
        self.has_fill
    }

    #[cfg(test)]
    pub(crate) fn empty_at(&self) -> Option<(usize, ClusterCursor)> {
        self.empty_at
    }
}

#[cfg(test)]
impl BrokenLines<'_> {
    pub(crate) fn iter(&self) -> impl ExactSizeIterator<Item = BrokenLine<'_>> + '_ {
        self.records.iter().map(|record| BrokenLine {
            text: self.text,
            record,
        })
    }
}

#[cfg(test)]
impl BrokenLine<'_> {
    pub(crate) fn text(&self) -> &str {
        let start = self.record.start.glyph_byte;
        let end = self.record.end.glyph_byte;
        &self.text.glyph_text()[start..end]
    }

    pub(crate) fn glyph_range(&self) -> Range<usize> {
        self.record.start.glyph..self.record.end.glyph
    }

    pub(crate) fn width(&self) -> Length<NonNegative> {
        self.record.width
    }
}

#[cfg(test)]
mod tests {
    use std::ops::Range;

    use super::*;

    fn pos(value: f32) -> Length<Positive> {
        Positive::pt(value).unwrap()
    }

    fn nn(value: f32) -> Length<NonNegative> {
        NonNegative::pt(value).unwrap()
    }

    fn break_owned(source: &str, avail: f32) -> Result<Vec<(String, f32, Range<usize>)>> {
        let text = ClusteredText::new(source);
        let broken = break_lines(&text, LineBudget::uniform(pos(avail)), |cluster| {
            let count = f32::from(
                u16::try_from(cluster.scalar_count()).expect("stub cluster scalar count"),
            );
            Length::from_trusted_pt(count)
        })?;
        Ok(broken
            .iter()
            .map(|line| {
                (
                    line.text().to_owned(),
                    line.width().as_pt(),
                    line.glyph_range(),
                )
            })
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
    fn first_and_rest_budgets_follow_visual_lines() {
        let text = ClusteredText::new("你你好");
        let broken = break_lines(
            &text,
            LineBudget {
                first: pos(1.0),
                rest: pos(2.0),
            },
            |_| nn(1.0),
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
                first: pos(1.0),
                rest: pos(1.0),
            },
            |_| nn(1.0),
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
                first: pos(0.5),
                rest: pos(10.0),
            },
            |_| nn(1.0),
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
    fn hard_break_sets_ended_flag_but_trailing_empty_does_not() {
        let text = ClusteredText::new("A\n");
        let broken = break_lines(&text, LineBudget::uniform(pos(10.0)), |_| nn(1.0)).unwrap();
        let groups = [&text];
        let spans = [Span::<()>::Group(0)];
        let full = break_spans(&groups, &spans, LineBudget::uniform(pos(10.0)), |_, _| {
            Ok(InkWidth {
                body: nn(1.0),
                gap: Finite::ZERO,
            })
        })
        .unwrap();
        assert_eq!(full.len(), 2);
        assert!(full.get(0).ended_by_hard_break());
        assert!(!full.get(0).has_fill());
        assert!(!full.get(1).ended_by_hard_break());
        assert!(!full.get(1).has_fill());
        assert_eq!(broken.iter().count(), 2);
    }

    #[test]
    fn soft_wrap_does_not_set_hard_flag() {
        let text = ClusteredText::new("你好");
        let groups = [&text];
        let spans = [Span::<()>::Group(0)];
        let broken = break_spans(&groups, &spans, LineBudget::uniform(pos(1.0)), |_, _| {
            Ok(InkWidth {
                body: nn(1.0),
                gap: Finite::ZERO,
            })
        })
        .unwrap();
        assert_eq!(broken.len(), 2);
        assert!(!broken.get(0).ended_by_hard_break());
        assert!(!broken.get(1).ended_by_hard_break());
        assert!(!broken.get(0).has_fill());
        assert!(!broken.get(1).has_fill());
    }

    #[test]
    fn fill_sets_has_fill_even_when_remainder_is_zero() {
        let groups = [ClusteredText::new("AB")];
        let spans = [
            Span::Group(0),
            Span::Blank {
                advance: Advance::Fill,
                payload: (),
            },
        ];
        let broken = break_groups(&groups, &spans, LineBudget::uniform(pos(2.0))).unwrap();
        assert_eq!(broken.len(), 1);
        assert!(broken.get(0).has_fill());
        assert!(!broken.get(0).ended_by_hard_break());
        assert!(matches!(
            broken.get(0).segs()[1],
            Seg::Blank { width, .. } if width.as_pt().to_bits() == 0.0f32.to_bits()
        ));
    }

    #[test]
    fn mid_hard_empty_line_is_hard_ended() {
        let text = ClusteredText::new("A\n\nB");
        let groups = [&text];
        let spans = [Span::<()>::Group(0)];
        let broken = break_spans(&groups, &spans, LineBudget::uniform(pos(10.0)), |_, _| {
            Ok(InkWidth {
                body: nn(1.0),
                gap: Finite::ZERO,
            })
        })
        .unwrap();
        assert_eq!(broken.len(), 3);
        assert!(broken.get(0).ended_by_hard_break());
        assert!(broken.get(1).ended_by_hard_break());
        assert!(broken.get(1).segs().is_empty());
        assert!(!broken.get(2).ended_by_hard_break());
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
            let count = f32::from(
                u16::try_from(cluster.scalar_count()).expect("stub cluster scalar count"),
            );
            Ok(InkWidth {
                body: Length::from_trusted_pt(count),
                gap: Finite::ZERO,
            })
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
                advance: Advance::Fixed(nn(3.0)),
                payload: (),
            },
            Span::Group(1),
        ];
        let broken = break_groups(&groups, &spans, LineBudget::uniform(pos(10.0))).unwrap();
        assert_eq!(broken.len(), 1);
        assert_width(broken.get(0).width().as_pt(), 6.0);
        let segs = broken.get(0).segs();
        assert!(matches!(
            segs[1],
            Seg::Blank { width, .. } if width.as_pt().to_bits() == 3.0f32.to_bits()
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
                advance: Advance::Fixed(nn(4.0)),
                payload: (),
            },
        ];
        let broken = break_groups(
            &groups,
            &spans,
            LineBudget {
                first: pos(2.0),
                rest: pos(5.0),
            },
        )
        .unwrap();
        assert_eq!(broken.len(), 2);
        assert!(matches!(broken.get(0).segs(), [Seg::Glyphs { .. }]));
        assert!(matches!(
            broken.get(1).segs(),
            [Seg::Blank { width, .. }] if width.as_pt().to_bits() == 4.0f32.to_bits()
        ));
    }

    #[test]
    fn fixed_blank_wider_than_first_budget_uses_that_budget() {
        let groups = [ClusteredText::new("")];
        let spans = [Span::Blank {
            advance: Advance::Fixed(nn(4.0)),
            payload: (),
        }];
        let error = break_groups(
            &groups,
            &spans,
            LineBudget {
                first: pos(2.0),
                rest: pos(5.0),
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
                advance: Advance::Fixed(nn(5.0)),
                payload: (),
            },
        ];
        let broken = break_groups(
            &groups,
            &spans,
            LineBudget {
                first: pos(2.0),
                rest: pos(10.0),
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
        let broken = break_groups(&groups, &spans, LineBudget::uniform(pos(10.0))).unwrap();
        assert_eq!(broken.len(), 1);
        assert_width(broken.get(0).width().as_pt(), 10.0);
        assert!(matches!(
            broken.get(0).segs()[1],
            Seg::Blank { width, .. } if width.as_pt().to_bits() == 7.0f32.to_bits()
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
        let broken = break_groups(&groups, &spans, LineBudget::uniform(pos(5.0))).unwrap();
        assert_eq!(broken.len(), 1);
        assert_width(broken.get(0).width().as_pt(), 5.0);
        assert!(matches!(
            broken.get(0).segs()[1],
            Seg::Blank { width, .. } if width.as_pt().to_bits() == 0.0f32.to_bits()
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
                first: pos(4.0),
                rest: pos(9.0),
            },
        )
        .unwrap();
        assert_eq!(broken.len(), 2);
        assert_width(broken.get(0).width().as_pt(), 1.0);
        assert_width(broken.get(1).width().as_pt(), 9.0);
        assert!(matches!(
            broken.get(1).segs(),
            [Seg::Blank { width, .. }] if width.as_pt().to_bits() == 9.0f32.to_bits()
        ));
    }

    #[test]
    fn hard_cursor_after_a_blank_uses_the_stored_cluster() {
        let groups = [ClusteredText::new("A"), ClusteredText::new("B\n")];
        let spans = [
            Span::Group(0),
            Span::Blank {
                advance: Advance::Fixed(nn(1.0)),
                payload: (),
            },
            Span::Group(1),
        ];
        let broken = break_groups(&groups, &spans, LineBudget::uniform(pos(10.0))).unwrap();
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
                advance: Advance::Fixed(nn(1.0)),
                payload: (),
            },
            Span::Group(1),
        ];
        let broken = break_groups(&groups, &spans, LineBudget::uniform(pos(2.0))).unwrap();
        assert_eq!(broken.len(), 2);
        assert_eq!(glyph_text(&groups, &broken.get(0).segs()[0]), "：");
        assert!(matches!(broken.get(1).segs()[0], Seg::Blank { .. }));
        assert_eq!(glyph_text(&groups, &broken.get(1).segs()[1]), "。");
    }

    fn ink(body: f32, gap: f32) -> WidthPart {
        WidthPart::Ink(InkWidth {
            body: nn(body),
            gap: Finite::pt(gap).unwrap(),
        })
    }

    fn drawn(parts: &[WidthPart]) -> Vec<f32> {
        draw_widths(parts).map(Length::as_pt).collect()
    }

    #[test]
    fn tracking_gap_follows_every_cluster_but_the_last() {
        assert_eq!(
            drawn(&[ink(1.0, 0.5), ink(1.0, 0.5), ink(1.0, 0.5)]),
            [1.5, 1.5, 1.0]
        );
    }

    #[test]
    fn a_blank_drops_the_tracking_gap_before_it() {
        assert_eq!(
            drawn(&[ink(1.0, 0.5), WidthPart::Blank(nn(2.0)), ink(1.0, 0.5)]),
            [1.0, 2.0, 1.0]
        );
    }

    #[test]
    fn negative_tracking_narrows_the_drawn_width() {
        assert_eq!(drawn(&[ink(1.0, -0.25), ink(1.0, -0.25)]), [0.75, 1.0]);
    }

    #[test]
    fn the_trailing_gap_does_not_count_toward_the_fit() {
        let groups = [ClusteredText::new("字字字")];
        let refs: Vec<&ClusteredText> = groups.iter().collect();
        let broken = break_spans(
            &refs,
            &[Span::<()>::Group(0)],
            LineBudget::uniform(pos(3.0)),
            |_, _| {
                Ok(InkWidth {
                    body: nn(1.0),
                    gap: Finite::pt(1.0).unwrap(),
                })
            },
        )
        .unwrap();
        assert_eq!(broken.len(), 2);
        assert_eq!(glyph_text(&groups, &broken.get(0).segs()[0]), "字字");
        assert_width(broken.get(0).width().as_pt(), 3.0);
        assert_eq!(glyph_text(&groups, &broken.get(1).segs()[0]), "字");
        assert_width(broken.get(1).width().as_pt(), 1.0);
    }
}
