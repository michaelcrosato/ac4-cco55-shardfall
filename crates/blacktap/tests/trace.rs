//! A bot trace for debugging (not part of any gate): plays `BT_SCENE` (e.g. `blacktap`) on
//! `BT_SEED` for `BT_SECS` seconds, printing a compact status every `BT_EVERY` seconds.
//!
//! `BT_SCENE=blacktap/round/2 BT_SEED=1 cargo test -p blacktap --test trace -- --ignored --nocapture`

mod common;

use serde_json::Value;

fn env<T: std::str::FromStr>(k: &str, d: T) -> T {
    std::env::var(k).ok().and_then(|v| v.parse().ok()).unwrap_or(d)
}

fn compact(s: &Value) -> String {
    let r = |v: &Value| v.as_f64().map(|x| format!("{x:.0}")).unwrap_or_default();
    let xyz = |v: &Value| v.as_array().map(|a| a.iter().map(r).collect::<Vec<_>>().join(",")).unwrap_or_default();
    let enemies: Vec<String> = s["enemies"]
        .as_array()
        .map(|a| a.iter().map(|e| format!("{}:{}@{}", e["tag"].as_str().unwrap_or(""), r(&e["hull"]), xyz(&e["pos"]))).collect())
        .unwrap_or_default();
    let batteries: Vec<String> = s["batteries"]
        .as_array()
        .map(|a| a.iter().map(|b| format!("{}:{}", b["name"].as_str().unwrap_or(""), r(&b["hp"]))).collect())
        .unwrap_or_default();
    format!(
        "R{} {} [{}] body={} {} player=({}) ship=({} hdg {} spd {} hull {}) land={} enemies=[{}] batt=[{}] flags={}",
        s["round"],
        s["step"].as_str().unwrap_or(""),
        s["objective"].as_str().unwrap_or(""),
        s["body"].as_str().unwrap_or(""),
        s["traversal"].as_str().unwrap_or(""),
        xyz(&s["player"]),
        xyz(&s["ship"]["pos"]),
        r(&s["ship"]["heading"]),
        r(&s["ship"]["speed"]),
        r(&s["ship"]["hull"]),
        s["land"],
        enemies.join(" "),
        batteries.join(" "),
        s["flags"],
    )
}

#[test]
#[ignore = "debugging aid"]
fn trace() {
    // Only on request (it would otherwise ride along with `--ignored` gate runs).
    let Ok(scene) = std::env::var("BT_SCENE") else {
        eprintln!("trace: set BT_SCENE (e.g. blacktap/round/2) to run it");
        return;
    };
    let seed: u64 = env("BT_SEED", 1);
    let secs: f32 = env("BT_SECS", 300.0);
    let every: f32 = env("BT_EVERY", 10.0);
    let mut sim = common::sim(&scene, seed);
    let hz = sim.config.tick_rate.hz() as f32;
    let mut t = 0u64;
    while (t as f32) < secs * hz {
        for _ in 0..(every * hz) as u64 {
            let i = sim.bot_input();
            sim.step(&i);
        }
        t += (every * hz) as u64;
        let s = common::status(&sim);
        eprintln!("{:>5.0}s {}", t as f32 / hz, compact(&s));
        if s["tab_complete"].as_bool() == Some(true) {
            break;
        }
    }
}
