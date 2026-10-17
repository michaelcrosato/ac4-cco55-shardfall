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
