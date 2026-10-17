//! Shared helpers: build Black Tap scenes and let the bot play them.
#![allow(dead_code)]

use pav_core::Sim;
use serde_json::Value;

pub fn sim(scene: &str, seed: u64) -> Sim {
    blacktap::register();
    Sim::new(scene, seed).unwrap_or_else(|e| panic!("{scene}: {e:#}"))
}

pub fn status(sim: &Sim) -> Value {
    sim.play().map(|g| g.status(sim)).unwrap_or(Value::Null)
}

/// Plays Round `n` from its own start with the bot until it is complete (or the next Round
/// starts), at most `minutes` of game time. Returns (complete, seconds, last status).
pub fn play_round(n: u8, seed: u64, minutes: f32) -> (bool, f32, Value) {
    let mut sim = sim(&format!("blacktap/round/{n}"), seed);
    let hz = sim.config.tick_rate.hz() as f32;
    let max = (minutes * 60.0 * hz) as u64;
    let mut t = 0;
    while t < max {
        for _ in 0..60 {
            let i = sim.bot_input();
            sim.step(&i);
        }
        t += 60;
        let s = status(&sim);
        assert!(s.is_object(), "no game status: {s}");
        let round = s["round"].as_u64().unwrap_or(0) as u8;
        if s["complete"].as_bool() == Some(true) || round != n || s["stats"]["rounds_done"].as_u64().unwrap_or(0) > 0 {
            return (true, t as f32 / hz, s);
        }
    }
    (false, t as f32 / hz, status(&sim))
}
