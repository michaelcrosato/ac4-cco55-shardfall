//! Crow's Perches (survey: a 360° pan that reveals the map around), the Leap of Spirits (an
//! assisted swan dive into hay or deep water) and the traversal camera contexts. Climbing,
//! free-running, ledges, beams and swimming are the engine's (`pav_core::character`).

use glam::{Vec2, Vec3};
use pav_core::character::{ScriptPose, Scripted};
use pav_core::entity::EntityId;
use pav_core::frame::SimEvent;
use pav_core::input::InputFrame;
use pav_core::play::{CameraHint, Prompt, UiModel};
use pav_core::sim::Sim;

use crate::game::{BlackTap, Body, b};
use crate::stealth::Leap;

/// Seconds the survey pan takes.
pub const SURVEY_SECONDS: f32 = 4.5;

/// The perch the player stands on, if any.
pub fn perch_here<'a>(g: &'a BlackTap, sim: &Sim) -> Option<crate::world::Perch> {
    let p = BlackTap::player_feet(sim)?;
    let w = g.world();
    w.perches.iter().find(|q| Vec2::new(q.at[0] - p.x, q.at[2] - p.z).length() < 2.6 && (q.at[1] - p.y).abs() < 2.0).cloned()
}

/// Perches, the survey and the leap: from `BlackTap::pre` while on foot. Returns an input
/// override for the player while a scripted move runs.
pub fn pre(
    g: &mut BlackTap,
    sim: &mut Sim,
    input: &InputFrame,
    playing: bool,
    dt: f32,
    events: &mut Vec<SimEvent>,
) -> Option<InputFrame> {
    let pid = sim.state.player?;
    // The leap in flight.
    if let Some(mut l) = g.stealth.leap.clone() {
        l.t += dt;
        let k = (l.t / l.len).clamp(0.0, 1.0);
        let arc = (k * std::f32::consts::PI).sin() * 2.5;
        let p = l.from.lerp(l.to, k * k * (3.0 - 2.0 * k) * 0.3 + k * 0.7) + Vec3::Y * arc;
        let d = l.to - l.from;
        if let Some(ch) = sim.state.entities.get_mut(pid).and_then(|e| e.character.as_mut()) {
            ch.scripted = Some(Scripted {
                feet: p,
                facing: d.x.atan2(d.z),
                vel: d / l.len,
                pose: if k < 0.92 { ScriptPose::Air } else { ScriptPose::Ground },
                climb_step: 0.0,
            });
        }
        if k >= 1.0 {
            g.stealth.leap = None;
            sim.set_position(pid, l.to + Vec3::Y * 0.1);
            g.mission.set_flag("leaped");
            events.push(SimEvent::cue("hay_land", l.to));
            g.show("leap_of_spirits");
            g.learned("leap");
        } else {
            g.stealth.leap = Some(l);
        }
        return Some(InputFrame::default());
    }
    // The survey pan.
    if let Some((name, t)) = g.stealth.survey.clone() {
        let t = t + dt;
        if t >= SURVEY_SECONDS {
            g.stealth.survey = None;
            g.mission.set_flag(&format!("surveyed_{name}"));
            g.stealth.revealed.insert(name.clone());
            let n = g.world().collectibles.iter().filter(|c| near_perch(g, &name, Vec3::from(c.at))).count();
            g.toast(&format!("Survey complete: {n} places revealed"));
            events.push(SimEvent::cue("survey_done", BlackTap::player_feet(sim).unwrap_or_default()));
            g.show("survey");
            g.learned("survey");
        } else {
            g.stealth.survey = Some((name, t));
        }
        return Some(InputFrame::default());
    }
    if !playing || g.body != Body::Foot {
        return None;
    }
    let perch = perch_here(g, sim)?;
    g.mission.set_flag(&format!("on_{}_perch", perch.name));
    if perch.name == "chapel" && !g.mission.flag("fast_climb") && g.stealth.ashore_at.is_some_and(|t| g.time - t < 60.0) {
        g.mission.set_flag("fast_climb");
    }
    let surveyed = g.mission.flag(&format!("surveyed_{}", perch.name)) || g.stealth.revealed.contains(&perch.name);
    if !surveyed && input.down(b::INTERACT) {
        g.stealth.survey = Some((perch.name.clone(), 0.0));
        events.push(SimEvent::cue("survey_start", Vec3::from(perch.at)));
        return Some(InputFrame::default());
    }
    if !perch.leap_to.is_empty() && input.just(b::JUMP) {
        let w = g.world();
        if let Some(to) = w.marker(&perch.leap_to) {
            let from = BlackTap::player_feet(sim).unwrap_or(Vec3::from(perch.at));
            let len = (1.0 + (from.y - to.y).max(0.0) / 14.0).clamp(1.0, 2.4);
            g.stealth.leap = Some(Leap { from, to, t: 0.0, len });
            events.push(SimEvent::cue("leap_whoosh", from));
            return Some(InputFrame::default());
        }
    }
    None
}

fn near_perch(g: &BlackTap, name: &str, p: Vec3) -> bool {
    g.world().perch(name).is_some_and(|q| Vec2::new(q.at[0] - p.x, q.at[2] - p.z).length() < q.reveals.max(150.0))
}

/// Prompts on a perch.
pub fn ui(g: &BlackTap, sim: &Sim, ui: &mut UiModel) {
    if g.body != Body::Foot || g.stealth.survey.is_some() || g.stealth.leap.is_some() {
        return;
    }
    let Some(perch) = perch_here(g, sim) else { return };
    if !g.mission.flag(&format!("surveyed_{}", perch.name)) {
        ui.prompts.push(Prompt { action: "interact".into(), label: "Survey".into(), hold: false });
    }
    if !perch.leap_to.is_empty() {
        ui.prompts.push(Prompt { action: "jump".into(), label: "Leap of Spirits".into(), hold: false });
    }
}

/// The survey's orbiting camera.
pub fn camera(g: &BlackTap, sim: &Sim) -> Option<CameraHint> {
    let (_, t) = g.stealth.survey.as_ref()?;
    let p = BlackTap::player_feet(sim)?;
    let k = t / SURVEY_SECONDS;
    let yaw = g.cam.yaw + k * 360.0;
    Some(CameraHint::orbit(p + Vec3::Y * 1.6, yaw, 8.0 - 6.0 * (k * std::f32::consts::PI).sin(), 5.5, 60.0))
}

/// A camera context traversal wants (climbing, swimming, free-running), if any.
pub fn camera_override(g: &BlackTap, sim: &Sim) -> Option<(&'static str, Vec3, Option<EntityId>)> {
    let pid = sim.state.player?;
    let ch = sim.state.entities.get(pid)?.character.as_ref()?;
    let feet = BlackTap::player_feet(sim)?;
    let ctx = if ch.swimming {
        "swim"
    } else if ch.hang.is_some() || ch.climbing.is_some() {
        "climb"
    } else if g.land.sprinting && Vec2::new(ch.vel.x, ch.vel.z).length() > 4.0 {
        "freerun"
    } else {
        return None;
    };
    Some((ctx, feet, Some(pid)))
}
