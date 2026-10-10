#![allow(clippy::wildcard_imports)]
use super::*;

pub(crate) struct BandSlot {
    pub(crate) text: String,
    pub(crate) link: Option<LinkTarget>,
}

/// One page's header or footer band.
///
/// The band is as tall as its margin and spans the full page width. Call
/// [`Band::inset`] when slots should line up with the body. Content that does
/// not fit is an error.
pub struct Band {
    pub(crate) page: Page,
    pub(crate) left: Option<BandSlot>,
    pub(crate) right: Option<BandSlot>,
    pub(crate) inset_left: Length<NonNegative>,
    pub(crate) inset_right: Length<NonNegative>,
    pub(crate) error: Option<Error>,
}

impl Band {
    /// The page being drawn.
    #[must_use]
    pub fn page(&self) -> Page {
        self.page
    }

    /// Shrinks the slot area inward from the page edges.
    ///
    /// Defaults are zero. Side margins do not apply automatically; pass
    /// [`Page::margins`] when the slots should align with the body.
    pub fn inset(&mut self, left: impl IntoNonNegative, right: impl IntoNonNegative) {
        match (left.into_non_negative(), right.into_non_negative()) {
            (Ok(left), Ok(right)) => {
                self.inset_left = left;
                self.inset_right = right;
            }
            (Err(error), _) | (_, Err(error)) => self.note(error),
        }
    }

    /// Sets the start-aligned text.
    pub fn left(&mut self, s: impl AsRef<str>) {
        self.left = Some(BandSlot {
            text: s.as_ref().to_owned(),
            link: None,
        });
    }

    /// Sets the end-aligned text.
    pub fn right(&mut self, s: impl AsRef<str>) {
        self.right = Some(BandSlot {
            text: s.as_ref().to_owned(),
            link: None,
        });
    }

    /// Sets the start-aligned slot to `text` linked to `uri`.
    ///
    /// An empty `uri` is stored as [`Error::EmptyLink`] and leaves the slot unchanged.
    pub fn left_link(&mut self, uri: impl AsRef<str>, text: impl AsRef<str>) {
        self.set_link(true, uri.as_ref(), text.as_ref());
    }

    /// Sets the end-aligned slot to `text` linked to `uri`.
    ///
    /// An empty `uri` is stored as [`Error::EmptyLink`] and leaves the slot unchanged.
    pub fn right_link(&mut self, uri: impl AsRef<str>, text: impl AsRef<str>) {
        self.set_link(false, uri.as_ref(), text.as_ref());
    }

    fn set_link(&mut self, left: bool, uri: &str, text: &str) {
        let target = match LinkTarget::uri(uri) {
            Ok(target) => target,
            Err(error) => {
                self.note(error);
                return;
            }
        };
        let slot = Some(BandSlot {
            text: text.to_owned(),
            link: Some(target),
        });
        if left {
            self.left = slot;
        } else {
            self.right = slot;
        }
    }

    pub(crate) fn note(&mut self, error: Error) {
        if self.error.is_none() {
            self.error = Some(error);
        }
    }

    pub(crate) fn lower(self, style: &Style) -> Result<Vec<FlowNode>> {
        if let Some(error) = self.error {
            return Err(error);
        }
        let column = |slot: BandSlot, align: TextAlign| {
            let style = Style {
                align,
                ..style.clone()
            };
            let node = if slot.text.is_empty() {
                TextNode::from_items(Vec::new(), style)
            } else {
                TextNode::from_runs(
                    vec![TextRun::new(slot.text, Face::Body).with_link(slot.link)],
                    style,
                )
            };
            vec![FlowNode::Text(node)]
        };
        let even = ColumnWidth::Ratio(Ratio::new(1.0)?);
        let slots: Vec<_> = [(self.left, TextAlign::Start), (self.right, TextAlign::End)]
            .into_iter()
            .filter_map(|(text, align)| text.map(|text| (even, column(text, align))))
            .collect();
        if slots.is_empty() {
            return Ok(Vec::new());
        }
        let spacer = |pad: Length<NonNegative>| -> Result<Option<(ColumnWidth, Vec<FlowNode>)>> {
            if pad.as_pt() == 0.0 {
                return Ok(None);
            }
            Ok(Some((
                ColumnWidth::Fixed(Positive::pt(pad.as_pt())?),
                Vec::new(),
            )))
        };
        let mut tracks: Vec<_> = spacer(self.inset_left)?.into_iter().collect();
        tracks.extend(slots);
        tracks.extend(spacer(self.inset_right)?);
        if tracks.len() == 1 {
            return Ok(tracks.remove(0).1);
        }
        Ok(vec![FlowNode::Columns(ColumnsNode::new(
            NonNegative::ZERO,
            tracks,
        )?)])
    }
}

pub(crate) fn band_content(hook: BandHook<'_>, style: Style) -> BandContent<'_> {
    Box::new(move |info: &PageInfo| {
        let mut band = Band {
            page: Page::from(info),
            left: None,
            right: None,
            inset_left: NonNegative::ZERO,
            inset_right: NonNegative::ZERO,
            error: None,
        };
        hook(&mut band);
        band.lower(&style)
    })
}
