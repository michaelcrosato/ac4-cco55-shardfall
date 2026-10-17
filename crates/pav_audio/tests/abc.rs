//! The ABC parser: lengths, keys, accidentals, repeats and endings, triplets, broken rhythm,
//! ties, chords and lyrics.

use pav_audio::abc::{self, Chord, Key, Mode, Quality};

fn tune(body: &str) -> abc::Song {
    abc::parse(&format!("X:1\nT:Test\nM:4/4\nL:1/8\nQ:1/4=120\nK:C\n{body}\n")).unwrap()
}

fn midis(s: &abc::Song) -> Vec<i32> {
    s.notes.iter().map(|n| n.midi).collect()
}

fn lens(s: &abc::Song) -> Vec<f64> {
    s.notes.iter().map(|n| n.len).collect()
}

#[test]
fn headers() {
    let s = abc::parse("X:3\nT:Spanish Ladies\nT:alt\nC:Trad\nM:3/4\nL:1/8\nQ:1/4=132\nK:Dm\nA2 |d3 d e2 |\n").unwrap();
    assert_eq!(s.title, "Spanish Ladies");
    assert_eq!(s.composer, "Trad");
    assert_eq!(s.meter, (3, 4));
    assert_eq!(s.unit, 0.125);
    assert_eq!(s.bpm, 132.0);
    assert_eq!(s.key, Key::parse("Dm").unwrap());
    // 6/8 with Q in dotted quarters: 3/8=80 is 120 quarter notes a minute.
    let j = abc::parse("X:1\nM:6/8\nQ:3/8=80\nK:G\nGAB cBA|\n").unwrap();
    assert_eq!(j.bpm, 120.0);
    assert_eq!(j.beat, 1.5);
    assert_eq!(j.unit, 0.125, "default L for 6/8 is 1/8");
    // Default L: 1/16 when the meter is below 3/4.
    let t = abc::parse("X:1\nM:2/4\nK:C\nC D|\n").unwrap();
    assert_eq!(t.unit, 1.0 / 16.0);
    // C means 4/4.
    assert_eq!(abc::parse("X:1\nM:C\nK:C\nC|\n").unwrap().meter, (4, 4));
    assert!(abc::parse("X:1\nT:no key\nCDE|\n").is_err(), "the header must end with K:");
}

#[test]
fn note_lengths() {
    // L:1/8, so a plain note is half a beat.
    let s = tune("C C2 C/2 C/ C// C3/2 C3/ C3 z2 C4 |");
    assert_eq!(lens(&s), vec![0.5, 1.0, 0.25, 0.25, 0.125, 0.75, 0.75, 1.5, 2.0]);
    // Starts include the rest.
    assert_eq!(s.notes.last().unwrap().start, 0.5 + 1.0 + 0.25 + 0.25 + 0.125 + 0.75 + 0.75 + 1.5 + 1.0);
}

#[test]
fn octaves_and_pitches() {
    let s = tune("C D E F G A B c d c' C, C,, a' a'' |");
    assert_eq!(midis(&s), vec![60, 62, 64, 65, 67, 69, 71, 72, 74, 84, 48, 36, 93, 105]);
}

#[test]
fn key_signatures_and_modes() {
    let k = |s: &str| Key::parse(s).unwrap();
    assert_eq!(k("G").acc, [0, 0, 0, 1, 0, 0, 0], "G major: F#");
    assert_eq!(k("D").acc, [1, 0, 0, 1, 0, 0, 0], "D major: F# C#");
    assert_eq!(k("Dm").acc, [0, 0, 0, 0, 0, 0, -1], "D minor: Bb");
    assert_eq!(k("Ddor").acc, [0; 7], "D dorian: no accidentals");
    assert_eq!(k("Ador").acc, [0, 0, 0, 1, 0, 0, 0], "A dorian: F#");
    assert_eq!(k("Emix").acc, [1, 0, 0, 1, 1, 0, 0], "E mixolydian: F# C# G#");
    assert_eq!(k("Bb").acc, [0, 0, -1, 0, 0, 0, -1]);
    assert_eq!(k("F#m").acc, [1, 0, 0, 1, 1, 0, 0]);
    assert_eq!(k("A minor").mode, Mode::Minor);
    assert_eq!(k("Gm").tonic, 7);
    assert_eq!(k("D ^g").acc[4], 1, "explicit extra accidentals");
    assert_eq!(k("Dphr").acc, [0, 0, -1, 0, 0, 0, -1]);
    assert_eq!(k("Flyd").acc, [0; 7]);
    // In the tune: K:D sharpens F and C.
    let s = abc::parse("X:1\nK:D\nD E F G A B c d|\n").unwrap();
    assert_eq!(midis(&s), vec![62, 64, 66, 67, 69, 71, 73, 74]);
    // Scale of D dorian.
    assert_eq!(Key::parse("Ddor").unwrap().scale_pcs(), [2, 4, 5, 7, 9, 11, 0]);
}

#[test]
fn accidentals_last_to_the_bar_line() {
    let s = tune("^F F =F F | F ^^G _B __B |");
    assert_eq!(midis(&s), vec![66, 66, 65, 65, 65, 69, 70, 69]);
    // Only for the same octave.
    let s = tune("^F f F |");
    assert_eq!(midis(&s), vec![66, 77, 66]);
    // In a key: a natural cancels the signature until the bar line.
    let s = abc::parse("X:1\nK:G\nF =F F | F|\n").unwrap();
    assert_eq!(midis(&s), vec![66, 65, 65, 66]);
}

#[test]
fn broken_rhythm() {
    let s = tune("A>B A<B A>>B |");
    assert_eq!(lens(&s), vec![0.75, 0.25, 0.25, 0.75, 0.875, 0.125]);
}

#[test]
fn triplets() {
    // Three eighths in the time of two.
    let s = tune("(3ABc d |");
    let l = lens(&s);
    assert!((l[0] - 1.0 / 3.0).abs() < 1e-9 && (l[2] - 1.0 / 3.0).abs() < 1e-9);
    assert!((s.notes[3].start - 1.0).abs() < 1e-9, "the triplet takes one beat");
    // (p:q:r
    let s = tune("(3:2:2A2B A |");
    assert!((s.notes[0].len - 2.0 / 3.0).abs() < 1e-9 && (s.notes[1].len - 1.0 / 3.0).abs() < 1e-9);
    assert!((s.notes[2].len - 0.5).abs() < 1e-9, "only r notes are affected");
}

#[test]
fn ties_merge_notes() {
    let s = tune("c4- c4 | d2- | d2 e2 |");
    assert_eq!(midis(&s), vec![72, 74, 76]);
    assert_eq!(lens(&s), vec![4.0, 2.0, 1.0]);
}

#[test]
fn repeats_and_endings() {
    let s = tune("|: C D :| E |");
    assert_eq!(midis(&s), vec![60, 62, 60, 62, 64]);
    let s = tune("|: C D |1 E :|2 F |] G |");
    assert_eq!(midis(&s), vec![60, 62, 64, 60, 62, 65, 67]);
    let s = tune("|: C D |[1 E :|[2 F |");
    assert_eq!(midis(&s), vec![60, 62, 64, 60, 62, 65]);
    // A repeat with no start goes back to the beginning; :: repeats both ways.
    let s = tune("C :| D :: E :|");
    assert_eq!(midis(&s), vec![60, 60, 62, 62, 64, 64]);
    // Times keep running through the repeats, and bars are counted through them.
    let s = tune("|: C8 :| D8 |");
    assert_eq!(s.notes.iter().map(|n| n.start).collect::<Vec<_>>(), vec![0.0, 4.0, 8.0]);
    assert_eq!(s.notes.iter().map(|n| n.bar).collect::<Vec<_>>(), vec![1, 2, 3]);
    assert_eq!(s.length, 12.0);
}

#[test]
fn pickup_bar_is_bar_zero() {
    let s = tune("G | C8 | D8 |");
    assert_eq!(s.bars[0].number, 0);
    assert_eq!(s.bars[0].len, 0.5);
    assert_eq!(s.notes[1].bar, 1);
}

#[test]
fn chords_and_chord_symbols() {
    let s = tune("\"Am\"A2 \"G7\"G2 \"D/F#\"[DFA]4 |");
    assert_eq!(s.chords.len(), 3);
    assert_eq!(s.chords[0].chord, Chord { root: 9, quality: Quality::Minor, bass: None });
    assert_eq!(s.chords[1].chord.quality, Quality::Dom7);
    assert_eq!(s.chords[1].start, 1.0);
    assert_eq!(s.chords[2].chord, Chord { root: 2, quality: Quality::Major, bass: Some(6) });
    assert_eq!(s.chord_at(1.5).unwrap().root, 7);
    // [DFA] sounds together.
    let st: Vec<f64> = s.notes.iter().skip(2).map(|n| n.start).collect();
    assert_eq!(st, vec![2.0, 2.0, 2.0]);
    assert!(s.notes[3].in_chord && s.notes[4].in_chord && !s.notes[2].in_chord);
    assert_eq!(Chord::parse("Bbmaj7").unwrap(), Chord { root: 10, quality: Quality::Maj7, bass: None });
    assert_eq!(Chord::parse("F#dim").unwrap().pcs(), vec![6, 9, 0]);
    assert!(Chord::parse("Hm").is_none());
    // Annotations are skipped; a bad chord symbol is an error.
    assert!(abc::parse("X:1\nK:C\n\"^Chorus\"C D|\n").is_ok());
    assert!(abc::parse("X:1\nK:C\n\"Qx\"C D|\n").is_err());
}

#[test]
fn decorations_grace_notes_and_comments_are_skipped() {
    let s = tune("!trill!C {g}D ~E .F % a comment G\n");
    assert_eq!(midis(&s), vec![60, 62, 64, 65]);
}

#[test]
fn lyrics_align_to_notes() {
    let s = tune("C D E F | G2 G2 |\nw: What shall we do with a-\n");
    let syl: Vec<Option<&str>> = s.notes.iter().map(|n| n.syllable.as_deref()).collect();
    assert_eq!(syl, vec![Some("What"), Some("shall"), Some("we"), Some("do"), Some("with"), Some("a")]);
    assert_eq!(s.lyrics, vec!["What shall we do with a-".to_string()]);
    // Hyphens split syllables, _ holds a note, * skips one, tied notes take none.
    let s = tune("C D E- E F G |\nw: drun-ken _ sai\n");
    let syl: Vec<Option<&str>> = s.notes.iter().map(|n| n.syllable.as_deref()).collect();
    assert_eq!(syl, vec![Some("drun"), Some("ken"), None, Some("sai"), None]);
    // Two lyric lines under a repeat: the first and second time through.
    let s = tune("|: C D :|\nw: one two\nw: three four\n");
    let syl: Vec<Option<&str>> = s.notes.iter().map(|n| n.syllable.as_deref()).collect();
    assert_eq!(syl, vec![Some("one"), Some("two"), Some("three"), Some("four")]);
}

#[test]
fn inline_fields_and_body_fields() {
    let s = tune("C D [K:G] F | \nM:3/4\nF2 F2 F2 |\n");
    assert_eq!(midis(&s), vec![60, 62, 66, 66, 66, 66]);
    assert_eq!(s.meter, (3, 4));
}

#[test]
fn tempo_in_seconds() {
    let s = tune("C8 | C8 |");
    // Two 4/4 bars at 120 bpm: 4 seconds.
    assert!((s.seconds() - 4.0).abs() < 1e-9);
}
