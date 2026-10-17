//! What the player sees and hears: the HUD model, the draw list (ships, sails, shot, telegraph
//! rings, floats, spouts, the islands while they are shapes), the camera, the atmosphere, the
//! music and the feel.

use std::sync::Arc;

use glam::{Quat, Vec2, Vec3};
use pav_core::entity::EntityId;
use pav_core::input::InputFrame;
use pav_core::play::{
    Atmosphere, AudioState, CameraHint, Compass, CompassMark, DemoState, DrawItem, DrawList, FrameEffects, HudItem, MarkerKind,
    MusicCue, Objective, Overlay, Prompt, Results, ScreenFx, Tile, UiModel, WeaponPip, WorldMarker,
};
use pav_core::sim::Sim;
use pav_core::{Color, Look, Shape};
use pav_kit::naval::{Side, Weapon};

use crate::game::{BlackTap, Body, Mode, b};

/// The camera context for now, the point it frames and what the spring arm ignores.
pub fn camera_context(g: &BlackTap, sim: &Sim, input: &InputFrame) -> (&'static str, Vec3, Option<EntityId>) {
    let feet = BlackTap::player_feet(sim).unwrap_or_default();
    if g.mode == Mode::Title {
        let at = g.sea.my_ship().map(|s| s.world(Vec3::Y * 4.0)).unwrap_or_default();
        return ("title", at, None);
    }
    if let Some(bd) = &g.land.boarding {
        if bd.phase == crate::foot::BoardPhase::Swing {
            let at = g.sea.my_ship().map(|s| s.world(Vec3::Y * 3.0)).unwrap_or_default();
            return ("helm", at, sim.state.player);
        }
        return ("combat", feet, sim.state.player);
    }
    match g.body {
        Body::Helm => {
            let at = g.sea.my_ship().map(|s| s.world(Vec3::Y * 2.0)).unwrap_or_default();
            if input.down(b::TOOLS) {
                ("spyglass", g.sea.my_ship().map(|s| s.world(Vec3::new(0.0, 4.0, -8.0))).unwrap_or(at), None)
            } else if input.down(b::AIM) || input.triggers[0] > 0.3 {
                ("broadside", at, None)
            } else {
                ("helm", at, None)
            }
        }
        Body::Foot => {
            let swimming = sim
                .state
                .player
                .and_then(|id| sim.state.entities.get(id))
                .and_then(|e| e.character.as_ref())
                .is_some_and(|c| c.swimming);
            let ctx = if g.land.enemies_left() > 0 {
                "combat"
            } else if swimming {
                "swim"
            } else if input.down(b::AIM) {
                "aim"
            } else if input.down(b::CROUCH) {
                "crouch"
            } else if input.down(b::SPRINT) {
                "freerun"
            } else {
                "foot"
            };
            crate::traverse::camera_override(g, sim).unwrap_or((ctx, feet, sim.state.player))
        }
    }
}

pub fn camera(g: &BlackTap, sim: &Sim) -> Option<CameraHint> {
    if let Some(h) = crate::stealth::cinematic_camera(g, sim).or_else(|| crate::traverse::camera(g, sim)) {
        return Some(h);
    }
    if g.mode == Mode::Title {
        let at = g.sea.my_ship().map(|s| s.world(Vec3::Y * 6.0)).unwrap_or_default();
        let mut h = pav_kit::camera::orbit_shot(at, 46.0, 7.0, 4.0, g.time, 50.0);
        h.far = 1800.0;
        return Some(h);
    }
    let sea = g.ocean();
    let surface = move |p: Vec2| sea.height(p);
    Some(g.cam.peek(Some(&surface)))
}

fn hex(c: &str) -> Color {
    Color::hex(c)
}

pub fn ui(g: &BlackTap, sim: &Sim) -> UiModel {
    let mut ui = UiModel::default();
    let c = g.content();
    let w = g.world();
    ui.demo = g.showcase.as_ref().map(|s| s.state()).map(|mut d: DemoState| {
        if d.caption.is_empty() {
            d.caption = "Assassin's Keg".into();
        }
        d
    });
    if g.mode == Mode::Title {
        return ui;
    }
    // Objective (top right) and the Full Pour checklist.
    if let Some(def) = c.missions.get(g.round as usize - 1) {
        if g.mode == Mode::Tab {
            let step = g.mission.current(def);
            ui.objective = Some(Objective {
                title: def.title.clone(),
                text: step.map(|s| s.objective.clone()).unwrap_or_else(|| "Round complete".into()),
                optional: g.mission.checklist(def),
                age: g.mission.step_time,
            });
            // Markers named by the step.
            for m in step.map(|s| s.markers.clone()).unwrap_or_default() {
                markers_for(g, sim, &w, &m, &mut ui);
            }
        }
    }
    // Health and heat (top left).
    let heat = crate::stealth::heat_level(g);
    if g.body == Body::Foot || g.land.hero.vitals.frac() < 1.0 || heat > 0 {
        ui.hud.push(HudItem::Health { frac: g.land.hero.vitals.frac(), segments: 8, heat });
    }
    // At sea: the ship (bottom left).
    if g.body == Body::Helm {
        if let Some(s) = g.sea.my_ship() {
            let cl = c.classes.get(&s.class).cloned().unwrap_or_default();
            let wind_rel = pav_kit::naval::turn_to(s.heading, pav_kit::naval::heading_of(-g.sea.wind));
            let active = g.sea.weapon;
            let pip = |w: Weapon| WeaponPip { name: w.name().into(), ready: s.ready(&cl, w), active: active == Some(w) };
            ui.hud.push(HudItem::Ship {
                hull: s.frac(),
                sails: s.sails,
                wind_rel,
                wind: (g.sea.wind.length() / 12.0).clamp(0.0, 1.0),
                sail: s.sail,
                knots: s.speed * 1.944,
                weapons: vec![
                    pip(Weapon::Broadside(Side::Port)),
                    pip(Weapon::Broadside(Side::Starboard)),
                    pip(Weapon::Chain),
                    pip(Weapon::Kegs),
                    pip(Weapon::Swivel),
                ],
            });
            ui.hud.push(HudItem::Counter { icon: "timber".into(), label: "Timber".into(), value: g.sea.timber.to_string() });
        }
        // Enemy ships and batteries get bars.
        for s in
            g.sea.ships.iter().filter(|s| s.id != g.sea.mine && s.alive() && s.faction != pav_kit::naval::Faction::Brotherhood)
        {
            let d = g.sea.my_ship().map(|m| m.pos.distance(s.pos)).unwrap_or(0.0);
            if d < 420.0 {
                ui.markers.push(WorldMarker {
                    pos: s.world(Vec3::Y * 22.0),
                    kind: if s.disabled { MarkerKind::Target } else { MarkerKind::Enemy },
                    label: s.name.clone(),
                    bar: Some(s.frac()),
                    distance: true,
                });
            }
        }
        for bt in g.sea.batteries.iter().filter(|b| !b.destroyed) {
            ui.markers.push(WorldMarker {
                pos: bt.pos + Vec3::Y * 12.0,
                kind: MarkerKind::Enemy,
                label: bt.name.clone(),
                bar: Some(bt.hp / bt.hp_max),
                distance: true,
            });
        }
        for sh in &g.sea.shells {
            ui.markers.push(WorldMarker {
                pos: sh.at,
                kind: MarkerKind::Threat,
                label: String::new(),
                bar: None,
                distance: false,
            });
        }
        // Prompts.
        if crate::rounds::boardable(g).is_some() {
            ui.prompts.push(Prompt::new("jump", "Board"));
        }
        if g.sea.my_ship().is_some_and(|s| s.speed.abs() <= 2.5) && g.mode != Mode::Title {
            ui.prompts.push(Prompt { action: "interact".into(), label: "Leave the helm".into(), hold: true });
        } else if g.sea.my_ship().is_some_and(|s| s.brace_cd <= 0.0) && g.sea.ships.iter().any(|s| s.telegraph > 0.0) {
            ui.prompts.push(Prompt::new("interact", "Brace!"));
        }
        if g.sea.swivel_target.is_some() {
            ui.prompts.push(Prompt::new("attack", "Swivel gun"));
        }
    } else {
        crate::traverse::ui(g, sim, &mut ui);
        crate::stealth::ui(g, sim, &mut ui);
    }
    // Onboarding: one big prompt for the action being taught.
    if let Some(t) = &g.teaching {
        if let Some((action, label)) = teach_prompt(t) {
            ui.prompts.insert(0, Prompt::new(action, label));
        }
    }
    ui.prompts.truncate(3);
    // Compass.
    let mut compass = Compass { heading: g.cam.yaw, marks: Vec::new() };
    let here = BlackTap::player_feet(sim).map(|p| Vec2::new(p.x, p.z)).unwrap_or_default();
    for m in &ui.markers {
        let d = Vec2::new(m.pos.x, m.pos.z) - here;
        compass.marks.push(CompassMark { bearing: pav_kit::naval::heading_of(d), kind: m.kind, distance: d.length() });
    }
    ui.compass = Some(compass);
    // Subtitles, banner, toasts.
    if g.options.subtitles {
        ui.subtitles = g.dialog.subtitles(&c.dialog);
    }
    ui.banner = g.banner.clone();
    ui.toasts = g.toasts.clone();
    // Overlays.
    if let Some(o) = crate::foot::overlay(g) {
        ui.overlay = Some(o);
    }
    if let Some(focus) = g.results {
        ui.overlay = Some(Overlay::Results(results(g, focus)));
    }
    if let Some(o) = crate::stealth::overlay(g) {
        ui.overlay = Some(o);
    }
    // Full screen.
    let hp = g.land.hero.vitals.frac();
    ui.screen = ScreenFx {
        letterbox: crate::stealth::letterbox(g),
        vignette: (1.0 - g.land.hero.vitals.since_hit.min(1.0)) * 0.6,
        desaturate: if hp < 0.3 { (0.3 - hp) * 2.0 } else { 0.0 } + if g.gull { 0.6 } else { 0.0 },
        fade: g.spilled.map(|t| (1.0 - t / 2.5).clamp(0.0, 1.0)).unwrap_or(0.0).max((g.stealth.tunnel_fade / 0.6).min(1.0)),
        white: crate::stealth::white(g),
    };
    dedupe_markers(&mut ui.markers);
    ui.map = Some(crate::map::map(g));
    ui.map_icons = crate::map::icons(g, sim);
    ui
}

/// One marker per thing: an objective marker takes over the enemy marker on the same ship or
/// battery (its health bar and name), so labels never stack.
fn dedupe_markers(m: &mut Vec<WorldMarker>) {
    let mut drop = vec![false; m.len()];
    for i in 0..m.len() {
        if !matches!(m[i].kind, MarkerKind::Objective | MarkerKind::Target) {
            continue;
        }
        for j in 0..m.len() {
            if i != j && !drop[j] && m[j].kind == MarkerKind::Enemy && m[j].pos.distance(m[i].pos) < 14.0 {
                let (bar, label, pos) = (m[j].bar, m[j].label.clone(), m[j].pos);
                m[i].bar = m[i].bar.or(bar);
                if m[i].label.is_empty() {
                    m[i].label = label;
                }
                m[i].pos = pos;
                drop[j] = true;
            }
        }
    }
    let mut k = 0;
    m.retain(|_| {
        k += 1;
        !drop[k - 1]
    });
}

fn results(g: &BlackTap, focus: usize) -> Results {
    let s = &g.stats;
    let mins = (s.time / 60.0) as u32;
    let secs = (s.time % 60.0) as u32;
    let c = g.content();
    let checklist: Vec<(String, bool)> =
        c.missions.iter().flat_map(|m| m.optional.iter().map(|o| (o.text.clone(), false))).collect();
    Results {
        title: "TAB I — THE DRY RUN".into(),
        pour: crate::rounds::pour(g),
        checklist: if checklist.len() > 8 { checklist[..8].to_vec() } else { checklist },
        stats: vec![
            ("Time".into(), format!("{mins}:{secs:02}")),
            ("Reales".into(), s.reales.to_string()),
            ("Collectibles".into(), format!("{} / 26", s.collectibles)),
            ("Assassinations".into(), s.assassinations.to_string()),
            ("Ships sunk".into(), s.ships_sunk.to_string()),
            ("Times detected".into(), s.detected.to_string()),
        ],
        tiles: vec![
            Tile {
                name: "Continue".into(),
                desc: "Free roam the isles".into(),
                cost: String::new(),
                icon: "sail".into(),
                disabled: false,
            },
            Tile {
                name: "Replay a Round".into(),
                desc: "Chase a Full Pour".into(),
                cost: String::new(),
                icon: "repair".into(),
                disabled: false,
            },
            Tile {
                name: "Title".into(),
                desc: "Back to the title".into(),
                cost: String::new(),
                icon: "keg".into(),
                disabled: false,
            },
        ],
        focus,
    }
}

fn teach_prompt(what: &str) -> Option<(&'static str, &'static str)> {
    Some(match what {
        "sails" => ("move_up", "Raise the sails"),
        "steer" => ("move_side", "Steer"),
        "spyglass" => ("tools", "Spyglass (hold), fire to tag"),
        "broadside" => ("fire", "Fire the guns you look at"),
        "brace" => ("interact", "Brace when gunports glow"),
        "board" => ("jump", "Board"),
        "counter" => ("interact", "Counter on the glint"),
        "leave_helm" => ("interact", "Hold to leave the helm"),
        "climb" => ("jump", "Free-run + jump: climb"),
        "survey" => ("interact", "Survey"),
        "leap" => ("jump", "Leap of Spirits"),
        "gull" => ("gull", "Gull Sight"),
        "assassinate" => ("attack", "Assassinate"),
        "whistle" => ("whistle", "Whistle"),
        _ => return None,
    })
}

fn markers_for(g: &BlackTap, sim: &Sim, w: &crate::world::World, m: &str, ui: &mut UiModel) {
    let (kind, name) = m.split_once(':').unwrap_or((m, ""));
    match (kind, name) {
        ("flotsam", _) => {
            for f in g.sea.floating.iter().filter(|f| f.kind == crate::sea::FloatKind::Flotsam) {
                ui.markers.push(WorldMarker {
                    pos: Vec3::new(f.pos.x, 2.0, f.pos.y),
                    kind: MarkerKind::Objective,
                    label: "Flotsam".into(),
                    bar: None,
                    distance: true,
                });
            }
        }
        ("ship", tag) => {
            for s in g.sea.by_tag(tag).filter(|s| s.alive()) {
                ui.markers.push(WorldMarker {
                    pos: s.world(Vec3::Y * 24.0),
                    kind: MarkerKind::Objective,
                    label: s.name.clone(),
                    bar: Some(s.frac()),
                    distance: true,
                });
            }
        }
        ("tag", tag) => {
            for s in g.sea.by_tag(tag).filter(|s| s.alive()) {
                ui.markers.push(WorldMarker {
                    pos: s.world(Vec3::Y * 14.0),
                    kind: MarkerKind::Objective,
                    label: s.name.clone(),
                    bar: Some(s.frac()),
                    distance: true,
                });
            }
            crate::stealth::markers_for_tag(g, sim, tag, ui);
        }
        ("battery", name) => {
            if let Some(bt) = g.sea.batteries.iter().find(|b| b.name == name && !b.destroyed) {
                ui.markers.push(WorldMarker {
                    pos: bt.pos + Vec3::Y * 14.0,
                    kind: MarkerKind::Objective,
                    label: format!("{} battery", bt.name),
                    bar: Some(bt.hp / bt.hp_max),
                    distance: true,
                });
            }
        }
        ("marker", name) => {
            if let Some(p) = w.marker(name) {
                ui.markers.push(WorldMarker {
                    pos: p + Vec3::Y * 2.5,
                    kind: MarkerKind::Objective,
                    label: String::new(),
                    bar: None,
                    distance: true,
                });
            }
        }
        ("npc", name) => crate::stealth::markers_for_npc(g, sim, name, ui),
        _ => {}
    }
}

/// Hull meshes by class, style and damage step (cached).
fn hull(
    class_key: &str,
    style_key: &str,
    damage_step: u32,
    seed: u32,
    c: &crate::content::Content,
) -> Option<Arc<pav_core::play::Mesh>> {
    use std::collections::BTreeMap;
    use std::sync::Mutex;
    static CACHE: Mutex<BTreeMap<(String, String, u32, u32), Arc<pav_core::play::Mesh>>> = Mutex::new(BTreeMap::new());
    let key = (class_key.to_string(), style_key.to_string(), damage_step, seed);
    let mut cache = CACHE.lock().unwrap_or_else(|p| p.into_inner());
    if let Some(m) = cache.get(&key) {
        return Some(m.clone());
    }
    let class = c.classes.get(class_key)?;
    let style = c.styles.get(style_key).cloned().unwrap_or_default();
    let id = {
        let mut h: u64 = 0x5417_0000_0000_0000;
        for b in format!("{class_key}/{style_key}/{damage_step}/{seed}").bytes() {
            h ^= b as u64;
            h = h.wrapping_mul(0x0100_0000_01b3);
        }
        h
    };
    let m = Arc::new(pav_kit::shipyard::hull_mesh(class, &style, id, damage_step as f32 / 4.0, seed));
    cache.insert(key, m.clone());
    Some(m)
}

fn style_for(tag: &str) -> &'static str {
    match tag {
        "black_tap" => "black_tap",
        "gunboat" | "restraint" => "temperance",
        "frigate" => "temperance_square",
        _ => "merchant",
    }
}

pub fn draw(g: &BlackTap, sim: &Sim, out: &mut DrawList) {
    let c = g.content();
    let w = g.world();
    // The islands while they are shapes (the terrain generator replaces this).
    if !crate::island::has_terrain(sim) {
        for (i, s) in w.shapes.iter().enumerate() {
            out.push(DrawItem::Mesh {
                mesh: crate::island::shape_mesh(s, i as u64),
                pos: Vec3::ZERO,
                rot: Quat::IDENTITY,
                scale: Vec3::ONE,
                color: Color::WHITE,
                emissive: 0.0,
            });
        }
    }
    // Ships.
    for s in &g.sea.ships {
        if s.sunk {
            continue;
        }
        let Some(cl) = c.classes.get(&s.class) else { continue };
        let style_key = style_for(&s.tag);
        let style = c.styles.get(style_key).cloned().unwrap_or_default();
        let step = ((1.0 - s.frac()) * 4.0).floor().min(3.0) as u32;
        let Some(h) = hull(&s.class, style_key, step, s.id, &c) else { continue };
        let tilt = Quat::from_rotation_y(std::f32::consts::PI - s.heading.to_radians())
            * Quat::from_rotation_x(-s.pitch.to_radians())
            * Quat::from_rotation_z(-s.roll.to_radians());
        let sink = s.sinking;
        let rot = tilt * Quat::from_rotation_x(sink * 0.35);
        let pos = s.world(Vec3::ZERO);
        out.push(DrawItem::Mesh { mesh: h, pos, rot, scale: Vec3::ONE, color: Color::WHITE, emissive: 0.0 });
        let wind_rel = pav_kit::naval::turn_to(s.heading, pav_kit::naval::heading_of(-g.sea.wind));
        let emblem = s.tag == "black_tap";
        let sails =
            pav_kit::shipyard::sails_mesh(cl, &style, s.sail_shown, wind_rel, g.time + s.id as f32, 1.0 - s.sails, emblem);
        out.push(DrawItem::Dynamic { mesh: sails, pos, rot, color: Color::WHITE, two_sided: true });
        let flag = if s.faction == pav_kit::naval::Faction::Brotherhood { [0.05, 0.05, 0.05] } else { Color::hex("#e8e8ee").0 };
        let pennant = pav_kit::shipyard::pennant_mesh(cl, flag, wind_rel, g.time + s.id as f32 * 0.37);
        out.push(DrawItem::Dynamic { mesh: pennant, pos, rot, color: Color::WHITE, two_sided: true });
        // Gunports glow before a volley (the brace cue).
        if s.telegraph > 0.0 && s.alive() {
            let side = if s.telegraph_side == Side::Starboard { 1.0 } else { -1.0 };
            let n = cl.guns_per_side.max(1);
            for i in 0..n {
                let along = (i as f32 / (n.max(2) - 1) as f32 - 0.5) * cl.length * 0.56;
                let p = s.world(Vec3::new(side * (cl.beam * 0.5 + 0.1), cl.freeboard * 0.6, along));
                out.push(DrawItem::Prim {
                    shape: Shape::Box { half: Vec3::new(0.12, 0.25, 0.3) },
                    pos: p,
                    rot,
                    color: hex("#ff9a2e"),
                    emissive: 3.0,
                    look: Look::Unlit,
                });
            }
            out.push(DrawItem::Light {
                pos: s.world(Vec3::new(side * cl.beam * 0.6, cl.freeboard, 0.0)),
                color: hex("#ff9a2e"),
                intensity: 3.0,
                radius: 14.0,
            });
        }
        // Fire and smoke at half hull.
        if s.fire > 0.0 && !sink.is_nan() {
            let p = s.world(Vec3::new(0.0, cl.freeboard + 0.5, cl.length * 0.1));
            out.push(DrawItem::Light { pos: p + Vec3::Y, color: hex("#ff7a2a"), intensity: 2.5, radius: 12.0 });
        }
        // The wake is foam particles from the `wake_foam` cue (game post).
    }
    // Batteries: stone emplacements with guns (until the fort is built from structures).
    for bt in &g.sea.batteries {
        let col = if bt.destroyed { hex("#4a4540") } else { hex("#cbbd9b") };
        out.push(DrawItem::Prim {
            shape: Shape::Box { half: bt.half },
            pos: bt.pos - Vec3::Y * bt.half.y * 0.5,
            rot: Quat::IDENTITY,
            color: col,
            emissive: 0.0,
            look: Look::Lit,
        });
        if bt.telegraph > 0.0 && !bt.destroyed {
            out.push(DrawItem::Light { pos: bt.pos + Vec3::Y * 2.0, color: hex("#ff9a2e"), intensity: 4.0, radius: 20.0 });
        }
    }
    // Mortar rings: red, filling as the shell falls.
    for sh in &g.sea.shells {
        let y = g.ocean().height(Vec2::new(sh.at.x, sh.at.z)) + 0.1;
        let t = (1.0 - sh.left / 2.2).clamp(0.0, 1.0);
        out.push(DrawItem::Decal {
            center: Vec3::new(sh.at.x, y, sh.at.z),
            radius: sh.radius,
            ring: 0.9,
            color: hex("#e0281e"),
            alpha: 1.0,
        });
        out.push(DrawItem::Decal {
            center: Vec3::new(sh.at.x, y + 0.02, sh.at.z),
            radius: sh.radius * t,
            ring: 0.0,
            color: hex("#a8322d"),
            alpha: 0.5,
        });
    }
    // Shot in flight.
    for s in g.sea.shots.shots.iter().filter(|s| s.delay <= 0.0) {
        let r = match s.kind {
            pav_kit::ballistics::ShotKind::Mortar => 0.45,
            pav_kit::ballistics::ShotKind::Chain => 0.3,
            _ => 0.22,
        };
        out.push(DrawItem::Prim {
            shape: Shape::Sphere { radius: r },
            pos: s.pos,
            rot: Quat::IDENTITY,
            color: hex("#1a1a1a"),
            emissive: 0.0,
            look: Look::Lit,
        });
    }
    // Floating crates and powder kegs.
    let ocean = g.ocean();
    for f in &g.sea.floating {
        let y = ocean.height(f.pos);
        let p = Vec3::new(f.pos.x, y + 0.2, f.pos.y);
        match f.kind {
            crate::sea::FloatKind::Flotsam => {
                out.push(DrawItem::Prim {
                    shape: Shape::Box { half: Vec3::new(0.7, 0.45, 0.6) },
                    pos: p,
                    rot: Quat::from_rotation_y(f.id as f32),
                    color: hex("#8b6b4a"),
                    emissive: 0.0,
                    look: Look::Lit,
                });
                out.push(DrawItem::Prim {
                    shape: Shape::Box { half: Vec3::new(0.5, 0.3, 0.5) },
                    pos: p + Vec3::new(0.9, -0.1, 0.4),
                    rot: Quat::from_rotation_y(f.id as f32 * 2.0),
                    color: hex("#6b4a2f"),
                    emissive: 0.0,
                    look: Look::Lit,
                });
                out.push(DrawItem::Decal {
                    center: Vec3::new(p.x, y + 0.05, p.z),
                    radius: 3.0 + (g.time * 2.0).sin() * 0.3,
                    ring: 0.85,
                    color: hex("#e2b04a"),
                    alpha: 0.7,
                });
            }
            crate::sea::FloatKind::Keg { .. } => {
                out.push(DrawItem::Prim {
                    shape: Shape::Cylinder { half_height: 0.45, radius: 0.35 },
                    pos: p,
                    rot: Quat::from_rotation_z(1.2),
                    color: hex("#6b4a2f"),
                    emissive: 0.0,
                    look: Look::Lit,
                });
                if (g.time * 4.0).sin() > 0.0 {
                    out.push(DrawItem::Light { pos: p + Vec3::Y * 0.6, color: hex("#ff5a2a"), intensity: 1.5, radius: 4.0 });
                }
            }
        }
    }
    // Waterspouts: a twisting column.
    for sp in &g.sea.spouts {
        for k in 0..10 {
            let h = k as f32 * 4.0;
            let a = g.time * 3.0 + k as f32 * 0.7;
            let r = sp.radius * (0.25 + k as f32 * 0.05);
            let p = Vec3::new(sp.pos.x + a.cos() * r * 0.3, h, sp.pos.y + a.sin() * r * 0.3);
            out.push(DrawItem::Prim {
                shape: Shape::Cylinder { half_height: 2.2, radius: r * 0.35 },
                pos: p,
                rot: Quat::from_rotation_y(a),
                color: hex("#9aa6ae"),
                emissive: 0.0,
                look: Look::Lit,
            });
        }
    }
    // Aim arcs while aiming broadsides.
    if g.body == Body::Helm && g.sea.aiming {
        crate::stealth::draw_arcs(g, out);
    }
    crate::stealth::draw(g, sim, out);
}

pub fn atmosphere(g: &BlackTap) -> Atmosphere {
    let c = g.content();
    let from = c.atmosphere(&g.atmos_from);
    let to = c.atmosphere(&g.atmos_to);
    let mut a = Atmosphere::blend(&from, &to, g.atmos_t);
    a.sea = Some(g.ocean().sea);
    a.sea_time = g.time;
    a.seabed = Some(g.world().seabed.clone());
    a.wind = g.sea.wind;
    a.gull = if g.gull { 1.0 } else { 0.0 };
    // Lightning in the storm: brief flashes from the game's clock (deterministic).
    if a.rain > 0.5 {
        let t = g.time * 0.37;
        let f = (t.sin() * 43758.5453).fract().abs();
        a.lightning = if f > 0.985 { 1.0 } else { 0.0 };
    }
    a
}

pub fn audio(g: &BlackTap, sim: &Sim) -> AudioState {
    let song = if g.mode == Mode::Title {
        "title".to_string()
    } else if g.demo.is_some() && g.music == "sailing" {
        "attract".to_string()
    } else if g.music == "sailing" {
        // The helm's D-pad picks a shanty in the rotation.
        format!("sailing#{}", g.shanty)
    } else {
        g.music.clone()
    };
    let intensity = crate::stealth::music_intensity(g).max(if g.sea.ships.iter().any(|s| s.telegraph > 0.0) { 0.9 } else { 0.5 });
    let a = atmosphere(g);
    let on_land = g.body == Body::Foot && BlackTap::player_feet(sim).is_some_and(|p| p.y > 0.5);
    AudioState {
        music: Some(MusicCue { song, intensity }),
        ambience: vec![
            ("sea".into(), if on_land { 0.35 } else { 0.85 }),
            ("wind".into(), (a.wind.length() / 12.0).clamp(0.1, 1.0)),
            ("rain".into(), a.rain),
            ("creak".into(), if g.body == Body::Helm { 0.6 } else { 0.0 }),
            ("sails".into(), if g.body == Body::Helm { g.sea.my_ship().map(|s| s.sail_shown / 2.0).unwrap_or(0.0) } else { 0.0 }),
            ("gulls".into(), if a.rain > 0.3 { 0.0 } else { 0.4 }),
            ("town".into(), crate::stealth::town_level(g, sim)),
            ("jungle".into(), crate::stealth::jungle_level(g, sim)),
        ],
        listener: None,
    }
}

pub fn effects(g: &BlackTap, _sim: &Sim) -> FrameEffects {
    FrameEffects {
        shake: g.shake,
        tints: Vec::new(),
        transition: g.spilled.map(|t| ("fade".to_string(), (1.0 - t / 2.5).clamp(0.0, 1.0))),
        rumble: g.rumble.clone(),
    }
}
