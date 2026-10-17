//! The game state and its `Game` hook. Scenes:
//! - `blacktap` / `blacktap/round/<n>`: play Round n (1–6) from its setup, then on through
//!   the Tab; `blacktap/round/6@escape` starts a Round at a named step (attract, tests);
//! - `blacktap/title`: the title screen's live backdrop;
//! - `blacktap/demo/<n>`: attract segment n (the bot plays, `showcase` personality);
//! - `blacktap/freeroam`: the isles after the Tab;
//! - `blacktap/drydock`: the developer level.
//!
//! Rules live in `rounds.rs` (the missions' vocabulary), `sea.rs` (ships), `foot.rs` (land),
//! presentation in `present.rs`, the bot in `bot.rs`.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use anyhow::{Result, anyhow, bail};
use glam::{Vec2, Vec3};
use pav_core::character::{ScriptPose, Scripted, Weapon as CharWeapon};
use pav_core::entity::EntityId;
use pav_core::frame::SimEvent;
use pav_core::games::{Game, GameBox, Outcome};
use pav_core::input::InputFrame;
use pav_core::ocean::Ocean;
use pav_core::play::{AudioState, Banner, CameraHint, DrawList, FrameEffects, RumbleCue, Toast, UiModel};
use pav_core::sim::{CheckpointRequest, Sim};
use pav_kit::camera::{OrbitCamera, Surroundings};
use pav_kit::demo::Showcase;
use pav_kit::dialog::Dialog;
use pav_kit::mission::{Act, MissionEvent, MissionRun};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::content::Content;
use crate::sea::Sea;
use crate::world::World;

/// Button bits (games/blacktap/bindings.toml).
pub mod b {
    pub const JUMP: u32 = 1 << 0;
    pub const CROUCH: u32 = 1 << 1;
    pub const INTERACT: u32 = 1 << 2;
    pub const FIRE: u32 = 1 << 3;
    pub const AIM: u32 = 1 << 4;
    pub const SPRINT: u32 = 1 << 5;
    pub const ATTACK: u32 = 1 << 6;
    pub const DROP: u32 = 1 << 13;
    pub const HEAL: u32 = 1 << 14;
    pub const GULL: u32 = 1 << 15;
    pub const TOOLS: u32 = 1 << 16;
    pub const WHISTLE: u32 = 1 << 17;
    pub const PREV: u32 = 1 << 18;
    pub const NEXT: u32 = 1 << 19;
    pub const MAP: u32 = 1 << 20;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    Title,
    Tab,
    FreeRoam,
    Drydock,
}

/// Where the player's body is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Body {
    /// At the wheel of the Black Tap.
    Helm,
    /// On foot, swimming or climbing (the land rules in `foot.rs`).
    Foot,
}

/// Difficulty and accessibility the rules use (from the app's settings).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Options {
    /// 0.6 Landlubber, 1.0 Sailor, 1.3 Captain.
    pub difficulty: f32,
    pub aim_assist: bool,
    pub colorblind: String,
    pub subtitles: bool,
    /// Gull Sight toggles on a press (else it shows while held).
    #[serde(default)]
    pub gull_toggle: bool,
}

impl Default for Options {
    fn default() -> Self {
        Self { difficulty: 1.0, aim_assist: true, colorblind: "off".into(), subtitles: true, gull_toggle: true }
    }
}

/// Tab-wide statistics for the results screen.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Stats {
    pub time: f32,
    pub reales: u32,
    pub collectibles: u32,
    pub assassinations: u32,
    pub ships_sunk: u32,
    pub detected: u32,
    pub spilled: u32,
    pub rounds_done: u32,
    pub optional_done: u32,
    pub optional_total: u32,
    /// Seconds each Round took.
    pub round_times: Vec<f32>,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct BlackTap {
    pub mode: Mode,
    /// 1..=6 while playing the Tab.
    pub round: u8,
    pub demo: Option<u8>,
    pub showcase: Option<Showcase>,
    pub time: f32,
    pub outcome: Outcome,
    pub mission: MissionRun,
    /// Seconds since the Round was completed (the next starts after a beat).
    pub between: Option<f32>,
    pub round_start: f32,
    pub body: Body,
    pub sea: Sea,
    /// Last tick's stick (sail level steps on its edges) and the interact hold.
    pub stick_prev: f32,
    pub hold: f32,
    pub cam: OrbitCamera,
    pub dialog: Dialog,
    /// Atmosphere: blending from one preset to another over `atmos_len` seconds.
    pub atmos_from: String,
    pub atmos_to: String,
    pub atmos_t: f32,
    pub atmos_len: f32,
    pub music: String,
    pub banner: Option<Banner>,
    pub toasts: Vec<Toast>,
    /// Spilled: seconds left before the checkpoint reloads.
    pub spilled: Option<f32>,
    pub stats: Stats,
    /// Onboarding: actions taught so far and the one on screen.
    pub taught: BTreeSet<String>,
    pub teaching: Option<String>,
    pub options: Options,
    pub gull: bool,
    /// Feel: shake strength and rumble cues to send.
    pub shake: f32,
    pub rumble: Vec<RumbleCue>,
    /// Once-only things that happened (beyond the mission's flags).
    pub flags: BTreeSet<String>,
    /// Shanty picked at the helm (D-pad).
    pub shanty: u32,
    /// The results screen's focused tile (Some while it shows).
    pub results: Option<usize>,
    pub land: crate::foot::Land,
    #[serde(default)]
    pub stealth: crate::stealth::Stealth,
    /// The world seed: runs on different seeds differ a little (wind, gun timing).
    #[serde(default)]
    pub seed: u64,
    /// The highest Round completed (saved progress), and seconds the "save" request shows.
    #[serde(default)]
    pub best_round: u8,
    #[serde(default)]
    pub save_for: f32,
    /// Where the showcase last counted travel as progress.
    #[serde(default)]
    pub showcase_anchor: Vec3,
    /// The level's rules file (tab1.toml; drydock.toml for the Drydock).
    #[serde(default = "tab1_level")]
    pub level: String,
    /// The Drydock's state (stations, pads).
    #[serde(default)]
    pub dock: crate::drydock::Dock,
    /// Events to emit in the next `post` (from places without the event list).
    #[serde(skip)]
    pub pending_events: Vec<SimEvent>,
    /// Buttons a tool presses on the next tick (`pav ship do=fire`).
    #[serde(skip)]
    pub tool_press: u32,
    #[serde(skip)]
    content: Option<Arc<Content>>,
    #[serde(skip)]
    world: Option<Arc<World>>,
}

/// Parses a scene name's rest into a mode, a Round, a demo segment and a start step.
fn parse_scene(rest: Option<&str>) -> Result<(Mode, u8, Option<u8>, Option<String>)> {
    let rest = rest.unwrap_or("");
    let (rest, step) = match rest.split_once('@') {
        Some((r, s)) => (r, Some(s.to_string())),
        None => (rest, None),
    };
    let parts: Vec<&str> = rest.split('/').filter(|p| !p.is_empty()).collect();
    Ok(match parts.as_slice() {
        [] => (Mode::Tab, 1, None, step),
        ["title"] => (Mode::Title, 1, None, None),
        ["drydock"] => (Mode::Drydock, 1, None, None),
        ["freeroam"] => (Mode::FreeRoam, 6, None, None),
        ["round", n] => {
            let n: u8 = n.parse().map_err(|_| anyhow!("blacktap/round/<1-6>: '{n}'"))?;
            (Mode::Tab, n.clamp(1, 6), None, step)
        }
        ["demo", n] => {
            let n: u8 = n.parse().map_err(|_| anyhow!("blacktap/demo/<1-5>: '{n}'"))?;
            // Rule the Waves, Storm the Fort, Ashore, Shadow of the Keg, Last Call.
            let (round, step) = match n.clamp(1, 5) {
                1 => (2, Some("disable".to_string())),
                2 => (3, None),
                3 => (4, None),
                4 => (5, Some("key".to_string())),
                _ => (6, Some("dive".to_string())),
            };
            (Mode::Tab, round, Some(n.clamp(1, 5)), step)
        }
        other => bail!("unknown blacktap scene '{}' (title, round/<n>[@step], demo/<n>, freeroam, drydock)", other.join("/")),
    })
}

/// Builds a scene: the rest of its name after `blacktap/`.
pub fn build(sim: &mut Sim, rest: Option<&str>) -> Result<()> {
    let (mode, round, demo, step) = parse_scene(rest)?;
    let content = crate::content::load()?;
    let level = if mode == Mode::Drydock { "drydock.toml" } else { "tab1.toml" };
    let world = crate::world::load(&crate::data(level).ok_or_else(|| anyhow!("games/blacktap/{level} is missing"))?)?;
    crate::foot::build_world(sim, &world, &content)?;
    sim.config.bombs.weapon = CharWeapon::None;
    // Free-running and climbing everything vertical; ledges grab; swimmers climb out.
    sim.config.movement.parkour = true;
    sim.config.movement.climb_all = true;
    sim.config.movement.ledge_grab = true;
    sim.config.history_seconds = 10.0;
    let mut game = BlackTap {
        mode,
        round,
        demo,
        showcase: None,
        time: 0.0,
        outcome: Outcome::Playing,
        mission: MissionRun::default(),
        between: None,
        round_start: 0.0,
        body: Body::Helm,
        sea: Sea { wind: Vec2::from_angle(seed_jitter(sim.state.seed, 0.35)).rotate(Vec2::new(5.0, -3.0)), ..Default::default() },
        stick_prev: 0.0,
        hold: 0.0,
        cam: OrbitCamera::default(),
        dialog: Dialog::default(),
        atmos_from: "golden_hour".into(),
        atmos_to: "golden_hour".into(),
        atmos_t: 1.0,
        atmos_len: 4.0,
        music: "sailing".into(),
        banner: None,
        toasts: Vec::new(),
        spilled: None,
        stats: Stats::default(),
        taught: BTreeSet::new(),
        teaching: None,
        options: Options::default(),
        gull: false,
        shake: 0.0,
        rumble: Vec::new(),
        flags: BTreeSet::new(),
        shanty: 0,
        results: None,
        land: crate::foot::Land::default(),
        stealth: crate::stealth::Stealth::default(),
        seed: sim.state.seed,
        level: level.to_string(),
        showcase_anchor: Vec3::ZERO,
        best_round: 0,
        save_for: 0.0,
        dock: crate::drydock::Dock::default(),
        pending_events: Vec::new(),
        tool_press: 0,
        content: Some(content.clone()),
        world: Some(world.clone()),
    };
    game.sea.jitter = seed_jitter(sim.state.seed, 1.5).abs();
    // The Black Tap is always afloat; six flotsam crates drift in the isles.
    let start = world.marker_xz("r1_start");
    game.sea.mine =
        game.sea.add_ship(&content.classes, "brig", "Black Tap", "black_tap", start, 45.0, pav_kit::naval::Faction::Brotherhood);
    for i in 1..=6 {
        let p = world.marker_xz(&format!("flotsam_{i}"));
        game.sea.add_float(p, crate::sea::FloatKind::Flotsam);
    }
    game.land.spawn_player(sim, &world);
    match mode {
        Mode::Title => crate::rounds::title_setup(&mut game, sim),
        Mode::Tab => {
            if let Some(n) = demo {
                let seg = content.demo.get(n as usize - 1);
                game.showcase = Some(Showcase::new(
                    seg.map(|s| s.caption.as_str()).unwrap_or("Demo"),
                    seg.map(|s| s.seconds).unwrap_or(60.0),
                ));
            }
            game.start_round(sim, round, true, step.as_deref());
        }
        Mode::FreeRoam => crate::rounds::free_roam_setup(&mut game, sim),
        Mode::Drydock => crate::rounds::drydock_setup(&mut game, sim),
    }
    let ctx = if mode == Mode::Title { "title" } else { "helm" };
    let c = content.camera.get(ctx);
    game.cam.set_context(ctx, &c, true);
    let h = game.sea.my_ship().map(|s| s.heading).unwrap_or(0.0);
    game.cam.face(h, &c);
    sim.state.ocean = Some(game.ocean());
    sim.state.play = Some(GameBox::new(game));
    Ok(())
}

impl BlackTap {
    pub fn content(&self) -> Arc<Content> {
        self.content.clone().or_else(|| crate::content::load().ok()).unwrap_or_default()
    }

    pub fn world(&self) -> Arc<World> {
        self.world.clone().unwrap_or_else(|| {
            let text = crate::data(&self.level).unwrap_or_default();
            crate::world::load(&text).unwrap_or_default()
        })
    }

    /// Restores the caches after a snapshot load.
    fn caches(&mut self) {
        if self.content.is_none() {
            self.content = crate::content::load().ok();
        }
        if self.world.is_none() {
            self.world = Some(self.world());
        }
    }

    /// The sea now (for characters, ships and the renderer).
    pub fn ocean(&self) -> Ocean {
        let c = self.content();
        let to = c.atmosphere(&self.atmos_to);
        let from = c.atmosphere(&self.atmos_from);
        let sea = match (from.sea, to.sea) {
            (Some(x), Some(y)) => pav_core::ocean::SeaState::blend(&x, &y, self.atmos_t),
            (_, Some(y)) => y,
            (Some(x), _) => x,
            _ => pav_core::ocean::SeaState::default(),
        };
        Ocean { sea, time: self.time }
    }

    pub fn player_feet(sim: &Sim) -> Option<Vec3> {
        sim.state.player.and_then(|id| sim.feet(id))
    }

    /// Changes the atmosphere toward a preset over `seconds`.
    pub fn set_atmosphere(&mut self, to: &str, seconds: f32) {
        if self.atmos_to == to {
            return;
        }
        self.atmos_from = self.atmos_to.clone();
        self.atmos_to = to.to_string();
        self.atmos_t = if seconds <= 0.0 { 1.0 } else { 0.0 };
        self.atmos_len = seconds.max(0.01);
    }

    pub fn banner(&mut self, text: &str, sub: &str, color: &str) {
        self.banner = Some(Banner { text: text.into(), sub: sub.into(), age: 0.0, color: color.into() });
    }

    pub fn toast(&mut self, text: &str) {
        if self.toasts.len() < 4 {
            self.toasts.push(Toast { text: text.into(), age: 0.0 });
        }
    }

    /// Showcase progress in attract mode.
    pub fn show(&mut self, mechanic: &str) {
        if let Some(s) = &mut self.showcase {
            s.note(mechanic);
        }
    }

    /// The feel of what happened at sea this tick beyond the rules' own (broadsides and hits
    /// shake in `rounds.rs`): a ram, and shells bursting close aboard, shake the camera and
    /// rumble the controller.
    fn sea_feel(&mut self, events: &[SimEvent]) {
        let Some(me) = self.sea.my_ship().filter(|s| s.alive()).map(|s| Vec3::new(s.pos.x, 0.0, s.pos.y)) else { return };
        if self.body != Body::Helm || self.mode == Mode::Title {
            return;
        }
        for e in events {
            let SimEvent::Cue { cue, pos, .. } = e else { continue };
            let d = Vec2::new(pos.x - me.x, pos.z - me.z).length();
            match cue.as_str() {
                "ram_hit" if d < 40.0 => self.shake(0.5, (1.0, 0.55, 280)),
                "splash_big" | "explosion" if d < 32.0 => {
                    let k = 1.0 - d / 32.0;
                    self.shake(0.12 + 0.3 * k, (0.35 + 0.6 * k, 0.2 + 0.3 * k, 120 + (120.0 * k) as u32));
                }
                _ => {}
            }
        }
    }

    pub fn shake(&mut self, s: f32, rumble: (f32, f32, u32)) {
        self.shake = self.shake.max(s);
        if self.rumble.len() < 8 {
            self.rumble.push(RumbleCue { low: rumble.0, high: rumble.1, ms: rumble.2 });
        }
    }

    /// Starts Round `n`: `fresh` runs its own setup (Replay, tests, attract); `at` jumps to a
    /// named step (its setup for that point is in `rounds::start_at`).
    pub fn start_round(&mut self, sim: &mut Sim, n: u8, fresh: bool, at: Option<&str>) {
        let c = self.content();
        let Some(def) = c.missions.get(n as usize - 1).cloned() else { return };
        self.round = n;
        self.mode = Mode::Tab;
        self.mission = MissionRun::new(&def);
        self.round_start = self.time;
        let (acts, events) = self.mission.start(&def, fresh);
        self.perform(sim, acts);
        self.mission_events(sim, events);
        if let Some(step) = at {
            crate::rounds::start_at(self, sim, step);
        }
        let title = def.title.clone();
        let (head, sub) =
            title.split_once(" — ").map(|(a, b)| (a.to_string(), b.to_string())).unwrap_or((title.clone(), String::new()));
        self.banner(&head.to_uppercase(), &sub, "#e2b04a");
        sim.request_checkpoint(CheckpointRequest::Save);
    }

    /// Carries out mission actions.
    pub fn perform(&mut self, sim: &mut Sim, acts: Vec<Act>) {
        let c = self.content();
        for a in acts {
            match a {
                Act::Line(id) => self.dialog.say(&c.dialog, &id),
                Act::Music(m) => self.music = m,
                Act::Atmosphere(name) => self.set_atmosphere(&name, 4.0),
                Act::Checkpoint => sim.request_checkpoint(CheckpointRequest::Save),
                Act::Banner(t) => self.banner(&t, "", "#e2b04a"),
                Act::Toast(t) => self.toast(&t),
                Act::Game(name) => crate::rounds::act(self, sim, &name),
                Act::Flag(_) | Act::Unflag(_) | Act::Count { .. } => {}
            }
        }
    }

    fn mission_events(&mut self, sim: &mut Sim, events: Vec<MissionEvent>) {
        let c = self.content();
        for e in events {
            match e {
                MissionEvent::StepStarted(_) => {
                    if let Some(s) = &mut self.showcase {
                        s.progress();
                    }
                }
                MissionEvent::StepDone(_) => {
                    let at = Self::player_feet(sim).unwrap_or_default();
                    self.pending_events.push(SimEvent::cue("objective_done", at));
                    self.show("objective");
                }
                MissionEvent::OptionalDone(id) => {
                    let text =
                        c.missions.iter().flat_map(|m| &m.optional).find(|o| o.id == id).map(|o| o.text.clone()).unwrap_or(id);
                    self.toast(&format!("Full Pour: {text}"));
                }
                MissionEvent::OptionalFailed(_) => {}
                MissionEvent::Complete => {
                    self.stats.rounds_done += 1;
                    if self.demo.is_none() {
                        self.best_round = self.best_round.max(self.round);
                        self.save_for = 1.0;
                    }
                    self.stats.round_times.push(self.time - self.round_start);
                    if let Some(d) = c.missions.get(self.round as usize - 1) {
                        self.stats.optional_total += d.optional.len() as u32;
                        self.stats.optional_done += self.mission.optional_done.len() as u32;
                    }
                    self.between = Some(0.0);
                    if let Some(s) = &mut self.showcase {
                        s.note("round_complete");
                        s.finish();
                    }
                    let at = Self::player_feet(sim).unwrap_or_default();
                    self.pending_events.push(SimEvent::cue("round_complete", at));
                }
                MissionEvent::Failed => self.spill(),
            }
        }
    }

    /// Spilled: the Round failed; reload the checkpoint after a beat.
    pub fn spill(&mut self) {
        if self.spilled.is_none() {
            self.spilled = Some(2.5);
            self.stats.spilled += 1;
            self.banner("SPILLED", "Back to the last checkpoint", "#a8322d");
        }
    }

    /// Onboarding: shows a big prompt until the action is performed once.
    pub fn teach(&mut self, what: &str) {
        if !self.taught.contains(what) {
            self.teaching = Some(what.to_string());
        }
    }

    pub fn learned(&mut self, what: &str) {
        if self.teaching.as_deref() == Some(what) {
            self.teaching = None;
        }
        self.taught.insert(what.to_string());
    }

    /// The player's ship's helm input from this tick's buttons and stick.
    fn helm_input(&mut self, input: &InputFrame, dt: f32) -> crate::sea::HelmInput {
        // The stick as the player pushed it: undo the camera turn the app applied.
        let f = self.cam.forward();
        let r = Vec2::new(-f.y, f.x);
        let axis = Vec2::new(input.move_dir.dot(r), input.move_dir.dot(f));
        let mut sail = 0;
        if axis.y > 0.6 && self.stick_prev <= 0.6 {
            sail = 1;
        } else if axis.y < -0.6 && self.stick_prev >= -0.6 {
            sail = -1;
        }
        self.stick_prev = axis.y;
        let interact = input.down(b::INTERACT);
        self.hold = if interact { self.hold + dt } else { 0.0 };
        crate::sea::HelmInput {
            helm: pav_kit::naval::Helm { sail, rudder: axis.x, boost: input.just(b::SPRINT), anchor: false },
            look_yaw: self.cam.yaw,
            aiming: input.down(b::AIM) || input.triggers[0] > 0.3,
            fire: input.just(b::FIRE),
            swivel: input.just(b::ATTACK),
            brace: input.just(b::INTERACT),
            repair: input.just(b::HEAL),
            spyglass: input.down(b::TOOLS),
        }
    }

    /// Puts the player's character at the wheel (scripted, standing at its post).
    fn seat_at_helm(&self, sim: &mut Sim) {
        let Some(pid) = sim.state.player else { return };
        let Some(ship) = self.sea.my_ship() else { return };
        let c = self.content();
        let class = c.classes.get(&ship.class).cloned().unwrap_or_default();
        let feet = ship.world(Vec3::new(0.0, class.freeboard + 0.6, -class.length * 0.36));
        let v = ship.forward() * ship.speed;
        let h = ship.heading.to_radians();
        let facing = h.sin().atan2(-h.cos());
        if let Some(ch) = sim.state.entities.get_mut(pid).and_then(|e| e.character.as_mut()) {
            ch.scripted =
                Some(Scripted { feet, facing, vel: Vec3::new(v.x, 0.0, v.y), pose: ScriptPose::Seated, climb_step: 0.0 });
        }
    }
}

impl Game for BlackTap {
    const KEY: &'static str = "blacktap";

    fn pre(&mut self, sim: &mut Sim, input: &InputFrame, dt: f32, events: &mut Vec<SimEvent>) -> BTreeMap<EntityId, InputFrame> {
        self.caches();
        // Rumble cues live for one tick: the frame after this tick carries what it started.
        self.rumble.clear();
        let pressed;
        let input = if self.tool_press != 0 {
            let tp = std::mem::take(&mut self.tool_press);
            pressed = InputFrame { held: input.held | tp, pressed: input.pressed | tp, ..*input };
            &pressed
        } else {
            input
        };
        let mut out = BTreeMap::new();
        let c = self.content();
        let w = self.world();
        let ocean = self.ocean();
        sim.state.ocean = Some(ocean);
        let playing = self.outcome == Outcome::Playing && self.spilled.is_none() && self.mode != Mode::Title;
        // Gull Sight: held, or toggled (the controls setting).
        let gull_now = if self.options.gull_toggle { self.gull ^ input.just(b::GULL) } else { input.down(b::GULL) };
        if playing && gull_now != self.gull {
            self.gull = gull_now;
            let at = Self::player_feet(sim).unwrap_or_default();
            events.push(SimEvent::cue(if self.gull { "gull_sight_on" } else { "gull_sight_off" }, at));
            self.learned("gull");
            self.show("gull_sight");
        }
        // Shanty choice at the helm.
        if self.body == Body::Helm && playing {
            if input.just(b::NEXT) {
                self.shanty = self.shanty.wrapping_add(1);
            }
            if input.just(b::PREV) {
                self.shanty = self.shanty.wrapping_sub(1);
            }
        }
        let helm = (self.body == Body::Helm && playing && !self.land.boarding_active()).then(|| self.helm_input(input, dt));
        if let Some(h) = &helm {
            if h.helm.sail != 0 {
                self.learned("sails");
            }
            if h.helm.rudder.abs() > 0.5 {
                self.learned("steer");
            }
            if h.brace {
                self.learned("brace");
            }
            if h.repair && self.sea.timber > 0 && self.sea.repair_cd <= 0.0 {
                self.show("repair");
            }
            if h.swivel && self.sea.swivel_target.is_some() {
                self.show("swivel");
            }
            if h.fire {
                self.learned("broadside");
            }
            if h.spyglass {
                self.learned("spyglass");
            }
        }
        let title_helm = (self.mode == Mode::Title).then(|| crate::rounds::title_helm(self));
        let braced_before = self.sea.my_ship().map(|s| s.brace_cd).unwrap_or(0.0);
        let first_sea_event = events.len();
        let sea_events = self.sea.step(&c.classes, &w, Some(&ocean), helm.or(title_helm), self.options.difficulty, dt, events);
        self.sea_feel(&events[first_sea_event..]);
        if self.sea.my_ship().is_some_and(|s| s.brace_cd > braced_before + 1.0) {
            self.show("brace");
        }
        crate::rounds::sea_events(self, sim, input, sea_events, events);
        if self.body == Body::Helm && !self.land.boarding_active() {
            self.seat_at_helm(sim);
            if let Some(pid) = sim.state.player {
                out.insert(pid, InputFrame::default());
            }
            // Leave the helm: hold interact when slow.
            if playing && self.hold > 0.8 && self.sea.my_ship().is_some_and(|s| s.speed.abs() <= 2.5) {
                self.hold = 0.0;
                crate::rounds::leave_helm(self, sim);
            }
        } else {
            out.extend(crate::foot::pre(self, sim, input, playing, dt, events));
            if let (Some(pid), Some(over)) = (sim.state.player, crate::traverse::pre(self, sim, input, playing, dt, events)) {
                out.insert(pid, over);
            }
        }
        // The town's and the fort's people (and the player's land tools).
        out.extend(crate::stealth::pre(self, sim, input, playing, dt, events));
        if self.mode == Mode::Drydock {
            out.extend(crate::drydock::pre(self, sim, input, dt, events));
        }
        // The camera.
        let (ctx_name, target, exclude) = crate::present::camera_context(self, sim, input);
        let ctx = c.camera.get(ctx_name);
        self.cam.set_context(ctx_name, &ctx, false);
        let heading = if self.body == Body::Helm {
            self.sea.my_ship().map(|s| s.heading)
        } else {
            (input.move_dir.length() > 0.2).then(|| input.move_dir.x.atan2(-input.move_dir.y).to_degrees())
        };
        let look = if self.mode == Mode::Title { Vec2::new(5.0 * dt, 0.0) } else { input.look };
        let around = Surroundings { sim, exclude, sea: None };
        self.cam.update(target, look, heading, &ctx, &around, dt);
        out
    }

    fn post(&mut self, sim: &mut Sim, dt: f32, events: &mut Vec<SimEvent>) {
        self.time += dt;
        self.stats.time = self.time;
        self.save_for = (self.save_for - dt).max(0.0);
        events.append(&mut self.pending_events);
        // Wakes: foam astern of every ship under way (a view-only cue).
        if sim.state.tick.is_multiple_of(5) {
            let c = self.content();
            for s in self.sea.ships.iter().filter(|s| s.alive() && s.speed > 2.0) {
                let len = c.classes.get(&s.class).map(|k| k.length).unwrap_or(20.0);
                let at = s.world(Vec3::new(0.0, 0.2, -len * 0.48));
                events.push(SimEvent::Cue {
                    cue: "wake_foam".into(),
                    pos: at,
                    gain: (s.speed / 7.0).clamp(0.3, 1.4),
                    pitch: 1.0,
                });
            }
        }
        let c = self.content();
        self.atmos_t = (self.atmos_t + dt / self.atmos_len.max(0.01)).min(1.0);
        self.shake = (self.shake - dt * 2.5).max(0.0);
        if let Some(b) = &mut self.banner {
            b.age += dt;
            if b.age > 4.0 {
                self.banner = None;
            }
        }
        for t in &mut self.toasts {
            t.age += dt;
        }
        self.toasts.retain(|t| t.age < 5.0);
        self.dialog.tick(&c.dialog, dt);
        for (voice, _) in self.dialog.started.clone() {
            let at = Self::player_feet(sim).unwrap_or_default();
            events.push(SimEvent::Cue { cue: format!("voice_{voice}"), pos: at, gain: 0.8, pitch: 1.0 });
        }
        if let Some(s) = &mut self.showcase {
            // Travelling counts as progress (sailing to the fort, walking to a target): the
            // watchdog only trips when the bot goes nowhere.
            let at = if self.body == Body::Helm {
                self.sea.my_ship().map(|v| Vec3::new(v.pos.x, 0.0, v.pos.y))
            } else {
                Self::player_feet(sim)
            };
            if let Some(at) = at {
                if at.distance(self.showcase_anchor) > 10.0 {
                    self.showcase_anchor = at;
                    s.progress();
                }
            }
            s.tick(dt);
        }
        crate::foot::post(self, sim, dt, events);
        crate::vfx::tick_cues(self, sim, dt, events);
        crate::rounds::post(self, sim, dt, events);
        if self.mode == Mode::Drydock {
            crate::drydock::post(self, sim, dt, events);
        }
        // Spilled → back to the checkpoint.
        if let Some(t) = &mut self.spilled {
            *t -= dt;
            if *t <= 0.0 {
                self.spilled = None;
                sim.request_checkpoint(CheckpointRequest::Load);
                return;
            }
        }
        if self.mode == Mode::Tab && self.spilled.is_none() {
            let Some(def) = c.missions.get(self.round as usize - 1).cloned() else { return };
            let mut answers: BTreeMap<String, bool> = BTreeMap::new();
            for name in def.game_names().0 {
                let v = crate::rounds::eval(self, sim, &name);
                answers.insert(name, v);
            }
            let (acts, mevents) = self.mission.tick(&def, dt, &mut |n, _| answers.get(n).copied().unwrap_or(false));
            self.perform(sim, acts);
            self.mission_events(sim, mevents);
            // The next Round after a beat (the Tab flows on).
            if let Some(t) = &mut self.between {
                *t += dt;
                if *t > 4.0 {
                    self.between = None;
                    if self.demo.is_some() {
                        // Attract segments end with their Round.
                    } else if self.round < 6 {
                        let next = self.round + 1;
                        self.start_round(sim, next, false, None);
                    } else {
                        crate::rounds::tab_complete(self, sim);
                    }
                }
            }
        }
    }

    fn hud(&self, _sim: &Sim) -> Vec<String> {
        let c = self.content();
        let step = c
            .missions
            .get(self.round as usize - 1)
            .and_then(|d| self.mission.current(d))
            .map(|s| s.objective.clone())
            .unwrap_or_default();
        vec![format!("Round {} — {}", self.round, step)]
    }

    fn status(&self, sim: &Sim) -> serde_json::Value {
        let c = self.content();
        let def = c.missions.get(self.round as usize - 1);
        let p = Self::player_feet(sim).map(|f| [round2(f.x), round2(f.y), round2(f.z)]);
        let ship = self.sea.my_ship().map(|s| {
            json!({"pos": [round2(s.pos.x), round2(s.pos.y)], "heading": round2(s.heading), "speed": round2(s.speed), "sail": s.sail, "hull": round2(s.hull)})
        });
        json!({
            "mode": self.mode,
            "round": self.round,
            "step": def.and_then(|d| self.mission.current(d)).map(|s| s.id.clone()),
            "objective": def.and_then(|d| self.mission.current(d)).map(|s| s.objective.clone()),
            "complete": self.mission.complete,
            "tab_complete": self.flags.contains("tab_complete"),
            "outcome": self.outcome,
            "time": round2(self.time),
            "body": self.body,
            "player": p,
            "traversal": sim
                .state
                .player
                .and_then(|id| sim.state.entities.get(id))
                .and_then(|e| e.character.as_ref())
                .map(|c| format!("{:?}", c.traversal_state())),
            "ship": ship,
            "enemies": self.sea.ships.iter().filter(|s| s.id != self.sea.mine && !s.sunk).map(|s| json!({"tag": s.tag, "hull": round2(s.hull), "disabled": s.disabled, "pos": [round2(s.pos.x), round2(s.pos.y)]})).collect::<Vec<_>>(),
            "batteries": self.sea.batteries.iter().map(|b| json!({"name": b.name, "hp": round2(b.hp), "destroyed": b.destroyed})).collect::<Vec<_>>(),
            "flags": self.mission.flags,
            "counters": self.mission.counters,
            "optional_done": self.mission.optional_done,
            "stats": self.stats,
            "showcase": self.showcase.as_ref().map(|s| s.state()),
            "land": crate::foot::status(self, sim),
        })
    }

    fn outcome(&self) -> Outcome {
        self.outcome
    }

    fn bot(&self, sim: &Sim) -> InputFrame {
        crate::bot::input(self, sim)
    }

    fn camera(&self, sim: &Sim) -> Option<CameraHint> {
        crate::present::camera(self, sim)
    }

    fn ui(&self, sim: &Sim) -> UiModel {
        crate::present::ui(self, sim)
    }

    fn draw(&self, sim: &Sim, out: &mut DrawList) {
        crate::present::draw(self, sim, out)
    }

    fn atmosphere(&self, _sim: &Sim) -> Option<pav_core::play::Atmosphere> {
        Some(crate::present::atmosphere(self))
    }

    fn audio(&self, sim: &Sim) -> AudioState {
        crate::present::audio(self, sim)
    }

    fn effects(&self, sim: &Sim) -> FrameEffects {
        crate::present::effects(self, sim)
    }

    fn time_scale(&self) -> f32 {
        self.land.time_scale()
    }

    fn restore(&mut self, v: &serde_json::Value) {
        if let Some(s) = v.get("settings") {
            let diff = s.pointer("/gameplay/difficulty").and_then(|d| d.as_str()).unwrap_or("Sailor");
            self.options.difficulty = match diff {
                "Landlubber" => 0.6,
                "Captain" => 1.3,
                _ => 1.0,
            };
            self.options.aim_assist = s.pointer("/accessibility/aim_assist").and_then(|a| a.as_bool()).unwrap_or(true);
            self.options.colorblind =
                s.pointer("/accessibility/colorblind").and_then(|a| a.as_str()).unwrap_or("Off").to_lowercase();
            self.options.subtitles = s.pointer("/accessibility/subtitles").and_then(|a| a.as_bool()).unwrap_or(true);
            self.options.gull_toggle = s.pointer("/controls/gull_toggle").and_then(|a| a.as_bool()).unwrap_or(true);
        }
        if let Some(p) = v.get("progress").filter(|p| !p.is_null()) {
            crate::rounds::restore_progress(self, p);
        }
    }

    fn progress(&self) -> Option<serde_json::Value> {
        Some(crate::rounds::progress(self))
    }

    fn command(&mut self, sim: &mut Sim, name: &str, args: &serde_json::Value) -> Option<Result<serde_json::Value, String>> {
        self.caches();
        crate::commands::command(self, sim, name, args)
    }
}

fn tab1_level() -> String {
    "tab1.toml".into()
}

/// A deterministic offset in ±`range` from the world seed (seed 1 gives 0: the authored run).
pub fn seed_jitter(seed: u64, range: f32) -> f32 {
    if seed <= 1 {
        return 0.0;
    }
    let mut h = seed.wrapping_mul(0x9E37_79B9_7F4A_7C15);
    h ^= h >> 31;
    ((h % 2001) as f32 / 1000.0 - 1.0) * range
}

pub fn round2(x: f32) -> f64 {
    (x as f64 * 100.0).round() / 100.0
}
