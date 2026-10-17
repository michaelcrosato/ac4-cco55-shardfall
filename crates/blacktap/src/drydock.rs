//! The Drydock: the developer level, a cay with a station per system (PLAN.md §9.19).

use pav_core::sim::Sim;

use crate::game::BlackTap;

pub fn setup(g: &mut BlackTap, _sim: &mut Sim) {
    g.set_atmosphere("afternoon", 0.0);
}
