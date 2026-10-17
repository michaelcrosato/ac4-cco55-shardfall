//! The whole of Tab I, Round 1 to TAB COMPLETE, played by the bot on three seeds (long).

mod common;

fn full(seed: u64) {
    let mut sim = common::sim("blacktap", seed);
    let hz = sim.config.tick_rate.hz() as f32;
    let max = (75.0 * 60.0 * hz) as u64;
    let mut t = 0;
    let mut rounds = Vec::new();
    let mut last_round = 0;
    while t < max {
        for _ in 0..120 {
            let i = sim.bot_input();
            sim.step(&i);
        }
        t += 120;
        let s = common::status(&sim);
        let r = s["round"].as_u64().unwrap_or(0);
        if r != last_round {
            rounds.push(format!("R{r} at {:.0} s", t as f32 / hz));
            last_round = r;
        }
        if s["flags_game"].as_array().is_some_and(|f| f.iter().any(|x| x == "tab_complete"))
            || s["tab_complete"].as_bool() == Some(true)
        {
            eprintln!("seed {seed}: TAB COMPLETE in {:.0} s ({})", t as f32 / hz, rounds.join(", "));
            return;
        }
    }
    panic!("seed {seed}: not complete after {:.0} s ({}): {}", t as f32 / hz, rounds.join(", "), common::status(&sim));
}

#[test]
#[ignore = "long: scripts/check.sh --full"]
fn full_level_seed_1() {
    full(1);
}

#[test]
#[ignore = "long: scripts/check.sh --full"]
fn full_level_seed_2() {
    full(2);
}

#[test]
#[ignore = "long: scripts/check.sh --full"]
fn full_level_seed_3() {
    full(3);
}
