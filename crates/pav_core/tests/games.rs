//! Games built on the engine (`pav_core::games`): the starter's bot wins on many seeds, the
//! starter can be lost, and a game's state survives snapshot files, rewind and replays like
//! the rest of the simulation. Copy these tests for a new game.

use pav_core::games::{GAMES, Outcome};
use pav_core::{InputFrame, Sim, SimState};

#[test]
fn the_starter_bot_wins_on_ten_seeds() {
    for seed in 1..=10 {
        let mut sim = Sim::new("starter", seed).unwrap();
        let (outcome, ticks) = sim.run_bot(60 * 60);
        let status = sim.play().unwrap().status(&sim);
        assert_eq!(outcome, Outcome::Won, "seed {seed}, {ticks} ticks: {status}");
    }
}

#[test]
fn standing_still_gets_caught() {
    let mut sim = Sim::new("starter", 1).unwrap();
    assert_eq!(sim.frame().hud.lines, ["Shards 0/5", "Lives 3"]);
    for _ in 0..60 * 60 {
        sim.step(&InputFrame::default());
    }
    assert_eq!(sim.outcome(), Outcome::Lost);
    assert_eq!(sim.frame().hud.lines, ["Shards 0/5", "Lives 0", "Game over"]);
}

#[test]
fn snapshots_rewind_and_replays_are_exact() {
    let mut sim = Sim::new("starter", 3).unwrap();
    for _ in 0..240 {
        let f = sim.bot_input();
        sim.step(&f);
    }
    // A snapshot file (CBOR) restores the game, and both copies then play on the same.
    let mut bytes = Vec::new();
    ciborium::into_writer(&sim.snapshot(), &mut bytes).unwrap();
    let state: SimState = ciborium::from_reader(bytes.as_slice()).unwrap();
    let mut copy = Sim::new("starter", 3).unwrap();
    copy.restore(state);
    assert_eq!(copy.state_hash(), sim.state_hash());
    for _ in 0..240 {
        let (a, b) = (sim.bot_input(), copy.bot_input());
        sim.step(&a);
        copy.step(&b);
    }
    assert_eq!(copy.state_hash(), sim.state_hash());

    let (tick, hash) = (sim.state.tick, sim.state_hash());
    assert!(sim.rewind_to(tick - 90));
    assert!(sim.rewind_to(tick));
    assert_eq!(sim.state_hash(), hash);

    let mut again = Sim::new("starter", 3).unwrap();
    for f in sim.recording.iter() {
        again.step(f);
    }
    assert_eq!(again.state_hash(), hash);
}

#[test]
fn game_keys_name_one_scene_each() {
    let names = pav_core::scenes::names();
    for g in GAMES {
        assert_eq!(names.iter().filter(|n| *n == g.key).count(), 1, "'{}' is also a scene or room key", g.key);
        assert!(!g.key.contains('/'), "'{}': `<key>/<rest>` passes the rest to the game", g.key);
        assert!(Sim::new(g.key, 1).unwrap().play().is_some_and(|p| p.key() == g.key));
    }
}

/// A game that only listens: what zone events its `post` sees.
#[derive(Clone, Default, serde::Serialize, serde::Deserialize)]
struct ZoneProbe {
    seen: Vec<String>,
}

impl pav_core::games::Game for ZoneProbe {
    const KEY: &'static str = "zone_probe";

    fn post(&mut self, _sim: &mut Sim, _dt: f32, events: &mut Vec<pav_core::SimEvent>) {
        for e in events.iter() {
            match e {
                pav_core::SimEvent::Checkpoint { .. } => self.seen.push("checkpoint".into()),
                pav_core::SimEvent::Splash { .. } => self.seen.push("splash".into()),
                _ => {}
            }
        }
    }

    fn status(&self, _sim: &Sim) -> serde_json::Value {
        serde_json::json!({ "seen": self.seen })
    }
}

#[test]
fn zone_events_reach_the_games_post() {
    use pav_core::glam::Vec3;
    use pav_core::statics::RegionKey;
    use pav_core::zones::{Zone, ZoneKind};
    let mut sim = Sim::new("starter", 1).expect("starter builds");
    sim.state.play = Some(pav_core::games::GameBox::new(ZoneProbe::default()));
    let feet = sim.state.player.and_then(|id| sim.feet(id)).expect("a player");
    let zone = |kind, min: Vec3, max: Vec3| Zone {
        min,
        max,
        kind,
        course: String::new(),
        index: 0,
        params: Default::default(),
        camera: None,
        label: String::new(),
        label_size: None,
        color: None,
        facing: None,
        speed: 0.0,
        signal: String::new(),
        note: String::new(),
    };
    let key = RegionKey::chunk_of(feet);
    sim.state
        .statics
        .add_zone_to(key, zone(ZoneKind::Checkpoint, feet - Vec3::new(1.0, 0.5, 1.0), feet + Vec3::new(1.0, 2.0, 1.0)));
    sim.state.statics.add_zone_to(key, zone(ZoneKind::Water, feet - Vec3::new(1.0, 0.5, 1.0), feet + Vec3::new(1.0, 0.3, 1.0)));
    for _ in 0..3 {
        sim.step(&InputFrame::default());
    }
    let seen = sim.play().map(|g| g.status(&sim)).unwrap_or_default();
    let seen: Vec<&str> = seen["seen"].as_array().map(|a| a.iter().filter_map(|v| v.as_str()).collect()).unwrap_or_default();
    assert!(seen.contains(&"checkpoint"), "the checkpoint zone's event never reached post: {seen:?}");
    assert!(seen.contains(&"splash"), "the water zone's event never reached post: {seen:?}");
}

#[test]
fn hook_games_can_turn_the_bombs_off() {
    use pav_core::character::Weapon;
    // Holding the fire buttons for a second: bombs (and shots) with the engine's default
    // weapon, nothing with `weapon = none`.
    let fired = |weapon: Weapon| {
        let mut sim = Sim::new("starter", 1).expect("starter builds");
        sim.config.bombs.weapon = weapon;
        let mut press = InputFrame::default();
        press.held = pav_core::input::buttons::PRIMARY | pav_core::input::buttons::USE;
        press.pressed = press.held;
        let mut n = 0;
        for _ in 0..60 {
            sim.step(&press);
            n = n.max(sim.state.entities.iter().filter(|e| e.name == "bomb").count() + sim.state.projectiles.list.len());
        }
        n
    };
    assert!(fired(Weapon::Bombs) > 0, "the default weapon should throw bombs");
    assert_eq!(fired(Weapon::None), 0, "weapon = none still fired");
}

#[test]
fn checkpoints_live_outside_the_state() {
    let mut sim = Sim::new("starter", 1).expect("starter builds");
    let pid = sim.state.player.expect("a player");
    for _ in 0..10 {
        sim.step(&InputFrame::default());
    }
    let saved_at = sim.feet(pid).unwrap();
    sim.checkpoint_save();
    let snapshot_after_save = sim.snapshot();
    let mut walk = InputFrame::default();
    walk.move_dir = pav_core::glam::Vec2::new(1.0, 0.0);
    for _ in 0..90 {
        sim.step(&walk);
    }
    let moved_to = sim.feet(pid).unwrap();
    assert!(moved_to.distance(saved_at) > 1.0, "the player should have walked away");
    let tick = sim.state.tick;
    assert!(sim.checkpoint_load(), "a checkpoint was saved");
    assert!(sim.feet(pid).unwrap().distance(saved_at) < 0.05, "back where the checkpoint was taken");
    assert_eq!(sim.state.tick, tick, "the tick counter keeps running forward");
    // The checkpoint is the sim's, not the state's: restoring an older snapshot keeps it.
    sim.restore(snapshot_after_save);
    assert!(sim.checkpoint_load(), "restoring a snapshot doesn't drop the checkpoint");
}
