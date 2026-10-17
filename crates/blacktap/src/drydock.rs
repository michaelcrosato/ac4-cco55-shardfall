//! The Drydock: the developer level, a cay with a station per system (PLAN.md §9.19).
//! Its rules data is `games/blacktap/drydock.toml` (markers, people, routes like tab1.toml) and
//! its geometry `games/blacktap/drydock_world.toml`.

use std::collections::BTreeMap;

use pav_core::entity::EntityId;
use pav_core::frame::SimEvent;
use pav_core::input::InputFrame;
use pav_core::play::{DrawList, UiModel};
use pav_core::sim::Sim;
use serde::{Deserialize, Serialize};

use crate::game::BlackTap;

/// The Drydock's own state (part of the game state: snapshots and rewind include it).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Dock {
    /// The station the player stands at, if any.
    pub station: Option<String>,
}

pub fn setup(g: &mut BlackTap, _sim: &mut Sim) {
    g.set_atmosphere("afternoon", 0.0);
}

/// Every tick in the Drydock (after the player's own rules): pads, dummies, stations.
pub fn pre(
    _g: &mut BlackTap,
    _sim: &mut Sim,
    _input: &InputFrame,
    _dt: f32,
    _events: &mut Vec<SimEvent>,
) -> BTreeMap<EntityId, InputFrame> {
    BTreeMap::new()
}

pub fn post(_g: &mut BlackTap, _sim: &mut Sim, _dt: f32, _events: &mut Vec<SimEvent>) {}

/// Signs, pads and station props.
pub fn draw(_g: &BlackTap, _sim: &Sim, _out: &mut DrawList) {}

/// Station guide and prompts.
pub fn ui(_g: &BlackTap, _sim: &Sim, _ui: &mut UiModel) {}
