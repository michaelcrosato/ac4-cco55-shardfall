//! The game's data: every file parses, every reference resolves, every scene builds.

mod common;

#[test]
fn level_check_passes() {
    let mut sim = common::sim("blacktap/round/1", 1);
    let r = sim.game_command("level_check", &serde_json::json!({})).expect("command").expect("ok");
    assert_eq!(r["ok"], true, "{r}");
    assert!(r["checked"].as_u64().unwrap() > 100);
}

#[test]
fn every_scene_builds() {
    for scene in [
        "blacktap",
        "blacktap/title",
        "blacktap/round/1",
        "blacktap/round/2",
        "blacktap/round/3",
        "blacktap/round/4",
        "blacktap/round/5",
        "blacktap/round/6",
        "blacktap/round/6@escape",
        "blacktap/demo/1",
        "blacktap/demo/2",
        "blacktap/demo/3",
        "blacktap/demo/4",
        "blacktap/demo/5",
        "blacktap/freeroam",
        "blacktap/drydock",
    ] {
        let mut sim = common::sim(scene, 1);
        for _ in 0..30 {
            let i = sim.bot_input();
            sim.step(&i);
        }
        let f = sim.frame();
        let p = f.play.expect("a play frame");
        assert!(p.camera.is_some(), "{scene}: camera");
        assert!(p.atmosphere.is_some(), "{scene}: atmosphere");
    }
}

#[test]
fn front_end_info_is_complete() {
    blacktap::register();
    let info = blacktap::front_end();
    assert_eq!(
        (info.title.as_str(), info.tagline.as_str(), info.id.as_str()),
        ("Assassin's Keg", "Black Tap", "AssassinsKegBlackTap-W9")
    );
    assert_eq!(info.demo.len(), 5, "five attract segments");
    assert!(info.songs.len() >= 7, "six shanties and the title theme: {}", info.songs.len());
    for (name, text) in &info.banks {
        assert!(!text.is_empty(), "bank {name} is empty");
    }
    assert!(!info.theme.is_empty() && !info.bindings.is_empty());
}

#[test]
fn the_tab_holds_every_collectible() {
    // PLAN §14 F: 4 shanty sheets, 5 bottles, 8 chests, 3 Keg Taps on the islands, 6 flotsam.
    let text = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../games/blacktap/tab1.toml")).unwrap();
    let doc: toml::Value = toml::from_str(&text).unwrap();
    let mut count = std::collections::BTreeMap::new();
    for c in doc["collectible"].as_array().unwrap() {
        *count.entry(c["kind"].as_str().unwrap().to_string()).or_insert(0) += 1;
    }
    let flotsam = doc["marker"].as_table().unwrap().keys().filter(|k| k.starts_with("flotsam_")).count();
    assert_eq!(count.get("sheet"), Some(&4), "{count:?}");
    assert_eq!(count.get("bottle"), Some(&5), "{count:?}");
    assert_eq!(count.get("chest"), Some(&8), "{count:?}");
    assert_eq!(count.get("keg_tap"), Some(&3), "{count:?}");
    assert_eq!(flotsam, 6);
}

#[test]
fn three_difficulties() {
    let mut seen = Vec::new();
    for name in ["Landlubber", "Sailor", "Captain"] {
        let mut sim = common::sim("blacktap/round/2", 1);
        if let Some(g) = sim.state.play.as_mut() {
            g.restore(&serde_json::json!({ "settings": { "gameplay": { "difficulty": name } } }));
        }
        let d = common::status(&sim)["difficulty"].as_f64().unwrap();
        assert!(!seen.contains(&d), "{name} repeats a difficulty");
        seen.push(d);
    }
    assert!(seen[0] < seen[1] && seen[1] < seen[2], "{seen:?}");
}
