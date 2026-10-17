//! Free-running, climbing, ledges, beams, perches and the Leap of Spirits (Phase 4).

use glam::Vec3;
use pav_core::entity::EntityId;
use pav_core::sim::Sim;

use crate::game::BlackTap;

/// A camera context traversal wants (climbing, the survey orbit), if any.
pub fn camera_override(_g: &BlackTap, _sim: &Sim) -> Option<(&'static str, Vec3, Option<EntityId>)> {
    None
}
