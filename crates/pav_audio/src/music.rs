//! Music: `music.toml` (songs, arrangements, states, stingers), the arranger (ABC tune +
//! arrangement → timed note events per part) and the sequencer that plays them
//! sample-accurately inside the audio callback, with stems by intensity and crossfades.
//!
//! ```toml
//! crossfade = 2.0
//!
//! [song.drunken_sailor]
//! abc = "drunken_sailor"       # a file in shanties/ (default: the song's name)
//! tempo = 120                  # in the tune's beat (Q: unit); default: the ABC's Q:
//! passes = 2                   # times through the tune per play
//! section = [{ bars = "1-4", role = "call" }, { bars = "9-16", role = "chorus" }]
//! [[song.drunken_sailor.part]]
//! inst = "fiddle"              # see instruments::Inst
//! role = "melody"              # melody harmony choir bass chords arp drums drone
//! stem = 0.0                   # plays when intensity >= stem (fades over ±0.1)
//! pattern = "R...F..."         # bass/chords/arp/drums: one bar, evenly divided
//!
//! [state.sailing]
//! songs = ["drunken_sailor", "haul_away_joe"]
//! shuffle = true
//!
//! [stinger.detection]
//! song = "stinger_detection"
//! duck = 0.6
//! ```

use std::collections::BTreeMap;
use std::sync::Arc;

use anyhow::{Context, bail};
use serde::Deserialize;

use crate::abc::{self, Chord, Key, Song};
use crate::bank::{Bus, Layer};
use crate::dsp::{Formant, Rng, hash_str, midi_hz};
use crate::instruments::Inst;
use crate::voice::{Consonant, Group, Spawn};

// ---------------------------------------------------------------- spec

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    #[default]
    Melody,
    /// The melody a third (or a chord tone) below.
    Harmony,
    /// The melody sung: solo on calls, the crew on responses, unison + octave + fifth on choruses.
    Choir,
    Bass,
    Chords,
    Arp,
    Drums,
    /// The tonic and fifth held through the whole pass.
    Drone,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct PartSpec {
    pub inst: String,
    pub role: Role,
    pub gain: f32,
    pub pan: f32,
    pub octave: i32,
    pub transpose: i32,
    /// Intensity threshold (0..1) and optional ceiling.
    pub stem: f32,
    pub stem_max: Option<f32>,
    pub pattern: Option<String>,
    /// Only on these passes (1-based); empty = all.
    pub passes: Vec<u32>,
    /// Only in these bars ("1-8,17-24"), counted through the repeats; empty = all.
    pub bars: Option<String>,
    /// Only in sections with these roles (call, response, chorus, verse...).
    pub sections: Vec<String>,
    /// Note length factor.
    pub legato: Option<f32>,
    pub humanize: Option<f32>,
    /// Delays every other pattern step by this fraction of a step.
    pub swing: f32,
    /// Extra velocity on the downbeat (half as much mid-bar).
    pub accent: f32,
    pub vel: f32,
    /// Choir: default vowel; which voices sing choruses; who sings the calls.
    pub vowel: Option<String>,
    pub voices: Option<Vec<String>>,
    pub call: Option<String>,
    pub reverb: Option<f32>,
    pub echo: Option<f32>,
    /// Guitar strum spread (ms between strings).
    pub strum: f32,
    /// Lowest MIDI note of the voicing window (chords, bass, arp, drone).
    pub low: Option<i32>,
}

impl Default for PartSpec {
    fn default() -> Self {
        Self {
            inst: String::new(),
            role: Role::Melody,
            gain: 1.0,
            pan: 0.0,
            octave: 0,
            transpose: 0,
            stem: 0.0,
            stem_max: None,
            pattern: None,
            passes: Vec::new(),
            bars: None,
            sections: Vec::new(),
            legato: None,
            humanize: None,
            swing: 0.0,
            accent: 0.15,
            vel: 1.0,
            vowel: None,
            voices: None,
            call: None,
            reverb: None,
            echo: None,
            strum: 14.0,
            low: None,
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct SectionSpec {
    pub bars: String,
    pub role: String,
    pub vowel: Option<String>,
}

impl Default for SectionSpec {
    fn default() -> Self {
        Self { bars: String::new(), role: "verse".into(), vowel: None }
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct SongSpec {
    pub about: String,
    pub abc: Option<String>,
    pub tempo: Option<f64>,
    pub passes: u32,
    /// Beats of silence after the last pass.
    pub gap: f64,
    pub transpose: i32,
    pub gain: f32,
    pub humanize: f32,
    pub part: Vec<PartSpec>,
    pub section: Vec<SectionSpec>,
}

impl Default for SongSpec {
    fn default() -> Self {
        Self {
            about: String::new(),
            abc: None,
            tempo: None,
            passes: 1,
            gap: 0.0,
            transpose: 0,
            gain: 1.0,
            humanize: 1.0,
            part: Vec::new(),
            section: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct StateSpec {
    pub about: String,
    pub songs: Vec<String>,
    /// Played once before the playlist.
    pub intro: Option<String>,
    pub crossfade: Option<f32>,
    pub shuffle: bool,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct StingerSpec {
    pub about: String,
    pub song: String,
    /// How far the music dips under it (0..1).
    pub duck: f32,
    pub gain: f32,
}

impl Default for StingerSpec {
    fn default() -> Self {
        Self { about: String::new(), song: String::new(), duck: 0.5, gain: 1.0 }
    }
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct MusicFile {
    crossfade: Option<f32>,
    song: BTreeMap<String, SongSpec>,
    state: BTreeMap<String, StateSpec>,
    stinger: BTreeMap<String, StingerSpec>,
}

// ---------------------------------------------------------------- arranged

/// One note (or chord, or drum hit) of one part.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Ev {
    pub start: f64,
    pub len: f64,
    pub inst: Inst,
    pub part: u16,
    /// MIDI notes (fractional allowed); unpitched instruments use notes[0] as a pitch ratio
    /// in semitones from their own tuning.
    pub notes: [f32; 4],
    pub n: u8,
    pub vel: f32,
    pub pan: f32,
    pub vowel: (Formant, Formant),
    pub cons: Consonant,
    pub shift: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PartRt {
    pub inst: Inst,
    pub role: Role,
    pub stem: f32,
    pub stem_max: f32,
    pub gain: f32,
    pub reverb: f32,
    pub echo: f32,
}

impl PartRt {
    /// How much of this stem plays at `intensity`.
    pub fn stem_gain(&self, intensity: f32) -> f32 {
        let up = if self.stem <= 0.0 { 1.0 } else { smooth((intensity - self.stem + 0.1) / 0.2) };
        let down = if self.stem_max >= 1.0 { 1.0 } else { 1.0 - smooth((intensity - self.stem_max + 0.1) / 0.2) };
        up * down
    }
}

fn smooth(x: f32) -> f32 {
    let x = x.clamp(0.0, 1.0);
    x * x * (3.0 - 2.0 * x)
}

/// A song ready to play.
#[derive(Clone, Debug, PartialEq)]
pub struct Arranged {
    pub name: String,
    pub title: String,
    /// Quarter notes per minute.
    pub qpm: f64,
    /// Beats per pass and in total (passes and gap).
    pub pass_beats: f64,
    pub length: f64,
    pub bar_beats: f64,
    pub events: Vec<Ev>,
    pub parts: Vec<PartRt>,
    pub gain: f32,
    pub song: Song,
}

impl Arranged {
    pub fn seconds(&self) -> f64 {
        self.length * 60.0 / self.qpm
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct MusicState {
    pub songs: Vec<usize>,
    pub intro: Option<usize>,
    pub crossfade: f32,
    pub shuffle: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Stinger {
    pub song: usize,
    pub duck: f32,
    pub gain: f32,
}

/// Parsed `music.toml` with every song arranged.
#[derive(Clone, Debug, PartialEq)]
pub struct MusicBank {
    pub songs: Vec<Arc<Arranged>>,
    pub index: BTreeMap<String, usize>,
    pub states: BTreeMap<String, MusicState>,
    pub stingers: BTreeMap<String, Stinger>,
    pub crossfade: f32,
    pub insts: Vec<Vec<Layer>>,
}

impl MusicBank {
    /// `songs` = (name, ABC text); a name may carry a folder and the extension
    /// ("shanties/drunken_sailor.abc" is "drunken_sailor"). Songs without an entry in
    /// `music.toml` get a default arrangement (accordion, bass, chords, bodhrán).
    pub fn parse(music_toml: &str, songs: &[(String, String)]) -> anyhow::Result<MusicBank> {
        let f: MusicFile = toml::from_str(music_toml).context("music.toml")?;
        let stem = |n: &str| {
            let n = n.rsplit(['/', '\\']).next().unwrap_or(n);
            n.strip_suffix(".abc").unwrap_or(n).to_string()
        };
        let songs: Vec<(String, String)> = songs.iter().map(|(n, t)| (stem(n), t.clone())).collect();
        let songs = &songs[..];
        let mut specs = f.song.clone();
        for (name, _) in songs {
            if !specs.contains_key(name) && !f.song.values().any(|s| s.abc.as_deref() == Some(name)) {
                specs.insert(name.clone(), default_spec());
            }
        }
        let mut out = MusicBank {
            songs: Vec::new(),
            index: BTreeMap::new(),
            states: BTreeMap::new(),
            stingers: BTreeMap::new(),
            crossfade: f.crossfade.unwrap_or(2.0),
            insts: Inst::ALL.iter().map(|i| i.layers()).collect(),
        };
        for (name, spec) in &specs {
            let abc_name = spec.abc.clone().unwrap_or_else(|| name.clone());
            let text = songs
                .iter()
                .find(|(n, _)| *n == abc_name)
                .map(|(_, t)| t)
                .with_context(|| format!("song {name}: no ABC named {abc_name:?}"))?;
            let song = abc::parse(text).with_context(|| format!("song {name} ({abc_name}.abc)"))?;
            let arr = arrange(name, spec, song).with_context(|| format!("song {name}"))?;
            out.index.insert(name.clone(), out.songs.len());
            out.songs.push(Arc::new(arr));
        }
        let find = |out: &MusicBank, s: &str, what: &str| -> anyhow::Result<usize> {
            out.index.get(s).copied().with_context(|| format!("{what}: no song {s:?}"))
        };
        for (name, st) in &f.state {
            if st.songs.is_empty() && st.intro.is_none() {
                bail!("state {name}: no songs");
            }
            let songs = st.songs.iter().map(|s| find(&out, s, &format!("state {name}"))).collect::<anyhow::Result<Vec<_>>>()?;
            let intro = st.intro.as_ref().map(|s| find(&out, s, &format!("state {name}"))).transpose()?;
            out.states.insert(
                name.clone(),
                MusicState { songs, intro, crossfade: st.crossfade.unwrap_or(out.crossfade), shuffle: st.shuffle },
            );
        }
        for (name, s) in &f.stinger {
            let song = find(&out, &s.song, &format!("stinger {name}"))?;
            out.stingers.insert(name.clone(), Stinger { song, duck: s.duck.clamp(0.0, 1.0), gain: s.gain });
        }
        Ok(out)
    }

    pub fn song(&self, name: &str) -> Option<&Arc<Arranged>> {
        self.index.get(name).map(|&i| &self.songs[i])
    }
    pub fn layers(&self, inst: Inst) -> &[Layer] {
        &self.insts[inst as usize]
    }
}

fn default_spec() -> SongSpec {
    let part =
        |inst: &str, role: Role, gain: f32, pan: f32| PartSpec { inst: inst.into(), role, gain, pan, ..Default::default() };
    SongSpec {
        part: vec![
            part("accordion", Role::Melody, 1.0, -0.1),
            part("squeeze", Role::Chords, 0.8, 0.25),
            part("bass", Role::Bass, 0.9, 0.0),
            PartSpec { stem: 0.4, ..part("bodhran", Role::Drums, 0.8, 0.1) },
        ],
        ..Default::default()
    }
}

// ---------------------------------------------------------------- arranging

fn parse_ranges(s: &str) -> anyhow::Result<Vec<(u32, u32)>> {
    let mut out = Vec::new();
    for p in s.split(',').map(str::trim).filter(|p| !p.is_empty()) {
        let (a, b) = match p.split_once('-') {
            Some((a, b)) => (a.trim().parse::<u32>()?, b.trim().parse::<u32>()?),
            None => {
                let a = p.parse::<u32>()?;
                (a, a)
            }
        };
        out.push((a.min(b), a.max(b)));
    }
    Ok(out)
}

fn in_ranges(r: &[(u32, u32)], bar: u32) -> bool {
    r.iter().any(|&(a, b)| bar >= a && bar <= b)
}

struct Section {
    ranges: Vec<(u32, u32)>,
    role: String,
    vowel: Option<Formant>,
}

fn vowel(s: &str) -> anyhow::Result<Formant> {
    Formant::parse(s).with_context(|| format!("unknown vowel {s:?} (ah eh ee oh oo uh mm)"))
}

/// Vowel (with glide target) and leading consonant of a sung syllable.
pub fn syllable_sound(s: &str) -> (Formant, Formant, Consonant) {
    let w: String = s.chars().filter(|c| c.is_ascii_alphabetic()).map(|c| c.to_ascii_lowercase()).collect();
    if w.is_empty() {
        return (Formant::Ah, Formant::Ah, Consonant::None);
    }
    let b = w.as_bytes();
    let is_v = |c: u8| matches!(c, b'a' | b'e' | b'i' | b'o' | b'u');
    let vstart = b.iter().position(|&c| is_v(c)).or_else(|| b.iter().position(|&c| c == b'y'));
    let onset = &w[..vstart.unwrap_or(w.len())];
    let cons = if onset.starts_with("sh") || onset.starts_with("ch") {
        Consonant::Sh
    } else if onset.starts_with("th") || onset.starts_with('f') || onset.starts_with('v') {
        Consonant::F
    } else if onset.starts_with('s') || onset.starts_with('z') {
        Consonant::S
    } else if onset.starts_with('t') || onset.starts_with('d') {
        Consonant::T
    } else if onset.starts_with('k') || onset.starts_with('c') || onset.starts_with('g') || onset.starts_with('q') {
        Consonant::K
    } else if onset.starts_with('p') || onset.starts_with('b') {
        Consonant::P
    } else if onset.starts_with('h') || onset.starts_with("wh") {
        Consonant::H
    } else {
        Consonant::None
    };
    let Some(v0) = vstart else { return (Formant::Ee, Formant::Ee, cons) };
    let rest = &w[v0..];
    let after = |n: usize| rest.as_bytes().get(n).copied().unwrap_or(b' ');
    let magic_e = |n: usize| !is_v(after(n)) && after(n) != b' ' && after(n + 1) == b'e' && after(n + 2) == b' ';
    use Formant::*;
    let (a, z) = if w == "you" || w == "to" || w == "do" || w == "who" {
        (Oo, Oo)
    } else if w == "the" || w == "a" || w == "of" {
        (Uh, Uh)
    } else if w == "shall" || w == "and" || w == "an" {
        (Ah, Ah)
    } else if rest == "e" && w.len() <= 3 {
        // we, he, me, she, be
        (Ee, Ee)
    } else if rest.starts_with("ee") || rest.starts_with("ea") || rest.starts_with("ie") {
        (Ee, Ee)
    } else if rest.starts_with("ai") || rest.starts_with("ay") || rest.starts_with("ei") || rest.starts_with("ey") {
        (Eh, Ee)
    } else if rest.starts_with("oo") {
        (Oo, Oo)
    } else if rest.starts_with("ou") || rest.starts_with("ow") {
        if rest.starts_with("ow") && rest.len() == 2 && w.len() > 3 { (Oh, Oo) } else { (Ah, Oo) }
    } else if rest.starts_with("oi") || rest.starts_with("oy") {
        (Oh, Ee)
    } else if rest.starts_with("au") || rest.starts_with("aw") || rest.starts_with("al") {
        (Oh, Oh)
    } else if rest.starts_with("er") || rest.starts_with("ur") || rest.starts_with("ir") {
        (Uh, Uh)
    } else {
        match rest.as_bytes()[0] {
            b'a' => {
                if magic_e(1) {
                    (Eh, Ee)
                } else {
                    (Ah, Ah)
                }
            }
            b'e' => (Eh, Eh),
            b'i' => {
                if magic_e(1) || rest.starts_with("igh") {
                    (Ah, Ee)
                } else {
                    (Ee, Eh)
                }
            }
            b'o' => {
                if magic_e(1) {
                    (Oh, Oo)
                } else {
                    (Oh, Oh)
                }
            }
            b'u' => (Uh, Uh),
            _ => (Ah, Ee), // "y" as in "my"
        }
    };
    (a, z, cons)
}

struct Ctx<'a> {
    song: &'a Song,
    qpm: f64,
    rng: Rng,
    events: Vec<Ev>,
}

impl Ctx<'_> {
    fn chord_at(&self, t: f64) -> Chord {
        self.song.chord_at(t).unwrap_or_else(|| {
            let k = self.song.key;
            Chord {
                root: k.tonic,
                quality: if k.mode.is_minor() { abc::Quality::Minor } else { abc::Quality::Major },
                bass: None,
            }
        })
    }
    fn secs_to_beats(&self, s: f64) -> f64 {
        s * self.qpm / 60.0
    }
    fn jitter(&mut self, h: f32, ms: f32) -> f64 {
        let r = self.rng.bi();
        self.secs_to_beats((r * ms * h) as f64 / 1000.0)
    }
}

/// Places a pitch class in [low, low + 12).
fn place(pc: i32, low: i32) -> i32 {
    low + (pc - low).rem_euclid(12)
}

fn voicing(c: &Chord, low: i32) -> Vec<i32> {
    let mut v: Vec<i32> = c.pcs().iter().map(|&pc| place(pc, low)).collect();
    v.sort();
    v.truncate(4);
    v
}

fn guitar_voicing(c: &Chord) -> Vec<i32> {
    let bass = place(c.bass.unwrap_or(c.root), 40);
    let mut v = vec![bass];
    let mut upper: Vec<i32> = c.pcs().iter().map(|&pc| place(pc, bass + 5)).collect();
    upper.push(place(c.root, bass + 10));
    upper.sort();
    upper.dedup();
    v.extend(upper.into_iter().take(4));
    v
}

fn scale_step_below(m: i32, key: &Key, steps: usize) -> i32 {
    let pcs = key.scale_pcs();
    let mut x = m;
    let mut k = 0;
    while k < steps {
        x -= 1;
        if pcs.contains(&x.rem_euclid(12)) {
            k += 1;
        }
        if m - x > 12 {
            break;
        }
    }
    x
}

fn harmony_below(m: i32, chord: &Chord, key: &Key) -> i32 {
    let pcs = chord.pcs();
    for d in 3..=9 {
        if pcs.contains(&(m - d).rem_euclid(12)) && pcs.contains(&m.rem_euclid(12)) {
            return m - d;
        }
    }
    scale_step_below(m, key, 2)
}

fn fifth_below(m: i32, chord: &Chord, key: &Key) -> i32 {
    let pcs = chord.pcs();
    let f = m - 7;
    if pcs.contains(&f.rem_euclid(12)) || !pcs.contains(&m.rem_euclid(12)) {
        if key.scale_pcs().contains(&f.rem_euclid(12)) {
            return f;
        }
        return scale_step_below(m, key, 4);
    }
    for d in [8, 6, 9, 5, 4, 3] {
        if pcs.contains(&(m - d).rem_euclid(12)) {
            return m - d;
        }
    }
    f
}

fn default_pattern(role: Role, inst: Inst, meter: (u32, u32)) -> &'static str {
    let m = match meter {
        (4, 4) | (2, 2) | (4, 2) => 0,
        (2, 4) => 1,
        (3, 4) | (3, 8) | (3, 2) => 2,
        (6, 8) | (6, 4) => 3,
        (9, 8) => 4,
        (12, 8) => 5,
        _ => 0,
    };
    use Inst::*;
    match role {
        Role::Bass => ["R...F...", "R.F.", "R.....", "R..F..", "R..F..R..", "R..F..R..F.."][m],
        Role::Arp => ["1.3.5.3.", "1353", "1.3.5.", "135535", "135535135", "135535135535"][m],
        Role::Chords => match inst {
            Guitar => ["x.x.xux.", "x.xu", "x.x.x.", "x.ux.u", "x.ux.ux.u", "x.ux.ux.ux.u"][m],
            Choir | Hum | Drone | Horn | Cello | Fiddle | Solo => {
                ["x_______", "x___", "x_____", "x_____", "x________", "x___________"][m]
            }
            _ => ["..x...x.", "..x.", "..x.x.", ".xx.xx", ".xx.xx.xx", ".xx.xx.xx.xx"][m],
        },
        _ => match inst {
            Clap => ["..x...x.", "..x.", "..x.x.", "...x..", "...x.....", "...x.....x.."][m],
            Bodhran => ["X.x.X.xo", "X.xo", "X.xox.", "XoxXox", "XoxXoxXox", "XoxXoxXoxXox"][m],
            Snare => ["..X...X.", "..X.", "..x.x.", "...X..", "...X..X..", "...X.....X.."][m],
            Tick => ["x.x.x.x.", "xxxx", "x.x.x.", "xxxxxx", "xxxxxxxxx", "xxxxxxxxxxxx"][m],
            Shout => ["......X.", "...X", "....X.", "...X..", "......X..", ".........X.."][m],
            _ => ["X...X...", "X...", "X.....", "X..X..", "X..X..X..", "X..x..X..x.."][m],
        },
    }
}

/// Builds the event list of one song.
pub fn arrange(name: &str, spec: &SongSpec, song: Song) -> anyhow::Result<Arranged> {
    let qpm = spec.tempo.map(|t| t * song.beat).unwrap_or(song.bpm).max(10.0);
    let passes = spec.passes.max(1);
    let pass_beats = song.length;
    if pass_beats <= 0.0 {
        bail!("the tune has no notes");
    }
    let bar_full = song.bar_beats();
    let sections: Vec<Section> = spec
        .section
        .iter()
        .map(|s| {
            Ok(Section {
                ranges: parse_ranges(&s.bars)?,
                role: s.role.clone(),
                vowel: s.vowel.as_deref().map(vowel).transpose()?,
            })
        })
        .collect::<anyhow::Result<_>>()?;
    let section_at = |bar: u32| sections.iter().find(|s| in_ranges(&s.ranges, bar));
    let mut parts = Vec::new();
    let mut ctx = Ctx { song: &song, qpm, rng: Rng::new(hash_str(name)), events: Vec::new() };
    let key = song.key;
    for (pi, p) in spec.part.iter().enumerate() {
        let inst = Inst::parse(&p.inst).with_context(|| {
            format!("part {}: unknown instrument {:?} (one of {})", pi + 1, p.inst, Inst::ALL.map(|i| i.name()).join(" "))
        })?;
        let (rv, ec) = inst.sends();
        parts.push(PartRt {
            inst,
            role: p.role,
            stem: p.stem,
            stem_max: p.stem_max.unwrap_or(2.0),
            gain: p.gain * spec.gain,
            reverb: p.reverb.unwrap_or(rv),
            echo: p.echo.unwrap_or(ec),
        });
        let bars = p.bars.as_deref().map(parse_ranges).transpose()?;
        let bar_ok = |bar: u32| {
            bars.as_ref().is_none_or(|r| in_ranges(r, bar))
                && (p.sections.is_empty()
                    || p.sections.iter().any(|s| section_at(bar).map(|x| &x.role == s).unwrap_or(s == "verse")))
        };
        let h = p.humanize.unwrap_or(1.0) * spec.humanize;
        let tr = spec.transpose + p.transpose + 12 * p.octave;
        let part = pi as u16;
        let pat = p.pattern.clone().unwrap_or_else(|| default_pattern(p.role, inst, song.meter).to_string());
        let steps: Vec<char> = pat.chars().filter(|c| !c.is_whitespace() && *c != '|').collect();
        let default_vowel = p.vowel.as_deref().map(vowel).transpose()?.unwrap_or(Formant::Ah);
        let ev = |start: f64, len: f64, inst: Inst, notes: &[f32], vel: f32, pan: f32| Ev {
            start: start.max(0.0),
            len: len.max(0.02),
            inst,
            part,
            notes: {
                let mut a = [0.0; 4];
                for (i, n) in notes.iter().take(4).enumerate() {
                    a[i] = *n;
                }
                a
            },
            n: notes.len().clamp(1, 4) as u8,
            vel: vel * p.vel,
            pan,
            vowel: (default_vowel, default_vowel),
            cons: Consonant::None,
            shift: 1.0,
        };
        let accent = |t: f64, bar_start: f64| -> f32 {
            let x = t - bar_start;
            if x.abs() < 1e-6 {
                1.0 + p.accent
            } else if (x - bar_full / 2.0).abs() < 1e-6 && bar_full >= 2.0 {
                1.0 + p.accent * 0.5
            } else {
                1.0
            }
        };
        for pass in 0..passes {
            if !p.passes.is_empty() && !p.passes.contains(&(pass + 1)) {
                continue;
            }
            let off = pass as f64 * pass_beats;
            match p.role {
                Role::Melody | Role::Harmony => {
                    let legato = p.legato.unwrap_or(0.94) as f64;
                    for nt in &song.notes {
                        if !bar_ok(nt.bar) || (p.role == Role::Harmony && nt.in_chord) {
                            continue;
                        }
                        let bar_start = song.bars.iter().find(|b| b.number == nt.bar).map(|b| b.start).unwrap_or(0.0);
                        let mut m = nt.midi + tr;
                        if p.role == Role::Harmony {
                            m = harmony_below(m, &ctx.chord_at(nt.start), &key);
                        }
                        let j = ctx.jitter(h, 6.0);
                        let vel = accent(nt.start, bar_start) * (1.0 + ctx.rng.bi() * 0.06 * h);
                        let mut e = ev(off + nt.start + j, nt.len * legato, inst, &[m as f32], vel, p.pan);
                        if inst.sung() {
                            let s = nt.syllable.as_deref().map(syllable_sound);
                            if let Some((a, z, c)) = s {
                                e.vowel = (a, z);
                                e.cons = c;
                            }
                        }
                        ctx.events.push(e);
                    }
                }
                Role::Choir => {
                    let legato = p.legato.unwrap_or(0.98) as f64;
                    for nt in &song.notes {
                        if !bar_ok(nt.bar) || nt.in_chord {
                            continue;
                        }
                        let sec = section_at(nt.bar);
                        let role = sec.map(|s| s.role.as_str()).unwrap_or("response");
                        let base_vowel = sec.and_then(|s| s.vowel).unwrap_or(default_vowel);
                        let (a, z, cons) = match nt.syllable.as_deref() {
                            Some(s) => syllable_sound(s),
                            None => (base_vowel, base_vowel, Consonant::None),
                        };
                        let m = nt.midi + tr;
                        let chord = ctx.chord_at(nt.start);
                        let mut voices: Vec<(Inst, i32, f32, f32, f32)> = Vec::new(); // inst, midi, pan, shift, gain
                        let call = p.call.as_deref().unwrap_or("solo");
                        let chorus_voices =
                            p.voices.clone().unwrap_or_else(|| vec!["unison".into(), "octave".into(), "fifth".into()]);
                        match role {
                            "call" => match call {
                                "rest" => {}
                                "crew" => {
                                    voices.push((Inst::Choir, m, p.pan - 0.15, 0.95, 0.72));
                                    voices.push((Inst::Choir, m, p.pan + 0.15, 1.04, 0.72));
                                }
                                _ => voices.push((Inst::Solo, m, p.pan - 0.05, 1.0, 1.0)),
                            },
                            "chorus" => {
                                for v in &chorus_voices {
                                    match v.as_str() {
                                        "unison" => {
                                            voices.push((Inst::Choir, m, p.pan - 0.15, 0.95, 0.72));
                                            voices.push((Inst::Choir, m, p.pan + 0.15, 1.04, 0.72));
                                        }
                                        // The basses an octave down, but never below F2 (they double the tune there).
                                        "octave" => voices.push((
                                            Inst::Choir,
                                            if m - 12 >= 41 { m - 12 } else { m },
                                            p.pan - 0.3,
                                            0.9,
                                            0.8,
                                        )),
                                        "fifth" => {
                                            voices.push((Inst::Choir, fifth_below(m, &chord, &key), p.pan + 0.3, 1.03, 0.62))
                                        }
                                        "third" => {
                                            voices.push((Inst::Choir, harmony_below(m, &chord, &key), p.pan + 0.25, 1.02, 0.62))
                                        }
                                        "high" => voices.push((Inst::Choir, m + 12, p.pan + 0.15, 1.1, 0.4)),
                                        other => bail!("part {}: unknown choir voice {other:?}", pi + 1),
                                    }
                                }
                            }
                            _ => {
                                voices.push((Inst::Choir, m, p.pan - 0.15, 0.95, 0.72));
                                voices.push((Inst::Choir, m, p.pan + 0.15, 1.04, 0.72));
                            }
                        }
                        let bar_start = song.bars.iter().find(|b| b.number == nt.bar).map(|b| b.start).unwrap_or(0.0);
                        for (vin, vm, pan, shift, g) in voices {
                            let late = if vin == Inst::Choir { ctx.secs_to_beats(0.012) } else { 0.0 };
                            let j = ctx.jitter(h, if vin == Inst::Choir { 9.0 } else { 5.0 }) + late;
                            let vel = g * accent(nt.start, bar_start) * (1.0 + ctx.rng.bi() * 0.07 * h);
                            let mut e = ev(off + nt.start + j, nt.len * legato, vin, &[vm as f32], vel, pan);
                            e.vowel = (a, z);
                            e.cons = if vm == m { cons } else { Consonant::None };
                            e.shift = shift;
                            ctx.events.push(e);
                        }
                    }
                }
                Role::Drone => {
                    let low = p.low.unwrap_or(38);
                    let t = place(key.tonic, low) + spec.transpose + p.transpose + 12 * p.octave;
                    ctx.events.push(ev(off, pass_beats, inst, &[t as f32, (t + 7) as f32], 1.0, p.pan));
                }
                Role::Bass | Role::Chords | Role::Arp | Role::Drums => {
                    if steps.is_empty() {
                        continue;
                    }
                    let n = steps.len();
                    let step = bar_full / n as f64;
                    let legato = p.legato.unwrap_or(match p.role {
                        Role::Bass => 0.9,
                        Role::Arp => 1.6,
                        _ => 0.8,
                    }) as f64;
                    let first = song.bars.first().map(|b| b.number).unwrap_or(0);
                    for bar in &song.bars {
                        if !bar_ok(bar.number) {
                            continue;
                        }
                        let bar_end = bar.start + bar.len;
                        let base = if bar.number == first && bar.len < bar_full - 1e-6 { bar_end - bar_full } else { bar.start };
                        for k in 0..n {
                            let c = steps[k];
                            if matches!(c, '.' | '-' | '_') {
                                continue;
                            }
                            let mut t = base + k as f64 * step;
                            if t < bar.start - 1e-6 || t >= bar_end - 1e-6 {
                                continue;
                            }
                            if k % 2 == 1 {
                                t += p.swing as f64 * step;
                            }
                            let mut held = 1;
                            while k + held < n && steps[k + held] == '_' {
                                held += 1;
                            }
                            // Held hits (x___) sustain into the next one; single steps use the legato.
                            let lg = if held > 1 && p.legato.is_none() { 1.0 } else { legato };
                            let len = step * held as f64 * lg;
                            let chord = ctx.chord_at(t + 1e-6);
                            let acc = accent(base + k as f64 * step, base);
                            let j = ctx.jitter(h, if p.role == Role::Drums { 4.0 } else { 6.0 });
                            let vr = 1.0 + ctx.rng.bi() * 0.06 * h;
                            let t = off + t + j;
                            match p.role {
                                Role::Bass => {
                                    let low = p.low.unwrap_or(40) + 12 * p.octave;
                                    let root = place(chord.bass.unwrap_or(chord.root), low);
                                    let r = place(chord.root, low);
                                    let third = chord.pcs().get(1).copied().unwrap_or(chord.root + 4);
                                    let m = match c.to_ascii_uppercase() {
                                        'R' => root,
                                        'F' => {
                                            if r + 7 < low + 14 {
                                                r + 7
                                            } else {
                                                r - 5
                                            }
                                        }
                                        'O' => r + 12,
                                        'T' => place(third, r),
                                        _ => continue,
                                    } + spec.transpose
                                        + p.transpose;
                                    let vel = if c.is_ascii_lowercase() { 0.7 } else { 1.0 } * acc * vr;
                                    ctx.events.push(ev(t, len, inst, &[m as f32], vel, p.pan));
                                }
                                Role::Chords => {
                                    let vel = match c {
                                        'X' => 1.2,
                                        'u' => 0.75,
                                        _ => 1.0,
                                    } * acc
                                        * vr;
                                    if inst == Inst::Guitar {
                                        let mut v = guitar_voicing(&chord);
                                        if c == 'u' {
                                            v.reverse();
                                            v.truncate(3);
                                        }
                                        let gap = ctx.secs_to_beats(p.strum as f64 / 1000.0);
                                        for (si, m) in v.iter().enumerate() {
                                            let m = m + spec.transpose + p.transpose + 12 * p.octave;
                                            let pan = p.pan + (si as f32 - 2.0) * 0.08;
                                            ctx.events.push(ev(t + gap * si as f64, len, inst, &[m as f32], vel * 0.85, pan));
                                        }
                                    } else {
                                        let low = p.low.unwrap_or(53);
                                        let v: Vec<f32> = voicing(&chord, low)
                                            .iter()
                                            .map(|m| (m + spec.transpose + p.transpose + 12 * p.octave) as f32)
                                            .collect();
                                        let mut e = ev(t, len, inst, &v, vel, p.pan);
                                        if inst.sung() {
                                            e.vowel = (default_vowel, default_vowel);
                                        }
                                        ctx.events.push(e);
                                    }
                                }
                                Role::Arp => {
                                    let Some(d) = c.to_digit(10) else { continue };
                                    let low = p.low.unwrap_or(48) + 12 * p.octave;
                                    let tones = voicing(&chord, place(chord.root, low));
                                    let d = (d.max(1) - 1) as usize;
                                    let m = tones[d % tones.len()] + 12 * (d / tones.len()) as i32 + spec.transpose + p.transpose;
                                    ctx.events.push(ev(t, len, inst, &[m as f32], acc * vr, p.pan));
                                }
                                _ => {
                                    let (vel, semis) = match c {
                                        'X' => (1.2, 0.0),
                                        'o' => (0.5, 4.0),
                                        _ => (0.9, 0.0),
                                    };
                                    let note = if inst.pitched() {
                                        (place(chord.root, p.low.unwrap_or(40)) + spec.transpose + p.transpose + 12 * p.octave)
                                            as f32
                                    } else {
                                        semis
                                    };
                                    ctx.events.push(ev(t, step * 0.9, inst, &[note], vel * vr, p.pan));
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    let mut events = ctx.events;
    events.sort_by(|a, b| a.start.partial_cmp(&b.start).unwrap_or(std::cmp::Ordering::Equal));
    Ok(Arranged {
        name: name.to_string(),
        title: song.title.clone(),
        qpm,
        pass_beats,
        length: pass_beats * passes as f64 + spec.gap.max(0.0),
        bar_beats: bar_full,
        events,
        parts,
        gain: spec.gain,
        song,
    })
}

// ---------------------------------------------------------------- the sequencer

#[derive(Clone, Debug)]
struct Track {
    id: u32,
    song: usize,
    sample: u64,
    cursor: usize,
    fade: f32,
    fade_target: f32,
    /// Fade change per sample.
    fade_rate: f32,
    stinger: bool,
    gain: f32,
    ended: bool,
    /// Samples since it ended (tails ring on).
    since_end: u64,
}

/// A request to start an instrument note (resolved to voices by the mixer).
#[derive(Clone, Copy, Debug)]
pub struct NoteOn {
    pub inst: Inst,
    pub spawn: Spawn,
    /// A drum struck again damps its previous stroke.
    pub choke: bool,
}

/// Plays songs from a [`MusicBank`]: states with playlists, crossfades, stems and stingers.
#[derive(Clone, Debug)]
pub struct MusicPlayer {
    pub bank: Option<Arc<MusicBank>>,
    tracks: Vec<Track>,
    /// The requested state or song, and the playlist it resolved to.
    pub current: Option<String>,
    playlist: Vec<usize>,
    pl_pos: usize,
    crossfade: f32,
    pub intensity: f32,
    intensity_target: f32,
    next_id: u32,
    rng: Rng,
    duck: f32,
    sr: f32,
    pub warned: Vec<String>,
}

impl Default for MusicPlayer {
    fn default() -> Self {
        Self {
            bank: None,
            tracks: Vec::new(),
            current: None,
            playlist: Vec::new(),
            pl_pos: 0,
            crossfade: 2.0,
            intensity: 0.5,
            intensity_target: 0.5,
            next_id: 1,
            rng: Rng::new(0x5eed_5a11),
            duck: 1.0,
            sr: 48000.0,
            warned: Vec::new(),
        }
    }
}

impl MusicPlayer {
    pub fn load(&mut self, bank: Arc<MusicBank>) {
        let cur = self.current.take();
        // Songs index into the old bank: drop them (their voices fade out).
        self.tracks.clear();
        self.bank = Some(bank);
        if let Some(c) = cur {
            let i = self.intensity_target;
            let sr = self.sr;
            self.set(&c, i, sr);
        }
    }

    /// The song playing now (the newest non-stinger track).
    pub fn now_playing(&self) -> Option<&Arc<Arranged>> {
        let bank = self.bank.as_ref()?;
        self.tracks.iter().rev().find(|t| !t.stinger && t.fade_target > 0.0).map(|t| &bank.songs[t.song])
    }

    fn start_track(&mut self, song: usize, fade_in: f32, sr: f32, stinger: bool, gain: f32) {
        let id = self.next_id;
        self.next_id += 1;
        let instant = fade_in <= 0.0;
        self.tracks.push(Track {
            id,
            song,
            sample: 0,
            cursor: 0,
            fade: if instant { 1.0 } else { 0.0 },
            fade_target: 1.0,
            fade_rate: if instant { 1.0 } else { 1.0 / (fade_in * sr) },
            stinger,
            gain,
            ended: false,
            since_end: 0,
        });
        // Bound the work: at most two fading tracks besides the new one.
        let mut fading: Vec<usize> = (0..self.tracks.len()).filter(|&i| self.tracks[i].fade_target == 0.0).collect();
        while fading.len() > 2 {
            self.tracks.remove(fading[0]);
            fading = (0..self.tracks.len()).filter(|&i| self.tracks[i].fade_target == 0.0).collect();
        }
    }

    fn fade_out_all(&mut self, secs: f32, sr: f32) {
        for t in &mut self.tracks {
            if !t.stinger {
                t.fade_target = 0.0;
                t.fade_rate = 1.0 / (secs.max(0.05) * sr);
            }
        }
    }

    /// Switches to a state or song (crossfading), or just updates the intensity.
    pub fn set(&mut self, name: &str, intensity: f32, sr: f32) {
        self.intensity_target = intensity.clamp(0.0, 1.0);
        if self.current.as_deref() == Some(name) {
            return;
        }
        let Some(bank) = self.bank.clone() else {
            self.current = Some(name.to_string());
            return;
        };
        let (playlist, intro, xf, shuffle) = if let Some(st) = bank.states.get(name) {
            (st.songs.clone(), st.intro, st.crossfade, st.shuffle)
        } else if let Some(&i) = bank.index.get(name) {
            (vec![i], None, bank.crossfade, false)
        } else {
            if !self.warned.iter().any(|w| w == name) {
                log::warn!("music: no state or song {name:?}");
                self.warned.push(name.to_string());
            }
            return;
        };
        self.current = Some(name.to_string());
        let mut playlist = playlist;
        if shuffle {
            for i in (1..playlist.len()).rev() {
                let j = self.rng.below(i + 1);
                playlist.swap(i, j);
            }
        }
        let silent = self.tracks.iter().all(|t| t.stinger || t.fade_target == 0.0 || t.fade < 0.01);
        self.fade_out_all(xf, sr);
        self.crossfade = xf;
        self.playlist = playlist;
        self.pl_pos = 0;
        let first = match intro {
            Some(i) => {
                self.pl_pos = usize::MAX;
                i
            }
            None => self.playlist[0],
        };
        self.start_track(first, if silent { 0.0 } else { (xf * 0.5).min(1.0) }, sr, false, 1.0);
    }

    pub fn stop(&mut self, sr: f32) {
        if self.current.is_some() {
            self.current = None;
            let xf = self.crossfade;
            self.fade_out_all(xf, sr);
        }
    }

    /// Next (+1) or previous (-1) song in the current playlist.
    pub fn skip(&mut self, delta: i32, sr: f32) {
        if self.playlist.is_empty() || self.bank.is_none() {
            return;
        }
        let n = self.playlist.len() as i64;
        let cur = if self.pl_pos == usize::MAX { -1 } else { self.pl_pos as i64 };
        self.pl_pos = (cur + delta as i64).rem_euclid(n) as usize;
        self.fade_out_all(1.0, sr);
        let s = self.playlist[self.pl_pos];
        self.start_track(s, 0.3, sr, false, 1.0);
    }

    pub fn stinger(&mut self, name: &str, sr: f32) -> bool {
        let Some(bank) = self.bank.clone() else { return false };
        let Some(s) =
            bank.stingers.get(name).cloned().or_else(|| bank.index.get(name).map(|&song| Stinger { song, duck: 0.5, gain: 1.0 }))
        else {
            return false;
        };
        self.start_track(s.song, 0.0, sr, true, s.gain);
        self.duck = 1.0 - s.duck;
        true
    }

    /// Gain of a part of a track right now (None = the track is gone).
    pub fn group_gain(&self, track: u32, part: u16) -> Option<f32> {
        let t = self.tracks.iter().find(|t| t.id == track)?;
        let bank = self.bank.as_ref()?;
        let song = &bank.songs[t.song];
        let p = song.parts.get(part as usize)?;
        let duck = if t.stinger { 1.0 } else { self.duck_now() };
        Some(t.fade * t.gain * p.stem_gain(self.intensity) * duck)
    }

    fn duck_now(&self) -> f32 {
        if self.tracks.iter().any(|t| t.stinger && !t.ended) { self.duck } else { 1.0 }
    }

    /// Advances `n` samples, appending the notes that start in this block.
    pub fn process(&mut self, n: usize, sr: f32, out: &mut Vec<NoteOn>) {
        let Some(bank) = self.bank.clone() else { return };
        self.sr = sr;
        let k = 1.0 - (-(n as f32) / (sr * 1.2)).exp();
        self.intensity += (self.intensity_target - self.intensity) * k;
        let mut next_song = false;
        for t in &mut self.tracks {
            let step = t.fade_rate * n as f32;
            if t.fade < t.fade_target {
                t.fade = (t.fade + step).min(t.fade_target);
            } else if t.fade > t.fade_target {
                t.fade = (t.fade - step).max(t.fade_target);
            }
            let song = &bank.songs[t.song];
            let spb = sr as f64 * 60.0 / song.qpm;
            let end = t.sample + n as u64;
            if t.fade_target > 0.0 || t.fade > 0.0 {
                while t.cursor < song.events.len() {
                    let e = &song.events[t.cursor];
                    let s = (e.start * spb).round() as u64;
                    if s >= end {
                        break;
                    }
                    t.cursor += 1;
                    let part = &song.parts[e.part as usize];
                    let g = part.stem_gain(self.intensity);
                    if g < 0.01 || t.fade_target == 0.0 {
                        continue;
                    }
                    let delay = s.saturating_sub(t.sample) as u32;
                    let gate = (e.len * spb).max(1.0) as u32;
                    out.push(note_on(e, part, t.id, delay, gate, (t.sample as f64 / spb * 60.0 / song.qpm) as f32));
                }
            }
            t.sample = end;
            let total = (song.length * spb) as u64;
            if !t.ended && t.sample >= total {
                t.ended = true;
                if !t.stinger && t.fade_target > 0.0 {
                    next_song = true;
                    t.fade_target = 0.0;
                    t.fade_rate = 1.0 / (4.0 * sr);
                }
            }
            if t.ended {
                t.since_end += n as u64;
            }
        }
        self.tracks.retain(|t| !(t.ended && t.since_end > (sr * 6.0) as u64) && !(t.fade_target == 0.0 && t.fade <= 0.0));
        if next_song && !self.playlist.is_empty() {
            self.pl_pos = if self.pl_pos == usize::MAX { 0 } else { (self.pl_pos + 1) % self.playlist.len() };
            let s = self.playlist[self.pl_pos];
            self.start_track(s, 0.0, sr, false, 1.0);
        }
    }
}

fn note_on(e: &Ev, part: &PartRt, track: u32, delay: u32, gate: u32, song_time: f32) -> NoteOn {
    let pitched = e.inst.pitched();
    let base = if pitched { midi_hz(e.notes[0]) } else { 2f32.powf(e.notes[0] / 12.0) };
    let mut chord = [1.0f32; 4];
    for (c, n) in chord.iter_mut().zip(e.notes.iter()).take(e.n as usize).skip(1) {
        *c = midi_hz(*n) / base;
    }
    let mut s = Spawn::new(Layer::default());
    s.pitch = base;
    s.gain = e.vel * part.gain;
    s.pan = e.pan;
    s.delay = delay;
    s.gate = if (pitched || e.inst.sung()) && !e.inst.rings() { Some(gate) } else { None };
    s.bus = Bus::Music;
    s.group = Group::Music { track, part: e.part };
    s.chord = chord;
    s.nchord = e.n;
    if e.inst.sung() {
        s.vowel = Some(e.vowel);
        s.consonant = e.cons;
        s.shift = e.shift;
    }
    s.reverb = part.reverb;
    s.echo = part.echo;
    s.lfo_time = song_time;
    NoteOn { inst: e.inst, spawn: s, choke: part.role == Role::Drums }
}
