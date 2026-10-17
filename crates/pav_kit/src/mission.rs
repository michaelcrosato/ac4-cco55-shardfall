//! Objective graphs from data: a mission is a list of steps, each with an objective line, a
//! completion condition and actions on start and on done, plus optional objectives tracked
//! alongside ("Full Pour"). Conditions and actions have a small generic vocabulary (flags,
//! counters, timers, all/any/not) and an escape hatch, `game = "<name>"`, that the game
//! evaluates or performs itself — so the content is data and the vocabulary is code.
//!
//! ```toml
//! id = "r1"
//! title = "First Round — Fair Winds"
//! setup = [{ game = "place_ship:r1_start" }]
//! [[step]]
//! id = "sails"
//! objective = "Raise the sails"
//! on_start = [{ line = "bung_sails" }]
//! done = { flag = "sails_raised" }
//! checkpoint = true
//! [[optional]]
//! id = "fast"
//! text = "Collect all three flotsam within 2:00"
//! track = { all = [{ counter = { name = "flotsam", at_least = 3 } }, { before = 120.0 }] }
//! ```

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

/// A condition.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Cond {
    Always,
    Never,
    Flag(String),
    All(Vec<Cond>),
    Any(Vec<Cond>),
    Not(Box<Cond>),
    /// Seconds spent in the current step.
    Timer(f32),
    /// The mission's clock is still under this many seconds.
    Before(f32),
    Counter { name: String, at_least: i64 },
    /// Evaluated by the game.
    Game(String),
}

/// An action.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Act {
    Flag(String),
    Unflag(String),
    Count { name: String, add: i64 },
    /// A dialog line by id.
    Line(String),
    Music(String),
    Atmosphere(String),
    /// Save the checkpoint.
    Checkpoint,
    Banner(String),
    Toast(String),
    /// Performed by the game.
    Game(String),
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Step {
    pub id: String,
    pub objective: String,
    /// Marker names the game shows for this step.
    pub markers: Vec<String>,
    pub on_start: Vec<Act>,
    pub done: Option<Cond>,
    pub on_done: Vec<Act>,
    /// Fails the mission (Spilled) while this step runs.
    pub fail: Option<Cond>,
    /// Save a checkpoint when the step starts.
    pub checkpoint: bool,
    /// Bot hint: what the bot should do in this step (the game's skill name).
    pub bot: String,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Optional {
    pub id: String,
    pub text: String,
    /// Done the moment this holds (tracked every tick while the mission runs).
    pub track: Option<Cond>,
    /// Failed (for good) the moment this holds.
    pub fail: Option<Cond>,
    /// Checked when the mission ends instead of while it runs.
    pub at_end: bool,
}

/// One mission (a Round).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct MissionDef {
    pub id: String,
    pub title: String,
    /// Actions when the mission starts from its own setup (Replay, tests, attract).
    pub setup: Vec<Act>,
    /// Actions whenever the mission starts (also when the previous one flows into it).
    pub on_start: Vec<Act>,
    pub step: Vec<Step>,
    pub optional: Vec<Optional>,
    pub on_complete: Vec<Act>,
}

impl MissionDef {
    pub fn parse(text: &str) -> anyhow::Result<Self> {
        Ok(toml::from_str(text)?)
    }

    /// Every `game = "..."` name used (conditions and actions), for data checks.
    pub fn game_names(&self) -> (BTreeSet<String>, BTreeSet<String>) {
        let mut conds = BTreeSet::new();
        let mut acts = BTreeSet::new();
        fn cond(c: &Cond, out: &mut BTreeSet<String>) {
            match c {
                Cond::Game(n) => {
                    out.insert(n.clone());
                }
                Cond::All(v) | Cond::Any(v) => v.iter().for_each(|c| cond(c, out)),
                Cond::Not(c) => cond(c, out),
                _ => {}
            }
        }
        let act = |a: &Act, out: &mut BTreeSet<String>| {
            if let Act::Game(n) = a {
                out.insert(n.clone());
            }
        };
        for a in self.setup.iter().chain(&self.on_start).chain(&self.on_complete) {
            act(a, &mut acts);
        }
        for s in &self.step {
            s.on_start.iter().chain(&s.on_done).for_each(|a| act(a, &mut acts));
            if let Some(c) = &s.done {
                cond(c, &mut conds);
            }
            if let Some(c) = &s.fail {
                cond(c, &mut conds);
            }
        }
        for o in &self.optional {
            for c in o.track.iter().chain(&o.fail) {
                cond(c, &mut conds);
            }
        }
        (conds, acts)
    }

    /// Every dialog line id used.
    pub fn lines(&self) -> BTreeSet<String> {
        let mut out = BTreeSet::new();
        let all = self.setup.iter().chain(&self.on_start).chain(&self.on_complete);
        for a in all.chain(self.step.iter().flat_map(|s| s.on_start.iter().chain(&s.on_done))) {
            if let Act::Line(l) = a {
                out.insert(l.clone());
            }
        }
        out
    }
}

/// What happened in a tick.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MissionEvent {
    StepStarted(usize),
    StepDone(usize),
    OptionalDone(String),
    OptionalFailed(String),
    Complete,
    Failed,
}

/// A mission in progress (game state).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct MissionRun {
    pub id: String,
    pub step: usize,
    pub step_time: f32,
    pub time: f32,
    pub flags: BTreeSet<String>,
    pub counters: BTreeMap<String, i64>,
    pub optional_done: BTreeSet<String>,
    pub optional_failed: BTreeSet<String>,
    pub started: bool,
    pub complete: bool,
    pub failed: bool,
}

impl MissionRun {
    pub fn new(def: &MissionDef) -> Self {
        Self { id: def.id.clone(), ..Default::default() }
    }

    pub fn flag(&self, name: &str) -> bool {
        self.flags.contains(name)
    }

    pub fn set_flag(&mut self, name: &str) {
        self.flags.insert(name.to_string());
    }

    pub fn count(&self, name: &str) -> i64 {
        self.counters.get(name).copied().unwrap_or(0)
    }

    pub fn add(&mut self, name: &str, n: i64) {
        *self.counters.entry(name.to_string()).or_default() += n;
    }

    pub fn current<'a>(&self, def: &'a MissionDef) -> Option<&'a Step> {
        if self.complete { None } else { def.step.get(self.step) }
    }

    /// Evaluates a condition (`game` asks the game).
    pub fn eval(&self, c: &Cond, game: &mut dyn FnMut(&str, &MissionRun) -> bool) -> bool {
        match c {
            Cond::Always => true,
            Cond::Never => false,
            Cond::Flag(f) => self.flags.contains(f),
            Cond::All(v) => v.iter().all(|c| self.eval(c, game)),
            Cond::Any(v) => v.iter().any(|c| self.eval(c, game)),
            Cond::Not(c) => !self.eval(c, game),
            Cond::Timer(s) => self.step_time >= *s,
            Cond::Before(s) => self.time < *s,
            Cond::Counter { name, at_least } => self.count(name) >= *at_least,
            Cond::Game(n) => game(n, self),
        }
    }

    /// Applies the generic actions to the run and returns everything for the game to perform
    /// (lines, music, game actions...; generic ones are returned too, for feedback).
    fn apply(&mut self, acts: &[Act], out: &mut Vec<Act>) {
        for a in acts {
            match a {
                Act::Flag(f) => {
                    self.flags.insert(f.clone());
                }
                Act::Unflag(f) => {
                    self.flags.remove(f);
                }
                Act::Count { name, add } => self.add(name, *add),
                _ => {}
            }
            out.push(a.clone());
        }
    }

    /// Starts the mission: `fresh` runs its setup (a Round played from its own start).
    pub fn start(&mut self, def: &MissionDef, fresh: bool) -> (Vec<Act>, Vec<MissionEvent>) {
        let mut acts = Vec::new();
        let mut events = Vec::new();
        self.started = true;
        if fresh {
            self.apply(&def.setup, &mut acts);
        }
        self.apply(&def.on_start, &mut acts);
        self.enter_step(def, 0, &mut acts, &mut events);
        (acts, events)
    }

    fn enter_step(&mut self, def: &MissionDef, i: usize, acts: &mut Vec<Act>, events: &mut Vec<MissionEvent>) {
        self.step = i;
        self.step_time = 0.0;
        if let Some(s) = def.step.get(i) {
            if s.checkpoint {
                acts.push(Act::Checkpoint);
            }
            self.apply(&s.on_start, acts);
            events.push(MissionEvent::StepStarted(i));
        }
    }

    /// One tick: tracks optional objectives, completes steps whose condition holds (several
    /// in one tick if they are already met), completes or fails the mission.
    pub fn tick(
        &mut self,
        def: &MissionDef,
        dt: f32,
        game: &mut dyn FnMut(&str, &MissionRun) -> bool,
    ) -> (Vec<Act>, Vec<MissionEvent>) {
        let mut acts = Vec::new();
        let mut events = Vec::new();
        if !self.started || self.complete || self.failed {
            return (acts, events);
        }
        self.time += dt;
        self.step_time += dt;
        for o in def.optional.iter().filter(|o| !o.at_end) {
            if self.optional_done.contains(&o.id) || self.optional_failed.contains(&o.id) {
                continue;
            }
            if o.fail.as_ref().is_some_and(|c| self.eval(c, game)) {
                self.optional_failed.insert(o.id.clone());
                events.push(MissionEvent::OptionalFailed(o.id.clone()));
            } else if o.track.as_ref().is_some_and(|c| self.eval(c, game)) {
                self.optional_done.insert(o.id.clone());
                events.push(MissionEvent::OptionalDone(o.id.clone()));
            }
        }
        for _ in 0..def.step.len() + 1 {
            let Some(step) = def.step.get(self.step) else { break };
            if step.fail.as_ref().is_some_and(|c| self.eval(c, game)) {
                self.failed = true;
                events.push(MissionEvent::Failed);
                return (acts, events);
            }
            if !step.done.as_ref().is_some_and(|c| self.eval(c, game)) {
                break;
            }
            self.apply(&step.on_done, &mut acts);
            events.push(MissionEvent::StepDone(self.step));
            if self.step + 1 >= def.step.len() {
                self.complete = true;
                for o in def.optional.iter().filter(|o| o.at_end) {
                    if !self.optional_failed.contains(&o.id) && o.track.as_ref().is_some_and(|c| self.eval(c, game)) {
                        self.optional_done.insert(o.id.clone());
                        events.push(MissionEvent::OptionalDone(o.id.clone()));
                    }
                }
                self.apply(&def.on_complete, &mut acts);
                events.push(MissionEvent::Complete);
                break;
            }
            let next = self.step + 1;
            self.enter_step(def, next, &mut acts, &mut events);
        }
        (acts, events)
    }

    /// Optional objectives as (text, done) for the HUD and results.
    pub fn checklist(&self, def: &MissionDef) -> Vec<(String, bool)> {
        def.optional.iter().map(|o| (o.text.clone(), self.optional_done.contains(&o.id))).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DEF: &str = r#"
id = "t"
title = "Test"
setup = [{ game = "place" }]
[[step]]
id = "a"
objective = "Do A"
done = { flag = "a" }
checkpoint = true
[[step]]
id = "b"
objective = "Do B"
on_start = [{ line = "hello" }]
done = { all = [{ counter = { name = "n", at_least = 2 } }, { game = "ready" }] }
[[optional]]
id = "quick"
text = "Fast"
track = { all = [{ flag = "a" }, { before = 1.0 }] }
"#;

    #[test]
    fn steps_progress_and_optionals_track() {
        let def = MissionDef::parse(DEF).unwrap();
        let mut run = MissionRun::new(&def);
        let (acts, _) = run.start(&def, true);
        assert!(acts.contains(&Act::Game("place".into())));
        assert!(acts.contains(&Act::Checkpoint));
        let mut ready = false;
        let (_, ev) = run.tick(&def, 0.1, &mut |n, _| n == "ready" && ready);
        assert!(ev.is_empty());
        run.set_flag("a");
        let (acts, ev) = run.tick(&def, 0.1, &mut |n, _| n == "ready" && ready);
        assert!(ev.contains(&MissionEvent::StepDone(0)) && ev.contains(&MissionEvent::OptionalDone("quick".into())));
        assert!(acts.contains(&Act::Line("hello".into())));
        run.add("n", 2);
        ready = true;
        let (_, ev) = run.tick(&def, 0.1, &mut |n, _| n == "ready" && ready);
        assert!(ev.contains(&MissionEvent::Complete) && run.complete);
        assert_eq!(run.checklist(&def), vec![("Fast".to_string(), true)]);
        let (conds, acts) = def.game_names();
        assert!(conds.contains("ready") && acts.contains("place"));
    }
}
