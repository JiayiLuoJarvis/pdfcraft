//! Anchor registration table and link resolution.

use std::collections::HashMap;

use crate::canvas::{LinkTarget, LinkXyz, OutlineItem};
use crate::error::{Error, ErrorKind, Fallible as Result, Location};
use crate::geom::Point;
use crate::layout::frame::FragmentCommand;
use crate::layout::paginate::LaidDocument;

/// Outline candidate recorded at lower time.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ExpectedOutline {
    pub(crate) title: String,
    pub(crate) level: u8,
}

/// Anchor declared by lower, in document order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ExpectedAnchor {
    pub(crate) id: String,
    pub(crate) outline: Option<ExpectedOutline>,
    pub(crate) location: Location,
    pub(crate) order: u32,
}

/// First-wins map from anchor id to resolved page destination.
#[derive(Debug, Default, Clone)]
pub(crate) struct AnchorTable {
    by_id: HashMap<String, LinkXyz>,
}

impl AnchorTable {
    pub(crate) fn get(&self, id: &str) -> Option<LinkXyz> {
        self.by_id.get(id).copied()
    }

    fn insert_first(&mut self, id: &str, xyz: LinkXyz) {
        self.by_id.entry(id.to_owned()).or_insert(xyz);
    }
}

/// Walk body-placed fragments and record first-wins destinations.
pub(crate) fn collect_anchors(laid: &LaidDocument) -> AnchorTable {
    let mut table = AnchorTable::default();
    for (page_index, page) in laid.pages().iter().enumerate() {
        for placed in &page.placed {
            for mark in &placed.fragment.anchors {
                let point = Point::try_pt(
                    placed.origin.x().as_pt() + mark.relative.x().as_pt(),
                    placed.origin.y().as_pt() + mark.relative.y().as_pt(),
                )
                .unwrap_or(placed.origin);
                table.insert_first(mark.id.as_str(), LinkXyz::new(page_index, point));
            }
        }
    }
    table
}

/// Rewrite `LinkTarget::Anchor` commands to `Xyz`. Missing ids → [`ErrorKind::UnknownAnchor`].
pub(crate) fn resolve_link_commands(laid: &mut LaidDocument, table: &AnchorTable) -> Result<()> {
    for (page_index, page) in laid.pages_mut().iter_mut().enumerate() {
        let page_at = Location::page_index(page_index).with_region(crate::error::Region::Body);
        for placed in &mut page.placed {
            for command in &mut placed.fragment.commands {
                let FragmentCommand::Link { target, at, .. } = command else {
                    continue;
                };
                let LinkTarget::Anchor(id) = target else {
                    continue;
                };
                let Some(xyz) = table.get(id) else {
                    let mut error = Error::from(ErrorKind::UnknownAnchor);
                    if *at == Location::default() {
                        error.fill_missing(&page_at);
                    } else {
                        error.fill_missing(at);
                    }
                    return Err(error);
                };
                *target = LinkTarget::xyz(xyz.page_index(), xyz.point());
            }
        }
    }
    Ok(())
}

fn located(kind: ErrorKind, at: &Location) -> Error {
    let mut error = Error::from(kind);
    error.fill_missing(at);
    error
}

/// Report never-placed ids and build the flat outline list.
pub(crate) fn finish_anchors(
    expected: &[ExpectedAnchor],
    table: &AnchorTable,
) -> Result<Vec<OutlineItem>> {
    let mut outline = Vec::new();
    let mut prev_level: Option<u8> = None;
    for entry in expected {
        let Some(xyz) = table.get(&entry.id) else {
            return Err(located(ErrorKind::AnchorWithoutFragment, &entry.location));
        };
        let Some(item) = &entry.outline else {
            continue;
        };
        if item.title.is_empty() {
            return Err(located(ErrorKind::EmptyOutlineTitle, &entry.location));
        }
        if item.level == 0 {
            return Err(located(ErrorKind::InvalidOutlineLevel, &entry.location));
        }
        match prev_level {
            None if item.level != 1 => {
                return Err(located(ErrorKind::InvalidOutlineLevel, &entry.location));
            }
            Some(prev) if item.level > prev.saturating_add(1) => {
                return Err(located(ErrorKind::InvalidOutlineLevel, &entry.location));
            }
            _ => {}
        }
        prev_level = Some(item.level);
        outline.push(OutlineItem {
            title: item.title.clone(),
            level: item.level,
            xyz,
        });
    }
    Ok(outline)
}
