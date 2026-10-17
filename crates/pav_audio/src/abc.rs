//! An ABC-notation subset for songs (<https://abcnotation.com/wiki/abc:standard:v2.1>).
//!
//! Supported: headers `X: T: C: M: L: Q: K:` (major, minor and the modes, with their key
//! signatures, plus explicit extra accidentals like `K:D ^g`); body fields on their own line
//! or inline (`[K:G]`, `M: L: Q: K: w: W: P:`); notes `A-G a-g` with `'` and `,`; accidentals
//! `^ _ = ^^ __` (they last to the end of the bar, for that pitch and octave); lengths `2 /2 /
//! // 3/2 3/`; broken rhythm `> < >> <<`; rests `z x` and bar rests `Z4`; chords `[CEG]`; bars
//! `| || |] [| |: :| :: :|:`; first and second endings `[1 [2 |1 :|2`; ties `-`; tuplets
//! `(3 (p:q:r`; chord symbols `"Am" "G7" "D/F#"` (annotations `"^text"` are skipped); lyric
//! lines `w:` aligned to the notes of the line above (`-` splits syllables, `_` holds, `*`
//! skips, `~` joins, `|` jumps to the next bar; tied continuation notes and rests get no
//! syllable; a second `w:` line under the same music is the second time through a repeat).
//! Skipped: grace notes `{...}`, decorations `!x! +x+ . ~ H T u v ...`, slurs, comments `%`.
//!
//! Output times are in quarter-note beats; `bpm` is quarter notes per minute.

use anyhow::{Context, bail};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Major,
    Minor,
    Dorian,
    Phrygian,
    Lydian,
    Mixolydian,
    Locrian,
}

impl Mode {
    /// Offset in fifths from the major key on the same tonic.
    fn fifths(self) -> i32 {
        match self {
            Mode::Major => 0,
            Mode::Lydian => 1,
            Mode::Mixolydian => -1,
            Mode::Dorian => -2,
            Mode::Minor => -3,
            Mode::Phrygian => -4,
            Mode::Locrian => -5,
        }
    }
    /// Semitone steps of the scale from the tonic.
    pub fn scale(self) -> [i32; 7] {
        let major = [0, 2, 4, 5, 7, 9, 11];
        let rot = match self {
            Mode::Major => 0,
            Mode::Dorian => 1,
            Mode::Phrygian => 2,
            Mode::Lydian => 3,
            Mode::Mixolydian => 4,
            Mode::Minor => 5,
            Mode::Locrian => 6,
        };
        let mut out = [0; 7];
        for (i, o) in out.iter_mut().enumerate() {
            let a = major[(i + rot) % 7] + if i + rot >= 7 { 12 } else { 0 };
            *o = a - major[rot];
        }
        out
    }
    pub fn is_minor(self) -> bool {
        matches!(self, Mode::Minor | Mode::Dorian | Mode::Phrygian | Mode::Locrian)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Key {
    /// Pitch class of the tonic (C = 0).
    pub tonic: i32,
    pub mode: Mode,
    /// Alteration in semitones for each letter C D E F G A B.
    pub acc: [i32; 7],
}

impl Default for Key {
    fn default() -> Self {
        Key { tonic: 0, mode: Mode::Major, acc: [0; 7] }
    }
}

const LETTER_PC: [i32; 7] = [0, 2, 4, 5, 7, 9, 11];

fn letter_index(c: char) -> Option<usize> {
    Some(match c.to_ascii_uppercase() {
        'C' => 0,
        'D' => 1,
        'E' => 2,
        'F' => 3,
        'G' => 4,
        'A' => 5,
        'B' => 6,
        _ => return None,
    })
}

impl Key {
    pub fn parse(s: &str) -> anyhow::Result<Key> {
        let s = s.trim();
        let mut words = s.split_whitespace();
        let first = words.next().unwrap_or("C");
        if first.eq_ignore_ascii_case("none") {
            return Ok(Key::default());
        }
        let mut ch = first.chars().peekable();
        let letter = ch.next().context("empty key")?;
        let li = letter_index(letter).with_context(|| format!("bad key {s:?}"))?;
        let mut tonic = LETTER_PC[li];
        let mut sharp_flat = 0;
        if let Some(&c) = ch.peek() {
            if c == '#' {
                sharp_flat = 1;
                ch.next();
            } else if c == 'b' {
                sharp_flat = -1;
                ch.next();
            }
        }
        tonic = (tonic + sharp_flat).rem_euclid(12);
        let rest: String = ch.collect();
        let mut mode_word = rest.to_ascii_lowercase();
        let mut extra = Vec::new();
        for w in words {
            if w.starts_with(['^', '_', '=']) {
                extra.push(w.to_string());
            } else if mode_word.is_empty() {
                mode_word = w.to_ascii_lowercase();
            }
        }
        let mode = match mode_word.as_str() {
            "" => Mode::Major,
            m if m.starts_with("maj") || m.starts_with("ion") => Mode::Major,
            "m" => Mode::Minor,
            m if m.starts_with("min") || m.starts_with("aeo") => Mode::Minor,
            m if m.starts_with("dor") => Mode::Dorian,
            m if m.starts_with("phr") => Mode::Phrygian,
            m if m.starts_with("lyd") => Mode::Lydian,
            m if m.starts_with("mix") => Mode::Mixolydian,
            m if m.starts_with("loc") => Mode::Locrian,
            m => bail!("unknown mode {m:?} in key {s:?}"),
        };
        // Major-key fifths of the tonic spelled with this letter.
        const MAJOR_FIFTHS: [i32; 7] = [0, 2, 4, -1, 1, 3, 5]; // C D E F G A B
        let fifths = MAJOR_FIFTHS[li] + 7 * sharp_flat + mode.fifths();
        let mut acc = [0; 7];
        const SHARPS: [usize; 7] = [3, 0, 4, 1, 5, 2, 6]; // F C G D A E B
        if fifths > 0 {
            for &l in SHARPS.iter().take(fifths.min(7) as usize) {
                acc[l] = 1;
            }
            if fifths > 7 {
                for &l in SHARPS.iter().take((fifths - 7) as usize) {
                    acc[l] = 2;
                }
            }
        } else if fifths < 0 {
            for &l in SHARPS.iter().rev().take((-fifths).min(7) as usize) {
                acc[l] = -1;
            }
            if fifths < -7 {
                for &l in SHARPS.iter().rev().take((-fifths - 7) as usize) {
                    acc[l] = -2;
                }
            }
        }
        for e in extra {
            let a = match &e[..1] {
                "^" => 1,
                "_" => -1,
                _ => 0,
            };
            if let Some(l) = e.chars().last().and_then(letter_index) {
                acc[l] = a;
            }
        }
        Ok(Key { tonic, mode, acc })
    }
    /// Pitch classes of the scale.
    pub fn scale_pcs(&self) -> [i32; 7] {
        self.mode.scale().map(|s| (self.tonic + s).rem_euclid(12))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Quality {
    Major,
    Minor,
    Dom7,
    Maj7,
    Min7,
    Dim,
    Dim7,
    HalfDim,
    Aug,
    Sus2,
    Sus4,
    Six,
    Min6,
    Power,
}

impl Quality {
    pub fn intervals(self) -> &'static [i32] {
        match self {
            Quality::Major => &[0, 4, 7],
            Quality::Minor => &[0, 3, 7],
            Quality::Dom7 => &[0, 4, 7, 10],
            Quality::Maj7 => &[0, 4, 7, 11],
            Quality::Min7 => &[0, 3, 7, 10],
            Quality::Dim => &[0, 3, 6],
            Quality::Dim7 => &[0, 3, 6, 9],
            Quality::HalfDim => &[0, 3, 6, 10],
            Quality::Aug => &[0, 4, 8],
            Quality::Sus2 => &[0, 2, 7],
            Quality::Sus4 => &[0, 5, 7],
            Quality::Six => &[0, 4, 7, 9],
            Quality::Min6 => &[0, 3, 7, 9],
            Quality::Power => &[0, 7],
        }
    }
}

/// A chord symbol: root pitch class, quality, optional slash bass.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Chord {
    pub root: i32,
    pub quality: Quality,
    pub bass: Option<i32>,
}

impl Chord {
    pub fn parse(s: &str) -> Option<Chord> {
        let s = s.trim();
        let mut it = s.chars().peekable();
        let li = letter_index(*it.peek()?)?;
        if !it.next()?.is_ascii_uppercase() {
            return None;
        }
        let mut root = LETTER_PC[li];
        match it.peek() {
            Some('#') => {
                root += 1;
                it.next();
            }
            Some('b') => {
                root -= 1;
                it.next();
            }
            _ => {}
        }
        let rest: String = it.collect();
        let (q, bass) = match rest.split_once('/') {
            Some((q, b)) => (q.to_string(), Chord::parse(b).map(|c| c.root)),
            None => (rest, None),
        };
        let quality = match q.as_str() {
            "" | "maj" | "M" => Quality::Major,
            "m" | "min" | "-" => Quality::Minor,
            "7" | "9" | "13" | "7sus4" => Quality::Dom7,
            "maj7" | "M7" | "maj9" => Quality::Maj7,
            "m7" | "min7" | "-7" | "m9" => Quality::Min7,
            "dim" | "o" => Quality::Dim,
            "dim7" | "o7" => Quality::Dim7,
            "m7b5" | "ø" => Quality::HalfDim,
            "aug" | "+" => Quality::Aug,
            "sus2" => Quality::Sus2,
            "sus4" | "sus" => Quality::Sus4,
            "6" => Quality::Six,
            "m6" => Quality::Min6,
            "5" => Quality::Power,
            _ => return None,
        };
        Some(Chord { root: root.rem_euclid(12), quality, bass })
    }
    pub fn pcs(&self) -> Vec<i32> {
        self.quality.intervals().iter().map(|i| (self.root + i).rem_euclid(12)).collect()
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Note {
    /// Quarter-note beats from the start.
    pub start: f64,
    pub len: f64,
    pub midi: i32,
    /// Bar number in the performance (after repeats; a pickup bar is 0).
    pub bar: u32,
    pub syllable: Option<String>,
    /// Several notes sounding together (`[CEG]`): all but the first carry this.
    pub in_chord: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ChordEvent {
    pub start: f64,
    pub chord: Chord,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Bar {
    pub start: f64,
    pub len: f64,
    pub number: u32,
}

/// A parsed, repeat-expanded tune.
#[derive(Clone, Debug, PartialEq)]
pub struct Song {
    pub title: String,
    pub composer: String,
    pub meter: (u32, u32),
    /// Default note length (fraction of a whole note).
    pub unit: f64,
    /// Quarter notes per minute.
    pub bpm: f64,
    /// The beat Q: counts in, in quarter notes (dotted quarter = 1.5).
    pub beat: f64,
    pub key: Key,
    pub notes: Vec<Note>,
    pub chords: Vec<ChordEvent>,
    pub bars: Vec<Bar>,
    /// Total length in beats.
    pub length: f64,
    /// Lyric lines as written (`w:` and `W:`).
    pub lyrics: Vec<String>,
}

impl Song {
    /// Quarter-note beats in a full bar.
    pub fn bar_beats(&self) -> f64 {
        self.meter.0 as f64 * 4.0 / self.meter.1 as f64
    }
    pub fn chord_at(&self, t: f64) -> Option<Chord> {
        let i = self.chords.partition_point(|c| c.start <= t + 1e-9);
        if i == 0 { self.chords.first().map(|c| c.chord) } else { Some(self.chords[i - 1].chord) }
    }
    pub fn seconds(&self) -> f64 {
        self.length * 60.0 / self.bpm
    }
}

// ---------------------------------------------------------------- tokens

#[derive(Clone, Debug, PartialEq)]
enum BarKind {
    Plain,
    RepeatStart,
    RepeatEnd,
    RepeatBoth,
}

#[derive(Clone, Debug)]
enum Tok {
    Note {
        midis: Vec<i32>,
        len: f64,
        tie: bool,
        /// Index into `Parser::sylls` (lyrics per verse).
        id: usize,
    },
    Rest {
        len: f64,
    },
    Chord(Chord),
    Bar(BarKind),
    Ending(u32),
}

struct Parser {
    unit: f64,
    meter: (u32, u32),
    key: Key,
    bpm: f64,
    beat: f64,
    toks: Vec<Tok>,
    /// Per note id: syllable per verse.
    sylls: Vec<Vec<Option<String>>>,
    /// Note ids on the current music line, and the bar index of each.
    line_notes: Vec<(usize, u32)>,
    line_verse: usize,
    bar_in_line: u32,
    bar_acc: Vec<((usize, i32), i32)>,
    tuplet: Option<(f64, u32)>,
    broken_next: f64,
    lyrics: Vec<String>,
}

fn parse_frac(s: &str) -> Option<f64> {
    let (a, b) = s.trim().split_once('/')?;
    let a: f64 = a.trim().parse().ok()?;
    let b: f64 = b.trim().parse().ok()?;
    if b == 0.0 { None } else { Some(a / b) }
}

fn parse_meter(s: &str) -> (u32, u32) {
    let s = s.trim();
    match s {
        "C" => (4, 4),
        "C|" => (2, 2),
        _ => {
            if let Some((a, b)) = s.split_once('/') {
                let a = a.split('+').filter_map(|x| x.trim().parse::<u32>().ok()).sum::<u32>();
                let b = b.trim().parse::<u32>().unwrap_or(4);
                if a > 0 && b > 0 {
                    return (a, b);
                }
            }
            (4, 4)
        }
    }
}

impl Parser {
    fn compound(&self) -> bool {
        self.meter.0.is_multiple_of(3) && self.meter.0 > 3 && self.meter.1 == 8
    }

    fn default_beat(&self) -> f64 {
        if self.compound() { 1.5 } else { 4.0 / self.meter.1 as f64 }
    }

    fn set_tempo(&mut self, s: &str) {
        // `"Allegro" 1/4=120`, `3/8=80`, or a bare number (unit notes per minute).
        let s: String = {
            let mut out = String::new();
            let mut q = false;
            for c in s.chars() {
                if c == '"' {
                    q = !q;
                } else if !q {
                    out.push(c);
                }
            }
            out
        };
        if let Some((l, r)) = s.split_once('=') {
            let beat: f64 = l.split_whitespace().filter_map(parse_frac).sum();
            if let Ok(n) = r.trim().parse::<f64>() {
                let beat = if beat > 0.0 { beat * 4.0 } else { self.unit * 4.0 };
                self.beat = beat;
                self.bpm = n * beat;
            }
        } else if let Ok(n) = s.trim().parse::<f64>() {
            self.beat = self.unit * 4.0;
            self.bpm = n * self.unit * 4.0;
        }
    }

    fn field(&mut self, k: char, v: &str) -> anyhow::Result<()> {
        match k {
            'M' => {
                self.meter = parse_meter(v);
            }
            'L' => {
                self.unit = parse_frac(v).with_context(|| format!("bad L:{v}"))?;
            }
            'Q' => self.set_tempo(v),
            'K' => self.key = Key::parse(v)?,
            _ => {}
        }
        Ok(())
    }

    fn lyric_line(&mut self, text: &str) {
        self.lyrics.push(text.trim().to_string());
        let verse = self.line_verse;
        self.line_verse += 1;
        // Tokenise: syllables split by spaces and '-', with _ * ~ | specials.
        let mut items: Vec<Option<String>> = Vec::new(); // None = skip/hold
        let mut bars_at: Vec<usize> = Vec::new(); // item index where a '|' appeared
        let mut cur = String::new();
        let mut chars = text.chars().peekable();
        let flush = |cur: &mut String, items: &mut Vec<Option<String>>| {
            if !cur.is_empty() {
                items.push(Some(std::mem::take(cur)));
            }
        };
        while let Some(c) = chars.next() {
            match c {
                ' ' | '\t' => flush(&mut cur, &mut items),
                '-' => {
                    if cur.is_empty() {
                        // "--": an extra note on the previous syllable.
                        if matches!(chars.peek(), Some(' ') | None) || items.is_empty() {
                            continue;
                        }
                        items.push(None);
                    } else {
                        flush(&mut cur, &mut items);
                    }
                }
                '_' | '*' => {
                    flush(&mut cur, &mut items);
                    items.push(None);
                }
                '~' => cur.push(' '),
                '|' => {
                    flush(&mut cur, &mut items);
                    bars_at.push(items.len());
                }
                '\\' => {
                    if let Some(n) = chars.next() {
                        cur.push(n);
                    }
                }
                _ => cur.push(c),
            }
        }
        flush(&mut cur, &mut items);
        let notes = self.line_notes.clone();
        let mut ni = 0usize;
        let mut bar_marks = bars_at.into_iter().peekable();
        for (ii, item) in items.into_iter().enumerate() {
            while bar_marks.peek() == Some(&ii) {
                bar_marks.next();
                // Jump to the first note of the next bar.
                if let Some(&(_, b)) = ni.checked_sub(1).and_then(|k| notes.get(k)) {
                    while ni < notes.len() && notes[ni].1 <= b {
                        ni += 1;
                    }
                }
            }
            let Some(&(id, _)) = notes.get(ni) else { break };
            let v = &mut self.sylls[id];
            while v.len() <= verse {
                v.push(None);
            }
            v[verse] = item;
            ni += 1;
        }
    }

    fn end_music_line(&mut self) {
        self.line_notes.clear();
        self.line_verse = 0;
        self.bar_in_line = 0;
    }

    fn push_note(&mut self, midis: Vec<i32>, mut len: f64, tie: bool, takes_syllable: bool) {
        if let Some((k, left)) = self.tuplet {
            len *= k;
            self.tuplet = if left > 1 { Some((k, left - 1)) } else { None };
        }
        if self.broken_next != 1.0 {
            len *= self.broken_next;
            self.broken_next = 1.0;
        }
        let id = self.sylls.len();
        self.sylls.push(Vec::new());
        if takes_syllable && !midis.is_empty() {
            self.line_notes.push((id, self.bar_in_line));
        }
        if midis.is_empty() {
            self.toks.push(Tok::Rest { len });
        } else {
            self.toks.push(Tok::Note { midis, len, tie, id });
        }
    }

    fn last_len_mut(&mut self) -> Option<&mut f64> {
        self.toks.iter_mut().rev().find_map(|t| match t {
            Tok::Note { len, .. } | Tok::Rest { len } => Some(len),
            _ => None,
        })
    }

    fn bar(&mut self, k: BarKind) {
        self.bar_acc.clear();
        self.bar_in_line += 1;
        self.toks.push(Tok::Bar(k));
    }

    fn pitch(&mut self, acc: Option<i32>, letter: char, octave_marks: i32) -> i32 {
        let li = letter_index(letter).unwrap_or(0);
        let base_oct = (if letter.is_ascii_lowercase() { 1 } else { 0 }) + octave_marks;
        let key = (li, base_oct);
        let alter = match acc {
            Some(a) => {
                if let Some(e) = self.bar_acc.iter_mut().find(|e| e.0 == key) {
                    e.1 = a;
                } else {
                    self.bar_acc.push((key, a));
                }
                a
            }
            None => self.bar_acc.iter().find(|e| e.0 == key).map(|e| e.1).unwrap_or(self.key.acc[li]),
        };
        60 + 12 * base_oct + LETTER_PC[li] + alter
    }

    fn music_line(&mut self, line: &str) -> anyhow::Result<()> {
        let c: Vec<char> = line.chars().collect();
        let n = c.len();
        let mut i = 0;
        let mut prev_tie_pending = false;
        let read_len = |c: &[char], i: &mut usize| -> f64 {
            let mut num = String::new();
            while *i < c.len() && c[*i].is_ascii_digit() {
                num.push(c[*i]);
                *i += 1;
            }
            let mut v: f64 = if num.is_empty() { 1.0 } else { num.parse().unwrap_or(1.0) };
            while *i < c.len() && c[*i] == '/' {
                *i += 1;
                let mut d = String::new();
                while *i < c.len() && c[*i].is_ascii_digit() {
                    d.push(c[*i]);
                    *i += 1;
                }
                v /= if d.is_empty() { 2.0 } else { d.parse().unwrap_or(2.0) };
            }
            v
        };
        while i < n {
            let ch = c[i];
            match ch {
                ' ' | '\t' | '`' | 'y' | ')' | '\\' => i += 1,
                '%' => break,
                '"' => {
                    let end = c[i + 1..].iter().position(|&x| x == '"').map(|p| i + 1 + p).unwrap_or(n);
                    let s: String = c[i + 1..end].iter().collect();
                    i = end + 1;
                    if s.starts_with(['^', '_', '<', '>', '@']) {
                        continue;
                    }
                    let chord = Chord::parse(&s).with_context(|| format!("bad chord symbol \"{s}\""))?;
                    self.toks.push(Tok::Chord(chord));
                }
                '!' | '+' => {
                    let end = c[i + 1..].iter().position(|&x| x == ch).map(|p| i + 1 + p).unwrap_or(n);
                    i = end + 1;
                }
                '{' => {
                    let end = c[i + 1..].iter().position(|&x| x == '}').map(|p| i + 1 + p).unwrap_or(n);
                    i = end + 1;
                }
                '.' | '~' | 'H' | 'L' | 'M' | 'O' | 'P' | 'S' | 'T' | 'u' | 'v' | 'J' | 'R' => i += 1,
                '(' => {
                    i += 1;
                    if i < n && c[i].is_ascii_digit() {
                        let mut nums = [0u32; 3];
                        let mut k = 0;
                        loop {
                            let mut s = String::new();
                            while i < n && c[i].is_ascii_digit() {
                                s.push(c[i]);
                                i += 1;
                            }
                            if k < 3 {
                                nums[k] = s.parse().unwrap_or(0);
                            }
                            k += 1;
                            if i < n && c[i] == ':' && k < 3 {
                                i += 1;
                                continue;
                            }
                            break;
                        }
                        let p = nums[0].max(1);
                        let q = if nums[1] > 0 {
                            nums[1]
                        } else {
                            match p {
                                2 | 4 | 8 => 3,
                                3 | 6 => 2,
                                _ => {
                                    if self.compound() {
                                        3
                                    } else {
                                        2
                                    }
                                }
                            }
                        };
                        let r = if nums[2] > 0 { nums[2] } else { p };
                        self.tuplet = Some((q as f64 / p as f64, r));
                    }
                }
                '-' => {
                    if let Some(Tok::Note { tie, .. }) = self.toks.last_mut() {
                        *tie = true;
                    }
                    prev_tie_pending = true;
                    i += 1;
                }
                '>' | '<' => {
                    let mut k = 0;
                    while i < n && c[i] == ch {
                        k += 1;
                        i += 1;
                    }
                    let f = [1.5, 1.75, 1.875][(k - 1).min(2)];
                    let (a, b) = if ch == '>' { (f, 2.0 - f) } else { (2.0 - f, f) };
                    if let Some(l) = self.last_len_mut() {
                        *l *= a;
                    }
                    self.broken_next = b;
                }
                '|' | ':' | '[' if self.is_bar_start(&c, i) => {
                    i = self.read_bar(&c, i);
                }
                '[' => {
                    // Inline field [K:G] or a chord [CEG].
                    if i + 2 < n && c[i + 1].is_ascii_alphabetic() && c[i + 2] == ':' {
                        let end = c[i..].iter().position(|&x| x == ']').map(|p| i + p).unwrap_or(n);
                        let v: String = c[i + 3..end].iter().collect();
                        self.field(c[i + 1], &v)?;
                        i = end + 1;
                        continue;
                    }
                    i += 1;
                    let mut midis = Vec::new();
                    let mut first_len = None;
                    let mut tie = false;
                    while i < n && c[i] != ']' {
                        let (m, l, ni) = self.read_note(&c, i)?;
                        i = ni;
                        if let Some(m) = m {
                            midis.push(m);
                            first_len.get_or_insert(l);
                        }
                        if i < n && c[i] == '-' {
                            tie = true;
                            i += 1;
                        }
                    }
                    i += 1;
                    let mult = read_len(&c, &mut i);
                    let len = first_len.unwrap_or(1.0) * mult * self.unit * 4.0;
                    let takes = !prev_tie_pending;
                    prev_tie_pending = false;
                    self.push_note(midis, len, tie, takes);
                }
                'z' | 'x' => {
                    i += 1;
                    let len = read_len(&c, &mut i) * self.unit * 4.0;
                    prev_tie_pending = false;
                    self.push_note(Vec::new(), len, false, false);
                }
                'Z' | 'X' => {
                    i += 1;
                    let mut s = String::new();
                    while i < n && c[i].is_ascii_digit() {
                        s.push(c[i]);
                        i += 1;
                    }
                    let bars: f64 = s.parse().unwrap_or(1.0);
                    let bar = self.meter.0 as f64 * 4.0 / self.meter.1 as f64;
                    self.push_note(Vec::new(), bars * bar, false, false);
                }
                '^' | '_' | '=' | 'A'..='G' | 'a'..='g' => {
                    let (m, l, ni) = self.read_note(&c, i)?;
                    i = ni;
                    let takes = !prev_tie_pending;
                    prev_tie_pending = false;
                    if let Some(m) = m {
                        self.push_note(vec![m], l * self.unit * 4.0, false, takes);
                    }
                }
                _ => i += 1,
            }
        }
        Ok(())
    }

    fn is_bar_start(&self, c: &[char], i: usize) -> bool {
        match c[i] {
            '|' => true,
            ':' => i + 1 < c.len() && matches!(c[i + 1], '|' | ':'),
            '[' => i + 1 < c.len() && (c[i + 1] == '|' || c[i + 1].is_ascii_digit()),
            _ => false,
        }
    }

    fn read_bar(&mut self, c: &[char], mut i: usize) -> usize {
        let n = c.len();
        if c[i] == '[' && i + 1 < n && c[i + 1].is_ascii_digit() {
            i += 1;
            let mut s = String::new();
            while i < n && c[i].is_ascii_digit() {
                s.push(c[i]);
                i += 1;
            }
            while i < n && matches!(c[i], ',' | '-') || (i < n && c[i].is_ascii_digit()) {
                i += 1;
            }
            self.toks.push(Tok::Ending(s.parse().unwrap_or(1)));
            return i;
        }
        let mut colons_before = 0;
        while i < n && c[i] == ':' {
            colons_before += 1;
            i += 1;
        }
        let mut pipes = 0;
        while i < n && (c[i] == '|' || c[i] == ']' || (c[i] == '[' && i + 1 < n && c[i + 1] == '|')) {
            if c[i] == '|' {
                pipes += 1;
            }
            i += 1;
        }
        let mut colons_after = 0;
        while i < n && c[i] == ':' {
            colons_after += 1;
            i += 1;
        }
        if pipes == 0 && colons_before >= 2 {
            colons_before = 1;
            colons_after = 1;
        }
        let kind = match (colons_before > 0, colons_after > 0) {
            (true, true) => BarKind::RepeatBoth,
            (true, false) => BarKind::RepeatEnd,
            (false, true) => BarKind::RepeatStart,
            (false, false) => BarKind::Plain,
        };
        self.bar(kind);
        // "|1" / ":|2": an ending right after the bar.
        if i < n && c[i].is_ascii_digit() {
            let mut s = String::new();
            while i < n && c[i].is_ascii_digit() {
                s.push(c[i]);
                i += 1;
            }
            self.toks.push(Tok::Ending(s.parse().unwrap_or(1)));
        }
        i
    }

    /// Reads one note (accidental, letter, octave marks, length) at `i`.
    fn read_note(&mut self, c: &[char], mut i: usize) -> anyhow::Result<(Option<i32>, f64, usize)> {
        let n = c.len();
        let mut acc = None;
        while i < n && matches!(c[i], '^' | '_' | '=') {
            let a = match c[i] {
                '^' => 1,
                '_' => -1,
                _ => 0,
            };
            acc = Some(match acc {
                Some(p) if a != 0 => p + a,
                _ => a,
            });
            i += 1;
        }
        if i >= n {
            return Ok((None, 1.0, i));
        }
        let letter = c[i];
        if letter_index(letter).is_none() {
            if matches!(letter, 'z' | 'x') {
                i += 1;
            } else {
                i += 1;
                return Ok((None, 1.0, i));
            }
        } else {
            i += 1;
        }
        let mut oct = 0;
        while i < n && matches!(c[i], '\'' | ',') {
            oct += if c[i] == '\'' { 1 } else { -1 };
            i += 1;
        }
        let mut num = String::new();
        while i < n && c[i].is_ascii_digit() {
            num.push(c[i]);
            i += 1;
        }
        let mut v: f64 = if num.is_empty() { 1.0 } else { num.parse().unwrap_or(1.0) };
        while i < n && c[i] == '/' {
            i += 1;
            let mut d = String::new();
            while i < n && c[i].is_ascii_digit() {
                d.push(c[i]);
                i += 1;
            }
            v /= if d.is_empty() { 2.0 } else { d.parse().unwrap_or(2.0) };
        }
        if letter_index(letter).is_none() {
            return Ok((None, v, i));
        }
        Ok((Some(self.pitch(acc, letter, oct)), v, i))
    }
}

/// Parses one tune and expands its repeats.
pub fn parse(text: &str) -> anyhow::Result<Song> {
    let mut p = Parser {
        unit: 0.0,
        meter: (4, 4),
        key: Key::default(),
        bpm: 0.0,
        beat: 1.0,
        toks: Vec::new(),
        sylls: Vec::new(),
        line_notes: Vec::new(),
        line_verse: 0,
        bar_in_line: 0,
        bar_acc: Vec::new(),
        tuplet: None,
        broken_next: 1.0,
        lyrics: Vec::new(),
    };
    let mut title = String::new();
    let mut composer = String::new();
    let mut in_body = false;
    let mut pending_q = None;
    for raw in text.lines() {
        let line = raw.trim_end();
        let trimmed = line.trim_start();
        if trimmed.starts_with('%') || (trimmed.is_empty() && !in_body) {
            continue;
        }
        let is_field = trimmed.len() >= 2 && trimmed.as_bytes()[1] == b':' && trimmed.as_bytes()[0].is_ascii_alphabetic();
        if is_field {
            let k = trimmed.as_bytes()[0] as char;
            let v = trimmed[2..].trim();
            let v = v.split('%').next().unwrap_or("").trim();
            match k {
                'T' if title.is_empty() => title = v.to_string(),
                'C' if !in_body => composer = v.to_string(),
                'w' => {
                    p.lyric_line(&trimmed[2..]);
                    continue;
                }
                'W' => p.lyrics.push(v.to_string()),
                'K' if !in_body => {
                    p.key = Key::parse(v)?;
                    in_body = true;
                    if p.unit == 0.0 {
                        let (a, b) = p.meter;
                        p.unit = if (a as f64 / b as f64) < 0.75 { 1.0 / 16.0 } else { 1.0 / 8.0 };
                    }
                    p.beat = p.default_beat();
                    if let Some(q) = pending_q.take() {
                        p.set_tempo(q);
                    }
                    continue;
                }
                'Q' if !in_body => {
                    pending_q = Some(v);
                    continue;
                }
                _ => {}
            }
            p.field(k, v)?;
            if k != 'w' && in_body {
                p.end_music_line();
            }
            continue;
        }
        if !in_body {
            continue;
        }
        if trimmed.is_empty() {
            continue;
        }
        p.end_music_line();
        p.music_line(line).with_context(|| format!("in line {line:?}"))?;
    }
    if !in_body {
        bail!("no K: field (the header must end with K:)");
    }
    if p.bpm <= 0.0 {
        p.beat = p.default_beat();
        p.bpm = 120.0 * p.beat;
    }
    let mut song = expand(&p)?;
    song.title = if title.is_empty() { "Untitled".into() } else { title };
    song.composer = composer;
    song.lyrics = p.lyrics.clone();
    Ok(song)
}

/// Plays the token list through its repeats into flat notes, chords and bars.
fn expand(p: &Parser) -> anyhow::Result<Song> {
    let toks = &p.toks;
    let mut order: Vec<usize> = Vec::new(); // token indices in performance order
    let mut verse_of: Vec<usize> = Vec::new();
    let mut pos = 0usize;
    let mut start = 0usize;
    let mut pass = 1u32;
    let mut verse = 0usize;
    let mut guard = 0usize;
    while pos < toks.len() {
        guard += 1;
        if guard > 200_000 {
            bail!("repeat structure does not terminate");
        }
        match &toks[pos] {
            Tok::Bar(BarKind::RepeatStart) => {
                start = pos + 1;
                pass = 1;
                verse = 0;
                order.push(pos);
                verse_of.push(verse);
            }
            Tok::Bar(BarKind::RepeatEnd | BarKind::RepeatBoth) => {
                order.push(pos);
                verse_of.push(verse);
                if pass == 1 {
                    pass = 2;
                    verse = 1;
                    pos = start;
                    continue;
                }
                pass = 1;
                verse = 0;
                start = pos + 1;
            }
            Tok::Ending(e) => {
                if *e != pass {
                    // Skip to the ending for this pass (or past the repeat).
                    let mut j = pos + 1;
                    while j < toks.len() {
                        if let Tok::Ending(e2) = toks[j] {
                            if e2 == pass {
                                break;
                            }
                        }
                        j += 1;
                    }
                    pos = j;
                    continue;
                }
                if pass >= 2 {
                    // The last ending: what follows is a fresh section.
                    pass = 1;
                    start = pos + 1;
                }
            }
            _ => {
                order.push(pos);
                verse_of.push(verse);
            }
        }
        pos += 1;
    }

    let bar_len = p.meter.0 as f64 * 4.0 / p.meter.1 as f64;
    let mut notes: Vec<Note> = Vec::new();
    let mut chords = Vec::new();
    let mut bars: Vec<Bar> = Vec::new();
    let mut t = 0.0f64;
    let mut bar_start = 0.0f64;
    let mut bar_count = 0u32;
    let mut tie_open: Vec<usize> = Vec::new(); // indices of notes waiting for a tie
    let close_bar = |t: f64, bar_start: &mut f64, bars: &mut Vec<Bar>, count: &mut u32| {
        if t > *bar_start + 1e-9 {
            bars.push(Bar { start: *bar_start, len: t - *bar_start, number: *count });
            *count += 1;
            *bar_start = t;
        }
    };
    for (k, &ti) in order.iter().enumerate() {
        match &toks[ti] {
            Tok::Note { midis, len, tie, id } => {
                let bar_no = bar_count;
                let v = verse_of[k];
                let syl = p.sylls[*id].get(v).cloned().flatten().or_else(|| p.sylls[*id].first().cloned().flatten());
                let mut new_open = Vec::new();
                for (j, &m) in midis.iter().enumerate() {
                    // Continue a tied note of the same pitch.
                    if let Some(pos) = tie_open.iter().position(|&ni| notes[ni].midi == m) {
                        let ni = tie_open.remove(pos);
                        notes[ni].len += len;
                        if *tie {
                            new_open.push(ni);
                        }
                        continue;
                    }
                    notes.push(Note {
                        start: t,
                        len: *len,
                        midi: m,
                        bar: bar_no,
                        syllable: if j == 0 { syl.clone() } else { None },
                        in_chord: j > 0,
                    });
                    if *tie {
                        new_open.push(notes.len() - 1);
                    }
                }
                tie_open = new_open;
                t += len;
            }
            Tok::Rest { len } => {
                tie_open.clear();
                t += len;
            }
            Tok::Chord(c) => chords.push(ChordEvent { start: t, chord: *c }),
            Tok::Bar(_) => close_bar(t, &mut bar_start, &mut bars, &mut bar_count),
            Tok::Ending(_) => {}
        }
    }
    close_bar(t, &mut bar_start, &mut bars, &mut bar_count);
    // A short first bar is a pickup: number it 0, the rest from 1.
    let pickup = bars.first().map(|b| b.len < bar_len - 1e-6).unwrap_or(false);
    if !pickup {
        for b in &mut bars {
            b.number += 1;
        }
        for n in &mut notes {
            n.bar += 1;
        }
    }
    notes.sort_by(|a, b| a.start.partial_cmp(&b.start).unwrap_or(std::cmp::Ordering::Equal));
    Ok(Song {
        title: String::new(),
        composer: String::new(),
        meter: p.meter,
        unit: p.unit,
        bpm: p.bpm,
        beat: p.beat,
        key: p.key,
        notes,
        chords,
        bars,
        length: t,
        lyrics: Vec::new(),
    })
}
