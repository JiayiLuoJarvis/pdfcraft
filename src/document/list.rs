#![allow(clippy::wildcard_imports)]
use super::*;

/// Marker numbering of an ordered list. Each label ends with `.`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Numbering {
    /// 1. 2. 3.
    Decimal,
    /// A. B. C., up to Z.
    UpperAlpha,
    /// a. b. c., up to z.
    LowerAlpha,
    /// I. II. III., up to 3999.
    UpperRoman,
    /// i. ii. iii., up to 3999.
    LowerRoman,
}

/// Items being added to a list. Markers hang in an 18 pt column by default.
pub struct List {
    pub(crate) parent: Flow,
    pub(crate) items: Vec<Result<ListItem>>,
    pub(crate) start: NonZeroU32,
    pub(crate) marker_width: Length<Positive>,
    pub(crate) nest_indent: Option<Length<Positive>>,
    pub(crate) error: Option<Error>,
}

impl List {
    /// First number of an ordered list. Default 1.
    ///
    /// Ignored on a bulleted list.
    pub fn start(&mut self, start: NonZeroU32) {
        self.start = start;
    }

    /// Width of the hanging marker column. Default 18 pt.
    pub fn marker_width(&mut self, width: impl IntoPositive) {
        match width.into_positive() {
            Ok(width) => self.marker_width = width,
            Err(error) => self.note(error),
        }
    }

    /// Extra inset of nested list bodies. Default is the marker column width.
    pub fn nest_indent(&mut self, indent: impl IntoPositive) {
        match indent.into_positive() {
            Ok(indent) => self.nest_indent = Some(indent),
            Err(error) => self.note(error),
        }
    }

    /// Adds an item with the list's own marker.
    pub fn item(&mut self, f: impl FnOnce(&mut Flow)) {
        let item = self.parent.nested(f).and_then(ListItem::new);
        self.items.push(item);
    }

    /// Adds an item with `marker` in place of the list's marker. An empty marker is an error.
    ///
    /// A custom marker does not consume an ordered number.
    pub fn item_marked(&mut self, marker: impl Into<String>, f: impl FnOnce(&mut Flow)) {
        let marker = marker.into();
        let item = self
            .parent
            .nested(f)
            .and_then(|nodes| ListItem::custom(marker, nodes));
        self.items.push(item);
    }

    pub(crate) fn note(&mut self, error: Error) {
        if self.error.is_none() {
            self.error = Some(error);
        }
    }

    pub(crate) fn lower(self, kind: ListKind) -> Result<FlowNode> {
        if let Some(error) = self.error {
            return Err(error);
        }
        let spec = ListSpec {
            kind,
            marker_width: self.marker_width,
            nest_indent: self.nest_indent,
            marker_style: self.parent.style,
            items: self.items.into_iter().collect::<Result<_>>()?,
        };
        Ok(FlowNode::List(ListNode::new(spec)?))
    }
}
