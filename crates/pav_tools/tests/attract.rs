//! Attract mode headless (PLAN §12): the real front end idles into the playlist, the game's bot
//! plays every segment, a key press returns to the title on the same update, and the playlist
//! loops over the seed sets with no stalls.

use pav_tools::kit_tools::run_attract;

fn info() -> pav_core::play::FrontEndInfo {
    blacktap::register();
    blacktap::front_end()
}

#[test]
fn attract_starts_plays_and_exits() {
    let run = run_attract(&info(), 2.0, 0.5, 30.0, 0).expect("attract");
    assert!(run.load_errors.is_empty(), "{:?}", run.load_errors);
    assert!(!run.segments.is_empty(), "no segment played");
    assert!(run.exits_ok >= 2 && run.exits_failed == 0, "exits ok {} failed {}", run.exits_ok, run.exits_failed);
    assert!(
        run.segments.iter().all(|s| s.scene.starts_with("blacktap/demo/")),
        "{:?}",
        run.segments.iter().map(|s| &s.scene).collect::<Vec<_>>()
    );
}

/// `pav attract minutes=30`: five loops of the playlist (five seed sets), zero stalls, and every
/// segment shows at least three mechanics.
#[test]
#[ignore = "long: scripts/check.sh --full"]
fn attract_thirty_minutes_no_stalls() {
    let run = run_attract(&info(), 30.0, 0.5, 0.0, 0).expect("attract");
    let lines: Vec<String> = run
        .segments
        .iter()
        .map(|s| {
            format!(
                "{} seed {} {:.0} s events {} mechanics {:?}{}",
                s.scene,
                s.seed,
                s.seconds,
                s.events,
                s.mechanics,
                if s.stalled { " STALLED" } else { "" }
            )
        })
        .collect();
    eprintln!("{}", lines.join("\n"));
    assert!(run.load_errors.is_empty(), "{:?}", run.load_errors);
    let seeds: std::collections::BTreeSet<u64> =
        run.segments.iter().filter(|s| s.scene.ends_with("/1")).map(|s| s.seed).collect();
    assert!(seeds.len() >= 5, "five seed sets: {seeds:?}");
    assert_eq!(run.stalls, 0, "stalls:\n{}", lines.join("\n"));
    let thin: Vec<&String> =
        run.segments.iter().zip(&lines).filter(|(s, _)| s.mechanics.len() < 3 && s.seconds > 10.0).map(|(_, l)| l).collect();
    assert!(thin.is_empty(), "segments showing fewer than 3 mechanics: {thin:?}");
}
