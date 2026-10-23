//! Stretch-slot table for [`crate::layout::style::TextAlign::Justify`].

use crate::geom::{Length, NonNegative, Positive};
use crate::layout::line_run::MeasuredInline;
use crate::text::break_class::is_justify_cluster;
use crate::text::line_break::Seg;

/// Parallel to per-cluster draws. Encodes what Justify can do at each index.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum DrawRole {
    /// Resolved blank. Never a stretch receiver; breaks phase-2 adjacency.
    Blank,
    /// Ink cluster whose first scalar is U+0020. Phase-1 only.
    Space {
        /// `0.5 * font_size` of this cluster's run.
        cap: Length<NonNegative>,
    },
    /// Any other ink cluster. `stretchable` ⇔ [`is_justify_cluster`].
    Ink { stretchable: bool },
}

/// One phase-1 receiver.
#[derive(Debug, Clone, Copy, PartialEq)]
struct SpaceSlot {
    draw_index: usize,
    cap: Length<NonNegative>,
}

/// One phase-2 receiver. Add goes onto the left cluster's draw.
#[derive(Debug, Clone, Copy, PartialEq)]
struct GapSlot {
    left_draw_index: usize,
}

/// Eligible stretch slots for one line, in left-to-right order.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct StretchInventory {
    spaces: Vec<SpaceSlot>,
    gaps: Vec<GapSlot>,
}

impl StretchInventory {
    /// Derive slots from the role tape. Pure. No remainder math.
    pub(crate) fn from_roles(roles: &[DrawRole]) -> Self {
        let mut spaces = Vec::new();
        let mut gaps = Vec::new();
        for (index, role) in roles.iter().enumerate() {
            if let DrawRole::Space { cap } = *role {
                spaces.push(SpaceSlot {
                    draw_index: index,
                    cap,
                });
            }
        }
        for index in 0..roles.len().saturating_sub(1) {
            let (
                DrawRole::Ink {
                    stretchable: left_ok,
                },
                DrawRole::Ink {
                    stretchable: right_ok,
                },
            ) = (roles[index], roles[index + 1])
            else {
                continue;
            };
            if left_ok || right_ok {
                gaps.push(GapSlot {
                    left_draw_index: index,
                });
            }
        }
        Self { spaces, gaps }
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.spaces.is_empty() && self.gaps.is_empty()
    }
}

/// Why a line stays Start-shaped under Justify.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LineInhibit {
    LastLine,
    HardBreak,
    Fill,
    NonPositiveRemainder,
    NoSlots,
}

/// §4.2 Start fallback as a type rather than scattered bools.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum JustifyGate {
    Blocked { reason: LineInhibit },
    Open(Length<Positive>),
}

impl JustifyGate {
    /// Cheap gates before inventory. `remainder_pt` is `frame_width - line_width`.
    pub(crate) fn before_slots(
        is_last_line: bool,
        line_ended_by_hard_break: bool,
        line_has_fill: bool,
        remainder_pt: f32,
    ) -> Self {
        if is_last_line {
            return Self::Blocked {
                reason: LineInhibit::LastLine,
            };
        }
        if line_ended_by_hard_break {
            return Self::Blocked {
                reason: LineInhibit::HardBreak,
            };
        }
        if line_has_fill {
            return Self::Blocked {
                reason: LineInhibit::Fill,
            };
        }
        if !(remainder_pt.is_finite() && remainder_pt > 0.0) {
            return Self::Blocked {
                reason: LineInhibit::NonPositiveRemainder,
            };
        }
        Self::Open(Length::from_trusted_pt(remainder_pt))
    }

    pub(crate) fn after_slots(self, inventory: &StretchInventory) -> Self {
        match self {
            Self::Open(_) if inventory.is_empty() => Self::Blocked {
                reason: LineInhibit::NoSlots,
            },
            other => other,
        }
    }
}

/// Per-draw increments over base `draw_widths`. Parallel to draws.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct StretchPlan {
    adds: Box<[Length<NonNegative>]>,
    /// Sum of adds. Painted width target = `line.width()` + allocated.
    allocated: Length<NonNegative>,
}

/// Inputs for [`StretchPlan::for_line`].
#[derive(Clone, Copy)]
pub(crate) struct StretchRequest<'a> {
    pub roles: &'a [DrawRole],
    pub frame_width: Length<Positive>,
    pub line_width: Length<NonNegative>,
    pub is_last_line: bool,
    pub ended_by_hard_break: bool,
    pub has_fill: bool,
}

impl StretchPlan {
    /// Deep entry: gate → inventory → distribute. One way to stretch a line.
    pub(crate) fn for_line(req: StretchRequest<'_>) -> Self {
        let remainder_pt = (req.frame_width - req.line_width).as_pt();
        let gate = JustifyGate::before_slots(
            req.is_last_line,
            req.ended_by_hard_break,
            req.has_fill,
            remainder_pt,
        );
        let JustifyGate::Open(remainder) = gate else {
            return Self::identity(req.roles.len());
        };
        let inventory = StretchInventory::from_roles(req.roles);
        let JustifyGate::Open(remainder) = JustifyGate::Open(remainder).after_slots(&inventory)
        else {
            return Self::identity(req.roles.len());
        };
        Self::distribute(&inventory, remainder, req.roles.len())
    }

    /// Water-fill spaces, then average gaps. Residual lands on the last slot that
    /// received a positive add across both phases.
    pub(crate) fn distribute(
        inventory: &StretchInventory,
        remainder: Length<Positive>,
        draw_count: usize,
    ) -> Self {
        let mut adds = vec![0.0f32; draw_count];
        let mut r = remainder.as_pt();
        let mut last_positive: Option<usize> = None;

        let caps: Vec<f32> = inventory
            .spaces
            .iter()
            .map(|slot| slot.cap.as_pt())
            .collect();
        let (space_adds, r2) = water_fill(&caps, r);
        for (slot, add) in inventory.spaces.iter().zip(&space_adds) {
            if *add > 0.0 {
                adds[slot.draw_index] += *add;
                last_positive = Some(slot.draw_index);
            }
        }
        r = r2;

        if !inventory.gaps.is_empty() && r > 0.0 {
            let m = inventory.gaps.len();
            // Slot counts on one line stay well inside f32's exact integer range.
            #[allow(clippy::cast_precision_loss)]
            let equal = r / m as f32;
            let mut allocated = 0.0f32;
            for gap in inventory.gaps.iter().take(m - 1) {
                adds[gap.left_draw_index] += equal;
                allocated += equal;
            }
            let last = inventory.gaps[m - 1].left_draw_index;
            let residual = r - allocated;
            adds[last] += residual;
        } else if r > 0.0 {
            // Phase-1 float dust only. Underfill past caps stays as trailing slack.
            if let Some(index) = last_positive {
                if let Some(slot) = inventory.spaces.iter().find(|s| s.draw_index == index) {
                    let room = (slot.cap.as_pt() - adds[index]).max(0.0);
                    adds[index] += r.min(room);
                }
            }
        }

        let allocated_pt: f32 = adds.iter().sum();
        Self {
            adds: adds
                .into_iter()
                .map(Length::from_trusted_pt)
                .collect::<Vec<_>>()
                .into_boxed_slice(),
            allocated: Length::from_trusted_pt(allocated_pt),
        }
    }

    pub(crate) fn identity(draw_count: usize) -> Self {
        Self {
            adds: vec![NonNegative::ZERO; draw_count].into_boxed_slice(),
            allocated: NonNegative::ZERO,
        }
    }

    pub(crate) fn apply(&self, base: &[Length<NonNegative>]) -> Vec<Length<NonNegative>> {
        debug_assert_eq!(
            self.adds.len(),
            base.len(),
            "stretch plan must hold one adjustment per base width"
        );
        base.iter()
            .zip(self.adds.iter())
            .map(|(width, add)| *width + *add)
            .collect()
    }

    pub(crate) fn allocated(&self) -> Length<NonNegative> {
        self.allocated
    }

    #[cfg(test)]
    pub(crate) fn add_at(&self, index: usize) -> Length<NonNegative> {
        self.adds[index]
    }

    #[cfg(test)]
    pub(crate) fn is_identity(&self) -> bool {
        self.allocated.as_pt() == 0.0
    }
}

/// Phase-1 water-fill: share remainder among spaces still under cap until R is
/// exhausted or every space is capped. Returns per-space adds and leftover R2.
///
/// Stops early when a round makes no progress (float guard).
pub(crate) fn water_fill(caps: &[f32], mut remainder: f32) -> (Vec<f32>, f32) {
    let mut adds = vec![0.0f32; caps.len()];
    if caps.is_empty() || !remainder.is_finite() || remainder <= 0.0 {
        return (adds, remainder.max(0.0));
    }
    loop {
        if !remainder.is_finite() || remainder <= 0.0 {
            break;
        }
        let open: Vec<usize> = adds
            .iter()
            .enumerate()
            .filter(|(index, add)| **add < caps[*index])
            .map(|(index, _)| index)
            .collect();
        if open.is_empty() {
            break;
        }
        // Open space count on one line stays well inside f32's exact integer range.
        #[allow(clippy::cast_precision_loss)]
        let share = remainder / open.len() as f32;
        let mut progressed = false;
        for index in open {
            let room = caps[index] - adds[index];
            let delta = share.min(room);
            if delta > 0.0 {
                adds[index] += delta;
                remainder -= delta;
                progressed = true;
            }
        }
        if !progressed {
            break;
        }
    }
    (adds, remainder.max(0.0))
}

/// Classify each draw from the same seg walk `place_line` uses.
pub(crate) fn build_roles<T>(groups: &[MeasuredInline], segs: &[Seg<T>]) -> Vec<DrawRole> {
    let mut roles = Vec::new();
    for seg in segs {
        match *seg {
            Seg::Blank { .. } => roles.push(DrawRole::Blank),
            Seg::Glyphs { group, start, end } => {
                let measured = &groups[group];
                let tape = measured.text.clustered();
                for cluster_index in start.cluster..end.cluster {
                    let cluster = tape.cluster(cluster_index);
                    let range = cluster.glyph_range();
                    if range.is_empty() {
                        continue;
                    }
                    let first = cluster
                        .as_str()
                        .chars()
                        .next()
                        .expect("non-empty cluster has a scalar");
                    let style = measured.style_of(range).expect("glyph range has a style");
                    if first == ' ' {
                        let cap = Length::from_trusted_pt(0.5 * style.font_size.as_pt());
                        roles.push(DrawRole::Space { cap });
                    } else {
                        roles.push(DrawRole::Ink {
                            stretchable: is_justify_cluster(first),
                        });
                    }
                }
            }
        }
    }
    roles
}

#[cfg(test)]
fn nn(pt: f32) -> Length<NonNegative> {
    NonNegative::pt(pt).unwrap()
}

#[cfg(test)]
fn pos(pt: f32) -> Length<Positive> {
    Positive::pt(pt).unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::canvas::pt_near;

    #[test]
    fn water_fill_equal_caps_match_one_shot() {
        let (adds, r2) = water_fill(&[5.0, 5.0, 5.0], 6.0);
        assert!(pt_near(adds[0], 2.0), "equal caps split 6.0 into 2.0 each");
        assert!(pt_near(adds[1], 2.0), "equal caps split 6.0 into 2.0 each");
        assert!(pt_near(adds[2], 2.0), "equal caps split 6.0 into 2.0 each");
        assert!(pt_near(r2, 0.0), "exact fill leaves no remainder");
    }

    #[test]
    fn water_fill_hits_caps_then_leaves_remainder() {
        let (adds, r2) = water_fill(&[1.0, 2.0], 10.0);
        assert!(pt_near(adds[0], 1.0));
        assert!(pt_near(adds[1], 2.0));
        assert!(pt_near(r2, 7.0));
    }

    #[test]
    fn water_fill_empty_passes_remainder() {
        let (adds, r2) = water_fill(&[], 4.0);
        assert!(adds.is_empty());
        assert!(pt_near(r2, 4.0));
    }

    #[test]
    fn inventory_space_not_ink_so_space_sides_skip_phase2() {
        let roles = [
            DrawRole::Ink { stretchable: true },
            DrawRole::Space { cap: nn(3.0) },
            DrawRole::Ink { stretchable: true },
        ];
        let inv = StretchInventory::from_roles(&roles);
        assert_eq!(inv.spaces.len(), 1);
        assert!(inv.gaps.is_empty());
    }

    #[test]
    fn inventory_cjk_latin_gap_and_latin_interior() {
        let roles = [
            DrawRole::Ink { stretchable: true },  // 好
            DrawRole::Ink { stretchable: false }, // A
            DrawRole::Ink { stretchable: false }, // B
        ];
        let inv = StretchInventory::from_roles(&roles);
        assert_eq!(inv.gaps.len(), 1);
        assert_eq!(inv.gaps[0].left_draw_index, 0);
    }

    #[test]
    fn inventory_blank_breaks_adjacency() {
        let roles = [
            DrawRole::Ink { stretchable: true },
            DrawRole::Blank,
            DrawRole::Ink { stretchable: true },
        ];
        let inv = StretchInventory::from_roles(&roles);
        assert!(inv.gaps.is_empty());
    }

    #[test]
    fn gate_blocks_last_hard_fill_and_non_positive() {
        assert_eq!(
            JustifyGate::before_slots(true, false, false, 10.0),
            JustifyGate::Blocked {
                reason: LineInhibit::LastLine
            }
        );
        assert_eq!(
            JustifyGate::before_slots(false, true, false, 10.0),
            JustifyGate::Blocked {
                reason: LineInhibit::HardBreak
            }
        );
        assert_eq!(
            JustifyGate::before_slots(false, false, true, 10.0),
            JustifyGate::Blocked {
                reason: LineInhibit::Fill
            }
        );
        assert_eq!(
            JustifyGate::before_slots(false, false, false, 0.0),
            JustifyGate::Blocked {
                reason: LineInhibit::NonPositiveRemainder
            }
        );
        assert!(matches!(
            JustifyGate::before_slots(false, false, false, 3.0),
            JustifyGate::Open(_)
        ));
    }

    #[test]
    fn gate_after_slots_blocks_empty_inventory() {
        let open = JustifyGate::Open(pos(5.0));
        let empty = StretchInventory {
            spaces: Vec::new(),
            gaps: Vec::new(),
        };
        assert_eq!(
            open.after_slots(&empty),
            JustifyGate::Blocked {
                reason: LineInhibit::NoSlots
            }
        );
    }

    #[test]
    fn residual_lands_on_last_positive_add_slot() {
        // Two equal spaces, remainder fully absorbed in phase 1 → last space that
        // took a positive add eats float dust if any; here exact.
        let inv = StretchInventory {
            spaces: vec![
                SpaceSlot {
                    draw_index: 0,
                    cap: nn(10.0),
                },
                SpaceSlot {
                    draw_index: 2,
                    cap: nn(10.0),
                },
            ],
            gaps: Vec::new(),
        };
        let plan = StretchPlan::distribute(&inv, pos(3.0), 3);
        assert!(pt_near(plan.add_at(0).as_pt(), 1.5));
        assert!(pt_near(plan.add_at(1).as_pt(), 0.0));
        assert!(pt_near(plan.add_at(2).as_pt(), 1.5));
        assert!(pt_near(plan.allocated().as_pt(), 3.0));
    }

    #[test]
    fn phase2_residual_on_last_gap() {
        let inv = StretchInventory {
            spaces: Vec::new(),
            gaps: vec![
                GapSlot { left_draw_index: 0 },
                GapSlot { left_draw_index: 1 },
                GapSlot { left_draw_index: 2 },
            ],
        };
        let plan = StretchPlan::distribute(&inv, pos(1.0), 4);
        let a0 = plan.add_at(0).as_pt();
        let a1 = plan.add_at(1).as_pt();
        let a2 = plan.add_at(2).as_pt();
        assert!(pt_near(a0, 1.0 / 3.0));
        assert!(pt_near(a1, 1.0 / 3.0));
        assert!(pt_near(a0 + a1 + a2, 1.0));
        assert!(pt_near(plan.allocated().as_pt(), 1.0));
    }

    #[test]
    fn for_line_identity_when_last_line() {
        let roles = [DrawRole::Ink { stretchable: true }; 3];
        let plan = StretchPlan::for_line(StretchRequest {
            roles: &roles,
            frame_width: pos(100.0),
            line_width: nn(50.0),
            is_last_line: true,
            ended_by_hard_break: false,
            has_fill: false,
        });
        assert!(plan.is_identity());
    }

    #[test]
    fn for_line_stretches_cjk_gaps() {
        let roles = [
            DrawRole::Ink { stretchable: true },
            DrawRole::Ink { stretchable: true },
            DrawRole::Ink { stretchable: true },
        ];
        let plan = StretchPlan::for_line(StretchRequest {
            roles: &roles,
            frame_width: pos(100.0),
            line_width: nn(70.0),
            is_last_line: false,
            ended_by_hard_break: false,
            has_fill: false,
        });
        assert!(pt_near(plan.allocated().as_pt(), 30.0));
        assert!(pt_near(
            plan.add_at(0).as_pt() + plan.add_at(1).as_pt(),
            30.0
        ));
        assert!(pt_near(plan.add_at(2).as_pt(), 0.0));
    }

    #[test]
    fn latin_interior_does_not_stretch() {
        let roles = [
            DrawRole::Ink { stretchable: false },
            DrawRole::Ink { stretchable: false },
            DrawRole::Ink { stretchable: false },
        ];
        let plan = StretchPlan::for_line(StretchRequest {
            roles: &roles,
            frame_width: pos(100.0),
            line_width: nn(40.0),
            is_last_line: false,
            ended_by_hard_break: false,
            has_fill: false,
        });
        assert!(plan.is_identity());
    }

    #[test]
    fn underfill_past_space_caps_leaves_trailing_slack() {
        let inv = StretchInventory {
            spaces: vec![SpaceSlot {
                draw_index: 0,
                cap: nn(1.0),
            }],
            gaps: Vec::new(),
        };
        let plan = StretchPlan::distribute(&inv, pos(5.0), 2);
        assert!(pt_near(plan.add_at(0).as_pt(), 1.0));
        assert!(pt_near(plan.allocated().as_pt(), 1.0));
    }

    #[test]
    fn residual_on_last_positive_add_across_both_phases() {
        // Phase 1 caps one space; phase 2 takes the rest. Last gap (index 2) is the
        // last slot that received a positive add and eats the residual.
        let inv = StretchInventory {
            spaces: vec![SpaceSlot {
                draw_index: 0,
                cap: nn(1.0),
            }],
            gaps: vec![
                GapSlot { left_draw_index: 1 },
                GapSlot { left_draw_index: 2 },
            ],
        };
        let plan = StretchPlan::distribute(&inv, pos(4.0), 4);
        assert!(pt_near(plan.add_at(0).as_pt(), 1.0));
        let gap_total = plan.add_at(1).as_pt() + plan.add_at(2).as_pt();
        assert!(pt_near(gap_total, 3.0));
        assert!(pt_near(plan.allocated().as_pt(), 4.0));
        assert!(plan.add_at(2).as_pt() > 0.0);
    }
}
