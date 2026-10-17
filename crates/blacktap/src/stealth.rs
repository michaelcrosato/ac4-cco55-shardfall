//! Land missions' vocabulary (Rounds 4–6): the town and its people, the courier, the stocks,
//! the fort's guards and the Commodore; perception, heat and posters. Filled in Phase 4; the
//! names below are what the Round files use.

use glam::Vec3;
use pav_core::entity::EntityId;
use pav_core::frame::SimEvent;
use pav_core::sim::Sim;
use pav_kit::combat::Assassination;

use crate::game::BlackTap;

/// Land actions (`rounds::act` hands over the names it doesn't know).
pub fn act(g: &mut BlackTap, _sim: &mut Sim, name: &str, _arg: Option<&str>) {
    match name {
        "spawn_town" | "spawn_courier" | "spawn_stocks_guards" | "spawn_fort_guards" | "drye_balcony" | "alarm" => {}
        "has_key" => g.mission.set_flag("has_key"),
        "last_words" => g.mission.set_flag("last_words_done"),
        other => log::warn!("blacktap: unknown mission action '{other}'"),
    }
}

/// Land conditions.
pub fn eval(_g: &BlackTap, _sim: &Sim, name: &str) -> bool {
    log::warn!("blacktap: unknown mission condition '{name}'");
    false
}

pub fn post(_g: &mut BlackTap, _sim: &mut Sim, _dt: f32, _events: &mut Vec<SimEvent>) {}

pub fn on_kill(g: &mut BlackTap, _sim: &mut Sim, _id: EntityId, _how: Option<Assassination>) {
    let _ = (g, Vec3::ZERO);
}

use pav_core::play::{CameraHint, DrawList, Overlay, UiModel};

/// A scripted scene's camera (Last Words, Fort Captured), if one plays.
pub fn cinematic_camera(_g: &BlackTap, _sim: &Sim) -> Option<CameraHint> {
    None
}

pub fn heat_level(_g: &BlackTap) -> u8 {
    0
}

/// HUD on land: prompts, awareness arcs.
pub fn ui(_g: &BlackTap, _sim: &Sim, _ui: &mut UiModel) {}

pub fn overlay(_g: &BlackTap) -> Option<Overlay> {
    None
}

pub fn letterbox(_g: &BlackTap) -> f32 {
    0.0
}

pub fn white(_g: &BlackTap) -> f32 {
    0.0
}

pub fn markers_for_tag(_g: &BlackTap, _sim: &Sim, _tag: &str, _ui: &mut UiModel) {}

pub fn markers_for_npc(_g: &BlackTap, _sim: &Sim, _name: &str, _ui: &mut UiModel) {}

/// Predicted arcs while aiming the broadside.
pub fn draw_arcs(g: &BlackTap, out: &mut DrawList) {
    let Some(s) = g.sea.my_ship() else { return };
    let c = g.content();
    let Some(cl) = c.classes.get(&s.class) else { return };
    let Some(w) = g.sea.weapon else { return };
    let dir = match w {
        pav_kit::naval::Weapon::Broadside(pav_kit::naval::Side::Port) => -s.right(),
        pav_kit::naval::Weapon::Broadside(pav_kit::naval::Side::Starboard) => s.right(),
        pav_kit::naval::Weapon::Chain => s.forward(),
        _ => return,
    };
    let el: f32 = 0.06;
    let from = s.world(Vec3::new(0.0, cl.freeboard * 0.6, 0.0)) + Vec3::new(dir.x, 0.0, dir.y) * cl.beam * 0.5;
    let vel = Vec3::new(dir.x * el.cos(), el.sin(), dir.y * el.cos()) * cl.muzzle;
    let pts = pav_kit::ballistics::arc(from, vel, 0.12, 3.0, 0.0);
    for p in pts.windows(2) {
        out.push(pav_core::play::DrawItem::Line { a: p[0], b: p[1], radius: 0.12, color: pav_core::Color::hex("#e2b04a"), emissive: 1.5 });
    }
    if let Some(last) = pts.last() {
        out.push(pav_core::play::DrawItem::Decal { center: Vec3::new(last.x, 0.1, last.z), radius: 5.0, ring: 0.85, color: pav_core::Color::hex("#e2b04a"), alpha: 0.9 });
    }
}

/// Land things to draw (awareness, ghosts, smoke...).
pub fn draw(_g: &BlackTap, _sim: &Sim, _out: &mut DrawList) {}

pub fn music_intensity(_g: &BlackTap) -> f32 {
    0.5
}

pub fn town_level(_g: &BlackTap, _sim: &Sim) -> f32 {
    0.0
}

pub fn jungle_level(_g: &BlackTap, _sim: &Sim) -> f32 {
    0.0
}

/// The bot on land (Phase 4 skills: swim ashore, climb routes, stealth lanes...).
pub fn bot(g: &BlackTap, sim: &Sim, _skill: &str) -> pav_core::input::InputFrame {
    if g.land.enemies_left() > 0 {
        return crate::bot::melee(g, sim);
    }
    pav_core::input::InputFrame::default()
}
