//! Progress round-trips: what a finished Round saves restores into a new scene.

mod common;

use pav_core::persist::{MemoryStore, Store};

#[test]
fn progress_round_trips_through_a_store() {
    let mut sim = common::sim("blacktap/round/1", 1);
    for _ in 0..60 {
        let i = sim.bot_input();
        sim.step(&i);
    }
    let p = sim.play().unwrap().progress().expect("progress");
    let mut store = MemoryStore::default();
    store.set("blacktap_progress", &p.to_string());
    let text = store.get("blacktap_progress").unwrap();
    let back: serde_json::Value = serde_json::from_str(&text).unwrap();
    assert_eq!(back, p);
    let mut next = common::sim("blacktap/title", 1);
    next.state.play.as_mut().unwrap().restore(&serde_json::json!({ "progress": back, "settings": null }));
    let again = next.play().unwrap().progress().unwrap();
    assert_eq!(again["version"], p["version"]);
}
