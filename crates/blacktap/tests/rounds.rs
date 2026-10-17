//! Each Round from its own start, played by the bot (`efficient`). The quick test plays the
//! first Round; `scripts/check.sh --full` runs every Round on five seeds (`--ignored`).

mod common;

fn check(n: u8, seeds: std::ops::RangeInclusive<u64>, minutes: f32) {
    let mut report = Vec::new();
    for seed in seeds {
        let (done, secs, s) = common::play_round(n, seed, minutes);
        report.push(format!("round {n} seed {seed}: {} in {secs:.0} s", if done { "complete" } else { "NOT complete" }));
        assert!(done, "round {n} seed {seed} not complete after {secs:.0} s: {s}");
    }
    eprintln!("{}", report.join("\n"));
}

#[test]
fn round_1_completes() {
    check(1, 1..=1, 6.0);
}

#[test]
#[ignore = "long: scripts/check.sh --full"]
fn round_1_five_seeds() {
    check(1, 1..=5, 6.0);
}

#[test]
#[ignore = "long: scripts/check.sh --full"]
fn round_2_five_seeds() {
    check(2, 1..=5, 12.0);
}

#[test]
#[ignore = "long: scripts/check.sh --full"]
fn round_3_five_seeds() {
    check(3, 1..=5, 14.0);
}

#[test]
#[ignore = "long: scripts/check.sh --full"]
fn round_4_five_seeds() {
    check(4, 1..=5, 10.0);
}

#[test]
#[ignore = "long: scripts/check.sh --full"]
fn round_5_five_seeds() {
    check(5, 1..=5, 12.0);
}

#[test]
#[ignore = "long: scripts/check.sh --full"]
fn round_6_five_seeds() {
    check(6, 1..=5, 16.0);
}
