//! The game state and its `Game` hook. Scenes:
//! - `blacktap` / `blacktap/round/<n>`: play Round n (1–6) from its setup;
//! - `blacktap/title`: the title screen's live backdrop;
//! - `blacktap/demo/<n>`: attract segment n (the bot plays, `showcase` personality);
//! - `blacktap/drydock`: the developer level.

use std::collections::BTreeMap;
use std::sync::Arc;

use anyhow::{Result, anyhow, bail};
use glam::{Quat, Vec2, Vec3};
use pav_core::character::Weapon;
use pav_core::entity::EntityId;
use pav_core::frame::SimEvent;
use pav_core::games::{Game, GameBox, Outcome};
use pav_core::input::{InputFrame, buttons};
use pav_core::nav::NavGrid;
use pav_core::play::{
    Atmosphere, AudioState, CameraHint, DrawItem, DrawList, MarkerKind, MusicCue, Objective, Prompt, UiModel, WorldMarker,
};
use pav_core::room::RoomDef;
use pav_core::sim::Sim;
use pav_core::{Color, Look, Shape};
use pav_kit::camera::{OrbitCamera, Surroundings};
use pav_kit::demo::Showcase;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::content::Content;

/// What this scene is for.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    Title,
    Round(u8),
    Drydock,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct BlackTap {
    pub mode: Mode,
    /// Attract segment (1-based) when this scene is a demo.
    pub demo: Option<u8>,
    pub showcase: Option<Showcase>,
    pub time: f32,
    pub outcome: Outcome,
    pub cam: OrbitCamera,
    pub objective: Objective,
    pub goal: Vec3,
    pub atmosphere: String,
    /// Settings and progress from the app (difficulty, palettes...).
    #[serde(default)]
    pub options: serde_json::Value,
    #[serde(skip)]
    content: Option<Arc<Content>>,
    #[serde(skip)]
    nav: Option<Arc<NavGrid>>,
}

/// Builds a scene: the rest of its name after `blacktap/`.
pub fn build(sim: &mut Sim, rest: Option<&str>) -> Result<()> {
    let (mode, demo) = match rest.map(|r| r.split('/').collect::<Vec<_>>()).as_deref() {
        None | Some([""]) => (Mode::Round(1), None),
        Some(["title"]) => (Mode::Title, None),
        Some(["drydock"]) => (Mode::Drydock, None),
        Some(["round", n]) => {
            (Mode::Round(n.parse::<u8>().map_err(|_| anyhow!("blacktap/round/<1-6>: '{n}'"))?.clamp(1, 6)), None)
        }
        Some(["demo", n]) => {
            let n: u8 = n.parse().map_err(|_| anyhow!("blacktap/demo/<1-5>: '{n}'"))?;
            // Each attract segment starts from a Round's setup.
            (Mode::Round([2, 3, 4, 5, 6].get(n.saturating_sub(1) as usize).copied().unwrap_or(1)), Some(n.clamp(1, 5)))
        }
        Some(other) => bail!("unknown blacktap scene '{}' (title, round/<n>, demo/<n>, drydock)", other.join("/")),
    };
    let content = crate::content::load()?;
    let text = crate::data("levels/skeleton.toml").ok_or_else(|| anyhow!("levels/skeleton.toml is missing"))?;
    let def = RoomDef::parse(&text).map_err(|e| anyhow!("levels/skeleton.toml: {e}"))?;
    pav_core::scenes::build_standalone_room(sim, "blacktap", def);
    // The fire buttons are the game's (cannons, pistols), not the engine's bombs.
    sim.config.bombs.weapon = Weapon::None;
    sim.config.history_seconds = 10.0;
    let goal = pav_core::scenes::markers(sim, "flag").first().copied().unwrap_or(Vec3::new(6.0, 0.25, -2.0));
    let showcase = demo.map(|n| {
        let seg = content.demo.get(n as usize - 1);
        Showcase::new(seg.map(|s| s.caption.as_str()).unwrap_or("Demo"), seg.map(|s| s.seconds).unwrap_or(60.0))
    });
    let mut cam = OrbitCamera::default();
    let ctx = content.camera.get(if mode == Mode::Title { "title" } else { "foot" });
    cam.set_context(if mode == Mode::Title { "title" } else { "foot" }, &ctx, true);
    cam.face(90.0, &ctx);
    let game = BlackTap {
        mode,
        demo,
        showcase,
        time: 0.0,
        outcome: Outcome::Playing,
        cam,
        objective: Objective {
            title: "First Round — Fair Winds".into(),
            text: "Reach the Brotherhood flag".into(),
            optional: Vec::new(),
            age: 0.0,
        },
        goal,
        atmosphere: "golden_hour".into(),
        options: serde_json::Value::Null,
        content: Some(content),
        nav: None,
    };
    sim.state.play = Some(GameBox::new(game));
    Ok(())
}

impl BlackTap {
    fn content(&mut self) -> Arc<Content> {
        if self.content.is_none() {
            self.content = crate::content::load().ok();
        }
        self.content.clone().unwrap_or_else(|| Arc::new(Content::default()))
    }

    fn content_ref(&self) -> Arc<Content> {
        self.content.clone().or_else(|| crate::content::load().ok()).unwrap_or_else(|| Arc::new(Content::default()))
    }

    fn player_feet(sim: &Sim) -> Option<Vec3> {
        sim.state.player.and_then(|id| sim.feet(id))
    }
}

impl Game for BlackTap {
    const KEY: &'static str = "blacktap";

    fn pre(&mut self, sim: &mut Sim, input: &InputFrame, dt: f32, _events: &mut Vec<SimEvent>) -> BTreeMap<EntityId, InputFrame> {
        let mut out = BTreeMap::new();
        let content = self.content();
        let Some(pid) = sim.state.player else { return out };
        let mut ch = InputFrame { move_dir: input.move_dir, ..Default::default() };
        if self.mode == Mode::Title || self.outcome != Outcome::Playing {
            ch.move_dir = Vec2::ZERO;
        } else {
            // The game's buttons → the character's.
            if input.down(buttons::JUMP) {
                ch.held |= buttons::JUMP;
            }
            if input.just(buttons::JUMP) {
                ch.pressed |= buttons::JUMP;
            }
            if input.down(buttons::CROUCH) {
                ch.held |= buttons::CROUCH;
            }
        }
        out.insert(pid, ch);
        // The camera turns with the look input.
        let feet = Self::player_feet(sim).unwrap_or_default();
        let ctx_name = if self.mode == Mode::Title { "title" } else { "foot" };
        let ctx = content.camera.get(ctx_name);
        self.cam.set_context(ctx_name, &ctx, false);
        let heading = (input.move_dir.length() > 0.2).then(|| input.move_dir.x.atan2(-input.move_dir.y).to_degrees());
        let around = Surroundings { sim, exclude: None, sea: None };
        let look = if self.mode == Mode::Title { Vec2::new(4.0 * dt, 0.0) } else { input.look };
        self.cam.update(feet, look, heading, &ctx, &around, dt);
        out
    }

    fn post(&mut self, sim: &mut Sim, dt: f32, events: &mut Vec<SimEvent>) {
        self.time += dt;
        self.objective.age += dt;
        if let Some(s) = &mut self.showcase {
            s.tick(dt);
        }
        if self.outcome != Outcome::Playing || self.mode == Mode::Title {
            return;
        }
        let Some(p) = Self::player_feet(sim) else { return };
        if let Some(s) = &mut self.showcase {
            if Vec2::new(p.x - self.goal.x, p.z - self.goal.z).length() < 6.0 {
                s.note("explore");
            }
        }
        if Vec2::new(p.x - self.goal.x, p.z - self.goal.z).length() < 1.4 {
            self.outcome = Outcome::Won;
            self.objective.text = "Flag reached".into();
            self.objective.age = 0.0;
            events.push(SimEvent::cue("objective_done", self.goal));
            if let Some(s) = &mut self.showcase {
                s.note("objective");
                s.finish();
            }
        }
    }

    fn hud(&self, _sim: &Sim) -> Vec<String> {
        vec![format!("{}: {}", self.objective.title, self.objective.text)]
    }

    fn status(&self, sim: &Sim) -> serde_json::Value {
        let p = Self::player_feet(sim).map(|f| [f.x, f.y, f.z]);
        json!({
            "mode": self.mode,
            "demo": self.demo,
            "outcome": self.outcome,
            "time": self.time,
            "objective": self.objective.text,
            "player": p,
            "goal": [self.goal.x, self.goal.z],
            "showcase": self.showcase.as_ref().map(|s| s.state()),
        })
    }

    fn outcome(&self) -> Outcome {
        self.outcome
    }

    fn bot(&self, sim: &Sim) -> InputFrame {
        let mut input = InputFrame::default();
        if self.mode == Mode::Title {
            return input;
        }
        let Some(p) = Self::player_feet(sim) else { return input };
        let nav = self.nav.as_deref().cloned().or_else(|| NavGrid::for_room(sim));
        let to = nav.as_ref().and_then(|g| g.path(p, self.goal)).and_then(|path| path.first().copied()).unwrap_or(self.goal);
        input.move_dir = Vec2::new(to.x - p.x, to.z - p.z).normalize_or_zero();
        // Turn the camera toward where the bot walks, like a player would.
        let want = input.move_dir.x.atan2(-input.move_dir.y).to_degrees();
        input.look.x = pav_kit::camera::angle_diff(want, self.cam.yaw).clamp(-2.0, 2.0);
        input
    }

    fn camera(&self, _sim: &Sim) -> Option<CameraHint> {
        if self.mode == Mode::Title {
            return Some(pav_kit::camera::orbit_shot(self.goal + Vec3::Y * 2.0, 26.0, 9.0, 6.0, self.time, 50.0));
        }
        Some(self.cam.peek(None))
    }

    fn ui(&self, sim: &Sim) -> UiModel {
        let mut ui = UiModel::default();
        if self.mode == Mode::Title {
            return ui;
        }
        ui.objective = Some(self.objective.clone());
        ui.markers.push(WorldMarker {
            pos: self.goal + Vec3::Y * 3.2,
            kind: MarkerKind::Objective,
            label: "Brotherhood flag".into(),
            bar: None,
            distance: true,
        });
        if let Some(p) = Self::player_feet(sim)
            && p.distance(self.goal) < 4.0
        {
            ui.prompts.push(Prompt::new("interact", "Raise the flag"));
        }
        ui.demo = self.showcase.as_ref().map(|s| s.state());
        ui
    }

    fn draw(&self, _sim: &Sim, out: &mut DrawList) {
        // Placeholder sea (the ocean renderer replaces it) and the flag.
        out.push(DrawItem::Prim {
            shape: Shape::Box { half: Vec3::new(600.0, 0.05, 600.0) },
            pos: Vec3::new(0.0, -0.45, 0.0),
            rot: Quat::IDENTITY,
            color: Color::hex("#1f7a94"),
            emissive: 0.0,
            look: Look::Lit,
        });
        out.push(DrawItem::Prim {
            shape: Shape::Cylinder { half_height: 1.6, radius: 0.06 },
            pos: self.goal + Vec3::Y * 1.6,
            rot: Quat::IDENTITY,
            color: Color::hex("#6b4a2f"),
            emissive: 0.0,
            look: Look::Lit,
        });
        let wave = (self.time * 3.0).sin() * 0.08;
        out.push(DrawItem::Prim {
            shape: Shape::Box { half: Vec3::new(0.55, 0.35, 0.02) },
            pos: self.goal + Vec3::new(0.6, 2.8 + wave * 0.3, 0.0),
            rot: Quat::from_rotation_y(wave),
            color: Color::hex("#22201e"),
            emissive: 0.0,
            look: Look::Lit,
        });
        out.push(DrawItem::Decal { center: self.goal, radius: 1.4, ring: 0.85, color: Color::hex("#e2b04a"), alpha: 0.8 });
        // The Black Tap at anchor off the cay (shipyard preview until the sea arrives).
        let c = self.content_ref();
        if let (Some(class), Some(style)) = (c.classes.get("brig"), c.styles.get("black_tap")) {
            let hull = std::sync::Arc::new(pav_kit::shipyard::hull_mesh(class, style, 0xb1ac_7a90, 0.0, 1));
            let at = Vec3::new(self.goal.x - 24.0, -0.45, self.goal.z - 42.0);
            let rot = Quat::from_rotation_y(1.0);
            out.push(DrawItem::Mesh { mesh: hull, pos: at, rot, scale: Vec3::ONE, color: Color::WHITE, emissive: 0.0 });
            let sails = pav_kit::shipyard::sails_mesh(class, style, 2.0, 140.0, self.time, 0.0, true);
            out.push(DrawItem::Dynamic { mesh: sails, pos: at, rot, color: Color::WHITE, two_sided: true });
        }
    }

    fn atmosphere(&self, _sim: &Sim) -> Option<Atmosphere> {
        let c = self.content_ref();
        let mut a = c.atmosphere(&self.atmosphere);
        a.sea_time = self.time;
        Some(a)
    }

    fn audio(&self, _sim: &Sim) -> AudioState {
        let song = match (self.mode, self.demo) {
            (Mode::Title, _) => "title",
            (_, Some(_)) => "attract",
            _ => "sailing",
        };
        AudioState {
            music: Some(MusicCue { song: song.into(), intensity: 0.6 }),
            ambience: vec![("sea".into(), 0.8)],
            listener: None,
        }
    }

    fn restore(&mut self, v: &serde_json::Value) {
        self.options = v.clone();
    }

    fn progress(&self) -> Option<serde_json::Value> {
        Some(json!({ "rounds_done": if self.outcome == Outcome::Won { 1 } else { 0 } }))
    }
}
