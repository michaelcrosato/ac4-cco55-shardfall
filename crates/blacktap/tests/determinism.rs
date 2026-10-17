//! Snapshots, rewind and replays are exact in a sea battle, a boarding and a stealth section.

mod common;

use pav_core::{InputFrame, Sim};

/// Plays `ticks` with the bot, recording the inputs; returns them.
fn play(sim: &mut Sim, ticks: u64) -> Vec<InputFrame> {
    let mut inputs = Vec::new();
    for _ in 0..ticks {
        let i = sim.bot_input();
        sim.step(&i);
        inputs.push(i);
    }
    inputs
}

fn exact(scene: &str, warmup: u64, ticks: u64) {
    let mut sim = common::sim(scene, 3);
    let mut all = play(&mut sim, warmup);
    // Snapshot → play → restore → play the same inputs: the same state.
    let snap = sim.snapshot();
    let inputs = play(&mut sim, ticks);
    all.extend(inputs.iter().cloned());
    let hash = sim.state_hash();
    sim.restore(snap);
    for i in &inputs {
        sim.step(i);
    }
    assert_eq!(sim.state_hash(), hash, "{scene}: snapshot + replay of {ticks} ticks");
    // Rewind a few seconds and forward again.
    let tick = sim.state.tick;
    assert!(sim.rewind_to(tick - 120), "{scene}: rewind");
    assert!(sim.rewind_to(tick), "{scene}: forward");
    assert_eq!(sim.state_hash(), hash, "{scene}: rewind and forward");
    // A fresh run from the scene and seed with the recorded inputs.
    let mut again = common::sim(scene, 3);
    for i in &all {
        again.step(i);
    }
    assert_eq!(again.state_hash(), hash, "{scene}: replay from the start");
}

#[test]
fn sea_battle_is_exact() {
    exact("blacktap/round/2", 60 * 40, 60 * 6);
}

#[test]
fn boarding_is_exact() {
    exact("blacktap/round/2@board", 60 * 12, 60 * 8);
}

#[test]
fn stealth_section_is_exact() {
    exact("blacktap/round/5", 60 * 10, 60 * 6);
}
