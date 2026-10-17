//! The front-end state machine, from `update` alone (plus an egui pass where pointer hit
//! boxes are needed): focus graphs, back, wrap, sounds, menus, settings, attract mode,
//! kiosk, mobile, pause.

mod common;

use std::collections::BTreeSet;

use glam::Vec3;
use pav_core::play::{DemoState, PlayFrame, UiRequest};
use pav_ui::frontend::Mode;
use pav_ui::glyph::GlyphFamily;
use pav_ui::{Command, FrontEnd, HudEnv, Nav, Platform, Projector, Settings, UiInput};

const DT: f32 = 1.0 / 60.0;

fn press(fe: &mut FrontEnd, nav: Nav) -> Vec<Command> {
    fe.update(DT, &UiInput::press(nav), None)
}

fn idle(fe: &mut FrontEnd, seconds: f32, dt: f32, frame: Option<&PlayFrame>) -> Vec<Command> {
    let mut all = Vec::new();
    let mut t = 0.0;
    while t < seconds {
        all.extend(fe.update(dt, &UiInput::default(), frame));
        t += dt;
    }
    all
}

fn loads(cmds: &[Command]) -> Vec<(String, u64, bool)> {
    cmds.iter()
        .filter_map(|c| match c {
            Command::LoadScene { scene, seed, autopilot } => Some((scene.clone(), *seed, *autopilot)),
            _ => None,
        })
        .collect()
}

fn sounds(cmds: &[Command]) -> Vec<String> {
    cmds.iter()
        .filter_map(|c| match c {
            Command::Sound(s) => Some(s.clone()),
            _ => None,
        })
        .collect()
}

/// Runs one egui pass of `draw` (records hit boxes), at 1920×1080 points.
fn draw_once(fe: &mut FrontEnd, frame: Option<&PlayFrame>, size: egui::Vec2) {
    let theme = common::theme();
    let bindings = common::bindings();
    let settings = Settings::default();
    let ctx = egui::Context::default();
    pav_ui::theme::install(&ctx, &theme);
    let raw = egui::RawInput { screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)), ..Default::default() };
    let mut out = ctx.run_ui(raw, |ui| {
        let env = HudEnv {
            theme: &theme,
            bindings: &bindings,
            family: GlyphFamily::PlayStation,
            settings: &settings,
            proj: Projector::default(),
            time: 0.0,
        };
        fe.draw(ui.ctx(), frame, &env);
    });
    out.textures_delta.clear();
}

fn to_menu(has_progress: bool) -> FrontEnd {
    let mut fe = common::desktop(has_progress);
    fe.update(DT, &UiInput::default(), None);
    press(&mut fe, Nav::Confirm);
    assert_eq!(fe.mode(), Mode::Menu);
    fe
}

/// Every focus id reachable from `fe` with `moves`, staying on the same screen.
fn reachable(fe: &FrontEnd, moves: &[Nav]) -> BTreeSet<String> {
    let mode = fe.mode();
    let mut seen = BTreeSet::new();
    seen.insert(fe.focus_id());
    let mut queue = vec![fe.clone()];
    while let Some(f) = queue.pop() {
        for m in moves {
            let mut g = f.clone();
            g.update(DT, &UiInput::press(*m), None);
            if g.mode() != mode {
                continue;
            }
            if seen.insert(g.focus_id()) {
                queue.push(g);
            }
        }
    }
    seen
}

const DIRS: [Nav; 4] = [Nav::Up, Nav::Down, Nav::Left, Nav::Right];

fn assert_all_reachable(fe: &FrontEnd, moves: &[Nav], expected: &[String]) {
    let seen = reachable(fe, moves);
    for id in expected {
        assert!(seen.contains(id), "{:?}: {id} not reachable (reached {seen:?})", fe.mode());
    }
}

#[test]
fn starts_on_the_title_and_loads_its_scene() {
    let mut fe = common::desktop(false);
    assert_eq!(fe.mode(), Mode::Title);
    assert_eq!(fe.mode().name(), "title");
    let c = fe.update(DT, &UiInput::default(), None);
    assert_eq!(loads(&c), vec![("blacktap/title".to_string(), 0, false)]);
    assert!(!fe.game_input());
}

#[test]
fn title_any_input_opens_the_menu_and_is_consumed() {
    let mut fe = common::desktop(true);
    fe.update(DT, &UiInput::default(), None);
    let input = UiInput { nav: vec![Nav::Down, Nav::Confirm], any: true, ..Default::default() };
    let c = fe.update(DT, &input, None);
    assert_eq!(fe.mode(), Mode::Menu);
    assert_eq!(fe.focus_id(), "menu/continue", "the press opened the menu and did nothing else");
    assert!(loads(&c).is_empty());
    assert_eq!(sounds(&c), vec!["ui_confirm"]);
}

#[test]
fn menu_items_follow_progress_and_platform() {
    let fe = to_menu(true);
    assert_eq!(
        fe.focusables(),
        ["menu/continue", "menu/new_game", "menu/replay", "menu/training", "menu/settings", "menu/credits", "menu/quit"]
            .map(String::from)
    );
    let fe = to_menu(false);
    assert_eq!(fe.focusables().first().map(String::as_str), Some("menu/new_game"));
    let mut web = FrontEnd::new(common::info(), Settings::default(), Platform { can_quit: false, ..Default::default() }, false);
    web.update(DT, &UiInput::default(), None);
    press(&mut web, Nav::Confirm);
    assert!(!web.focusables().iter().any(|f| f == "menu/quit"));
    assert!(web.focusables().len() <= 7);
}

#[test]
fn focus_graphs_reach_every_item() {
    // Menu (both shapes).
    for progress in [true, false] {
        let fe = to_menu(progress);
        assert_all_reachable(&fe, &DIRS, &fe.focusables());
    }
    // Replay a Round.
    let mut fe = to_menu(true);
    press(&mut fe, Nav::Down);
    press(&mut fe, Nav::Down);
    press(&mut fe, Nav::Confirm);
    assert_eq!(fe.mode(), Mode::Chapters);
    assert_all_reachable(&fe, &DIRS, &fe.focusables());
    assert_eq!(fe.focusables().len(), 6);
    // Settings: every row of every tab with directions and L1/R1.
    let mut fe = to_menu(true);
    for _ in 0..4 {
        press(&mut fe, Nav::Down);
    }
    press(&mut fe, Nav::Confirm);
    assert_eq!(fe.mode(), Mode::Settings);
    let mut expected = Vec::new();
    let mut probe = fe.clone();
    for _ in 0..pav_ui::settings::TABS.len() {
        expected.extend(probe.focusables());
        press(&mut probe, Nav::TabNext);
    }
    assert_eq!(probe.focus_id(), fe.focus_id(), "tabs wrap");
    assert!(expected.len() >= 30);
    assert_all_reachable(&fe, &[Nav::Up, Nav::Down, Nav::Left, Nav::Right, Nav::TabPrev, Nav::TabNext], &expected);
    // Credits pages.
    let mut fe = to_menu(true);
    press(&mut fe, Nav::Up);
    press(&mut fe, Nav::Up);
    press(&mut fe, Nav::Confirm);
    assert_eq!(fe.mode(), Mode::Credits);
    assert_all_reachable(&fe, &DIRS, &fe.focusables());
    // Confirm (New Game over a save): both buttons, safe first.
    let mut fe = to_menu(true);
    press(&mut fe, Nav::Down);
    press(&mut fe, Nav::Confirm);
    assert_eq!(fe.mode(), Mode::Confirm);
    assert_eq!(fe.focus_id(), "confirm/safe");
    assert_all_reachable(&fe, &DIRS, &fe.focusables());
    // Pause menu.
    let fe = paused();
    assert_all_reachable(&fe, &DIRS, &fe.focusables());
    assert_eq!(fe.focusables().len(), 6);
}

#[test]
fn lists_wrap_and_every_move_sounds() {
    let mut fe = to_menu(true);
    let c = press(&mut fe, Nav::Up);
    assert_eq!(fe.focus_id(), "menu/quit", "up from the top wraps to the bottom");
    assert_eq!(sounds(&c), vec!["ui_move"]);
    let c = press(&mut fe, Nav::Down);
    assert_eq!(fe.focus_id(), "menu/continue");
    assert_eq!(sounds(&c), vec!["ui_move"]);
    let mut p = paused();
    press(&mut p, Nav::Up);
    assert_eq!(p.focus_id(), "pause/quit_to_title");
}

#[test]
fn back_always_returns() {
    // Settings → Menu → Title, saving on the way out.
    let mut fe = to_menu(false);
    for _ in 0..3 {
        press(&mut fe, Nav::Down);
    }
    assert_eq!(fe.focus_id(), "menu/settings");
    press(&mut fe, Nav::Confirm);
    assert_eq!(fe.mode(), Mode::Settings);
    let c = press(&mut fe, Nav::Back);
    assert_eq!(fe.mode(), Mode::Menu);
    assert!(c.contains(&Command::SaveSettings));
    assert_eq!(sounds(&c), vec!["ui_back"]);
    assert_eq!(fe.focus_id(), "menu/settings", "focus returns to where it was");
    press(&mut fe, Nav::Back);
    assert_eq!(fe.mode(), Mode::Title);
    // Chapters, Credits, Confirm → Menu.
    for (downs, mode) in [(2, Mode::Chapters), (5, Mode::Credits), (1, Mode::Confirm)] {
        let mut fe = to_menu(true);
        for _ in 0..downs {
            press(&mut fe, Nav::Down);
        }
        press(&mut fe, Nav::Confirm);
        assert_eq!(fe.mode(), mode);
        press(&mut fe, Nav::Back);
        assert_eq!(fe.mode(), Mode::Menu, "back from {mode:?}");
    }
    // Pause family → Paused → Play.
    for (downs, mode) in [(1, Mode::PauseMap), (2, Mode::PauseObjectives), (3, Mode::PauseSettings), (5, Mode::Confirm)] {
        let mut fe = paused();
        for _ in 0..downs {
            press(&mut fe, Nav::Down);
        }
        press(&mut fe, Nav::Confirm);
        assert_eq!(fe.mode(), mode);
        press(&mut fe, Nav::Back);
        assert_eq!(fe.mode(), Mode::Paused, "back from {mode:?}");
    }
    let mut fe = paused();
    let c = press(&mut fe, Nav::Back);
    assert_eq!(fe.mode(), Mode::Play);
    assert!(c.contains(&Command::SetPaused(false)));
}

#[test]
fn esc_sends_back_and_start_but_acts_once() {
    // Esc in the pause settings: Back (to Paused) wins; Start is dropped with the screen change.
    let mut fe = paused();
    for _ in 0..3 {
        press(&mut fe, Nav::Down);
    }
    press(&mut fe, Nav::Confirm);
    assert_eq!(fe.mode(), Mode::PauseSettings);
    fe.update(DT, &UiInput { nav: vec![Nav::Back, Nav::Start], any: true, ..Default::default() }, None);
    assert_eq!(fe.mode(), Mode::Paused);
}

fn play() -> FrontEnd {
    let mut fe = to_menu(false);
    for _ in 0..2 {
        press(&mut fe, Nav::Down);
    }
    assert_eq!(fe.focus_id(), "menu/training");
    let c = press(&mut fe, Nav::Confirm);
    assert_eq!(loads(&c), vec![("blacktap/drydock".to_string(), 0, false)]);
    assert_eq!(fe.mode(), Mode::Play);
    assert!(fe.game_input());
    fe
}

fn paused() -> FrontEnd {
    let mut fe = play();
    let c = press(&mut fe, Nav::Start);
    assert_eq!(fe.mode(), Mode::Paused);
    assert!(c.contains(&Command::SetPaused(true)));
    assert!(!fe.game_input());
    fe
}

#[test]
fn new_game_continue_and_quit() {
    // New Game over a save asks; the safe choice is the default and goes back.
    let mut fe = to_menu(true);
    press(&mut fe, Nav::Down);
    press(&mut fe, Nav::Confirm);
    assert_eq!(fe.mode(), Mode::Confirm);
    let c = press(&mut fe, Nav::Confirm);
    assert_eq!(fe.mode(), Mode::Menu);
    assert!(loads(&c).is_empty());
    press(&mut fe, Nav::Confirm);
    press(&mut fe, Nav::Right);
    let c = press(&mut fe, Nav::Confirm);
    assert_eq!(c.iter().position(|x| *x == Command::NewGame), Some(1), "after the confirm sound");
    assert_eq!(loads(&c), vec![("blacktap/round/1".to_string(), 0, false)]);
    assert_eq!(fe.mode(), Mode::Play);
    // New Game without a save starts at once.
    let mut fe = to_menu(false);
    let c = press(&mut fe, Nav::Confirm);
    assert!(c.contains(&Command::NewGame));
    assert_eq!(fe.mode(), Mode::Play);
    // Continue.
    let mut fe = to_menu(true);
    let c = press(&mut fe, Nav::Confirm);
    let start = c.iter().position(|x| *x == Command::StartGame).expect("StartGame");
    let load = c.iter().position(|x| matches!(x, Command::LoadScene { .. })).expect("LoadScene");
    assert!(start < load);
    // Quit asks, then quits.
    let mut fe = to_menu(true);
    press(&mut fe, Nav::Up);
    press(&mut fe, Nav::Confirm);
    assert_eq!(fe.mode(), Mode::Confirm);
    press(&mut fe, Nav::Left);
    let c = press(&mut fe, Nav::Confirm);
    assert!(c.contains(&Command::Quit));
}

#[test]
fn chapters_load_their_scene() {
    let mut fe = to_menu(true);
    press(&mut fe, Nav::Down);
    press(&mut fe, Nav::Down);
    press(&mut fe, Nav::Confirm);
    press(&mut fe, Nav::Down);
    press(&mut fe, Nav::Down);
    let c = press(&mut fe, Nav::Confirm);
    assert_eq!(loads(&c), vec![("blacktap/round/3".to_string(), 0, false)]);
    assert_eq!(fe.mode(), Mode::Play);
}

#[test]
fn settings_change_live_reset_and_save() {
    let mut fe = to_menu(false);
    for _ in 0..3 {
        press(&mut fe, Nav::Down);
    }
    press(&mut fe, Nav::Confirm);
    // Audio tab, Master volume.
    press(&mut fe, Nav::TabNext);
    press(&mut fe, Nav::TabNext);
    assert_eq!(fe.focus_id(), "settings/audio/master");
    let before = fe.settings().audio.master;
    let c = press(&mut fe, Nav::Right);
    assert!(fe.settings().audio.master > before);
    assert!(c.contains(&Command::SettingsChanged));
    assert_eq!(sounds(&c), vec!["ui_slider"]);
    // △ resets the row.
    let c = press(&mut fe, Nav::Context);
    assert_eq!(fe.settings().audio.master, before);
    assert!(c.contains(&Command::SettingsChanged));
    // Change two rows, then Defaults resets the tab.
    press(&mut fe, Nav::Left);
    press(&mut fe, Nav::Down);
    press(&mut fe, Nav::Left);
    assert_ne!(fe.settings().audio, Settings::default().audio);
    press(&mut fe, Nav::Up);
    press(&mut fe, Nav::Up);
    assert_eq!(fe.focus_id(), "settings/audio/defaults");
    let c = press(&mut fe, Nav::Confirm);
    assert_eq!(fe.settings().audio, Settings::default().audio);
    assert!(c.contains(&Command::SettingsChanged));
    // Pickers wrap and toggle with a toggle sound.
    press(&mut fe, Nav::TabNext);
    press(&mut fe, Nav::TabNext);
    assert_eq!(fe.focus_id(), "settings/accessibility/subtitles");
    let c = press(&mut fe, Nav::Right);
    assert!(!fe.settings().accessibility.subtitles);
    assert_eq!(sounds(&c), vec!["ui_toggle"]);
    let c = press(&mut fe, Nav::Back);
    assert!(c.contains(&Command::SaveSettings));
}

#[test]
fn button_layout_pages_open_and_close() {
    let mut fe = to_menu(false);
    for _ in 0..3 {
        press(&mut fe, Nav::Down);
    }
    press(&mut fe, Nav::Confirm);
    for _ in 0..5 {
        press(&mut fe, Nav::Down);
    }
    assert_eq!(fe.focus_id(), "settings/controls/buttonlayout");
    press(&mut fe, Nav::Confirm);
    assert_eq!(fe.focus_id(), "settings/layout/page/0");
    assert!(fe.focusables().len() >= 3, "15 actions + move, camera, pause: 3 pages");
    press(&mut fe, Nav::Right);
    assert_eq!(fe.focus_id(), "settings/layout/page/1");
    press(&mut fe, Nav::Back);
    assert_eq!(fe.mode(), Mode::Settings);
    assert_eq!(fe.focus_id(), "settings/controls/buttonlayout");
}

#[test]
fn play_pause_and_game_requests() {
    let mut fe = play();
    // B (East) in play is the game's: no pause.
    press(&mut fe, Nav::Back);
    assert_eq!(fe.mode(), Mode::Play);
    let mut fe = paused();
    press(&mut fe, Nav::Start);
    assert_eq!(fe.mode(), Mode::Play);
    // Restart from checkpoint.
    let mut fe = paused();
    for _ in 0..4 {
        press(&mut fe, Nav::Down);
    }
    let c = press(&mut fe, Nav::Confirm);
    assert_eq!(&c[1..], &[Command::RestartCheckpoint, Command::SetPaused(false)]);
    assert_eq!(fe.mode(), Mode::Play);
    // Quit to title: confirm.
    let mut fe = paused();
    press(&mut fe, Nav::Up);
    press(&mut fe, Nav::Confirm);
    assert_eq!(fe.mode(), Mode::Confirm);
    press(&mut fe, Nav::Right);
    let c = press(&mut fe, Nav::Confirm);
    assert!(c.contains(&Command::SetPaused(false)));
    assert_eq!(loads(&c), vec![("blacktap/title".to_string(), 0, false)]);
    assert_eq!(fe.mode(), Mode::Title);
    // The game asks for the title.
    let mut fe = play();
    let mut frame = PlayFrame::default();
    frame.ui.request = Some(UiRequest::Title);
    let c = fe.update(DT, &UiInput::default(), Some(&frame));
    assert_eq!(fe.mode(), Mode::Title);
    assert_eq!(loads(&c).len(), 1);
    // The map from the pause menu pans and zooms; △ toggles the legend.
    let mut fe = paused();
    press(&mut fe, Nav::Map);
    assert_eq!(fe.mode(), Mode::PauseMap);
    let c = press(&mut fe, Nav::Context);
    assert_eq!(sounds(&c), vec!["ui_toggle"]);
    press(&mut fe, Nav::Start);
    assert_eq!(fe.mode(), Mode::Play);
}

// ---------------------------------------------------------------- attract

fn demo_frame(done: bool) -> PlayFrame {
    let mut f = PlayFrame::default();
    f.ui.demo = Some(DemoState { caption: "x".into(), done, ..Default::default() });
    f
}

#[test]
fn idle_on_the_title_starts_attract_mode() {
    let mut fe = common::desktop(false);
    fe.update(DT, &UiInput::default(), None);
    let c = idle(&mut fe, 9.9, 0.1, None);
    assert_eq!(fe.mode(), Mode::Title);
    assert!(loads(&c).is_empty());
    let c = idle(&mut fe, 1.0, 0.1, None);
    assert_eq!(fe.mode(), Mode::Attract);
    assert_eq!(loads(&c), vec![("blacktap/round/2".to_string(), 11, true)]);
    // From the menu and settings too, and with slow frames (2 s steps).
    for path in [vec![], vec![Nav::Down, Nav::Down, Nav::Down, Nav::Confirm]] {
        let mut fe = to_menu(false);
        for n in &path {
            press(&mut fe, *n);
        }
        let c = idle(&mut fe, 12.1, 2.0, None);
        assert_eq!(fe.mode(), Mode::Attract);
        assert_eq!(loads(&c).len(), 1);
    }
}

#[test]
fn input_resets_the_idle_timer() {
    let mut fe = to_menu(false);
    idle(&mut fe, 9.0, 0.1, None);
    press(&mut fe, Nav::Down);
    idle(&mut fe, 9.0, 0.1, None);
    assert_eq!(fe.mode(), Mode::Menu);
}

#[test]
fn no_attract_from_play_or_pause() {
    let mut fe = play();
    idle(&mut fe, 120.0, 0.5, None);
    assert_eq!(fe.mode(), Mode::Play);
    let mut fe = paused();
    idle(&mut fe, 120.0, 0.5, None);
    assert_eq!(fe.mode(), Mode::Paused);
}

fn attract() -> FrontEnd {
    let mut fe = common::desktop(false);
    fe.update(DT, &UiInput::default(), None);
    idle(&mut fe, 10.6, 0.1, None);
    assert_eq!(fe.mode(), Mode::Attract);
    fe
}

#[test]
fn any_input_leaves_attract_on_the_same_update_and_is_consumed() {
    for input in [
        UiInput::press(Nav::Confirm),
        UiInput { nav: vec![Nav::Down, Nav::Confirm, Nav::Start], any: true, ..Default::default() },
        UiInput::any_key(),
        UiInput::click_at(egui::pos2(500.0, 500.0)),
    ] {
        let mut fe = attract();
        fe.on_scene_loaded();
        let c = fe.update(DT, &input, None);
        assert_eq!(fe.mode(), Mode::Title, "{input:?}");
        assert_eq!(loads(&c), vec![("blacktap/title".to_string(), 0, false)]);
        assert!(sounds(&c).is_empty(), "consumed: no menu sound");
        // Next frame: nothing pending.
        let c = fe.update(DT, &UiInput::default(), None);
        assert!(c.is_empty());
        assert_eq!(fe.mode(), Mode::Title);
    }
}

#[test]
fn segments_advance_on_done_and_on_time() {
    let mut fe = attract();
    // Time doesn't count before the scene is loaded.
    let c = idle(&mut fe, 100.0, 0.5, None);
    assert!(loads(&c).is_empty());
    fe.on_scene_loaded();
    // done → next segment.
    let c = fe.update(DT, &UiInput::default(), Some(&demo_frame(true)));
    assert_eq!(loads(&c), vec![("blacktap/round/3".to_string(), 11, true)]);
    // A stale done frame doesn't skip the next one before it loads.
    let c = fe.update(DT, &UiInput::default(), Some(&demo_frame(true)));
    assert!(loads(&c).is_empty());
    fe.on_scene_loaded();
    // On time: segment 2 lasts 50 s.
    let c = idle(&mut fe, 49.0, 0.25, Some(&demo_frame(false)));
    assert!(loads(&c).is_empty());
    let c = idle(&mut fe, 1.5, 0.25, Some(&demo_frame(false)));
    assert_eq!(loads(&c), vec![("blacktap/round/4".to_string(), 11, true)]);
}

#[test]
fn after_the_last_segment_a_card_then_the_next_seed() {
    let mut fe = attract();
    let mut seen = Vec::new();
    for _ in 0..5 {
        fe.on_scene_loaded();
        let c = fe.update(DT, &UiInput::default(), Some(&demo_frame(true)));
        seen.extend(loads(&c));
    }
    assert_eq!(seen.len(), 4, "segments 2..5 loaded");
    assert_eq!(fe.mode(), Mode::AttractCard);
    let c = idle(&mut fe, 2.9, 0.1, None);
    assert!(loads(&c).is_empty());
    assert_eq!(fe.mode(), Mode::AttractCard);
    let c = idle(&mut fe, 0.3, 0.1, None);
    assert_eq!(loads(&c), vec![("blacktap/round/2".to_string(), 22, true)]);
    assert_eq!(fe.mode(), Mode::Attract);
    // Any input on the card leaves too.
    let mut fe2 = fe.clone();
    for _ in 0..5 {
        fe2.on_scene_loaded();
        fe2.update(DT, &UiInput::default(), Some(&demo_frame(true)));
    }
    assert_eq!(fe2.mode(), Mode::AttractCard);
    fe2.update(DT, &UiInput::press(Nav::Back), None);
    assert_eq!(fe2.mode(), Mode::Title);
}

#[test]
fn an_hour_of_attract_mode_does_not_grow() {
    let mut fe = attract();
    let frame = demo_frame(false);
    let mut loads_seen = 0usize;
    let mut size_at_minute = 0;
    let steps = (3600.0 / (1.0 / 30.0)) as usize;
    for i in 0..steps {
        let c = fe.update(1.0 / 30.0, &UiInput::default(), Some(&frame));
        let n = loads(&c).len();
        assert!(c.len() <= 2, "a frame emits at most a load: {c:?}");
        if n > 0 {
            loads_seen += n;
            fe.on_scene_loaded();
        }
        if i == 1800 {
            size_at_minute = fe.footprint();
        }
    }
    assert!(loads_seen > 50, "the playlist kept looping ({loads_seen} loads)");
    assert_eq!(fe.footprint(), size_at_minute);
    assert!(fe.mode().is_attract());
}

#[test]
fn kiosk_starts_in_attract_and_returns_after_a_minute() {
    let mut fe = FrontEnd::new(common::info(), Settings::default(), Platform { kiosk: true, ..Default::default() }, false);
    assert_eq!(fe.mode(), Mode::Attract);
    let c = fe.update(DT, &UiInput::default(), None);
    assert_eq!(loads(&c), vec![("blacktap/round/2".to_string(), 11, true)]);
    fe.update(DT, &UiInput::press(Nav::Confirm), None);
    assert_eq!(fe.mode(), Mode::Title);
    // Into play, pause, then walk away.
    press(&mut fe, Nav::Confirm);
    press(&mut fe, Nav::Down);
    press(&mut fe, Nav::Down);
    press(&mut fe, Nav::Confirm);
    assert_eq!(fe.mode(), Mode::Play);
    press(&mut fe, Nav::Start);
    assert_eq!(fe.mode(), Mode::Paused);
    let c = idle(&mut fe, 59.0, 0.5, None);
    assert!(loads(&c).is_empty());
    let c = idle(&mut fe, 1.5, 0.5, None);
    assert_eq!(fe.mode(), Mode::Attract);
    assert!(c.contains(&Command::SetPaused(false)));
    assert_eq!(loads(&c).len(), 1);
}

// ---------------------------------------------------------------- mobile

fn mobile() -> FrontEnd {
    let mut fe = FrontEnd::new(common::info(), Settings::default(), Platform { mobile: true, ..Default::default() }, false);
    assert_eq!(fe.mode(), Mode::Attract);
    let c = fe.update(DT, &UiInput::default(), None);
    assert_eq!(loads(&c).len(), 1);
    fe.on_scene_loaded();
    fe
}

#[test]
fn mobile_stays_in_attract_and_touch_does_not_exit() {
    let mut fe = mobile();
    assert!(!fe.sound_on());
    for p in [egui::pos2(900.0, 500.0), egui::pos2(100.0, 900.0)] {
        fe.update(DT, &UiInput::tap(p), None);
        assert_eq!(fe.mode(), Mode::Attract);
    }
    // Long idle: still attract, the playlist goes on.
    let c = idle(&mut fe, 400.0, 0.5, None);
    assert!(fe.mode().is_attract());
    assert!(!loads(&c).is_empty());
}

#[test]
fn mobile_menu_opens_from_the_button() {
    let mut fe = mobile();
    let size = egui::vec2(2337.0, 1080.0); // 844×390 phone landscape in points
    draw_once(&mut fe, None, size);
    let c = fe.menu_button().expect("☰ shown");
    assert!(c.x > size.x * 0.85 && c.y < size.y * 0.2, "top right: {c:?}");
    fe.update(DT, &UiInput::tap(c), None);
    assert_eq!(fe.mode(), Mode::MobileMenu);
    assert_eq!(fe.mode().name(), "mobile_menu");
    assert!(fe.menu_button().is_none());
    // Sound on: unlock + toggle.
    draw_once(&mut fe, None, size);
    let row = |i: usize| egui::pos2(size.x * 0.5, size.y * 0.5 - 430.0 + 170.0 + i as f32 * 150.0 + 65.0);
    let c = fe.update(DT, &UiInput::tap(row(0)), None);
    assert!(c.contains(&Command::UnlockAudio));
    assert!(c.contains(&Command::ToggleSound(true)));
    assert!(fe.sound_on());
    // Quality cycles.
    let q = fe.settings().display.quality;
    let c = fe.update(DT, &UiInput::tap(row(1)), None);
    assert_ne!(fe.settings().display.quality, q);
    assert!(c.contains(&Command::SettingsChanged));
    // About, then back, then Close.
    fe.update(DT, &UiInput::tap(row(2)), None);
    assert_eq!(fe.mode(), Mode::MobileAbout);
    draw_once(&mut fe, None, size);
    let close = egui::pos2(size.x * 0.5 + (size.x * 0.8).min(1700.0) * 0.5 - 260.0, size.y * 0.5 + 450.0 - 100.0);
    fe.update(DT, &UiInput::tap(close), None);
    assert_eq!(fe.mode(), Mode::MobileMenu);
    draw_once(&mut fe, None, size);
    fe.update(DT, &UiInput::tap(row(3)), None);
    assert_eq!(fe.mode(), Mode::Attract);
    // The ☰ works even before any draw.
    let mut fresh = mobile();
    let c = fresh.menu_button().expect("☰");
    fresh.update(DT, &UiInput::tap(c), None);
    assert_eq!(fresh.mode(), Mode::MobileMenu);
}

#[test]
fn a_keyboard_or_pad_on_mobile_goes_to_the_title() {
    let mut fe = mobile();
    let c = fe.update(DT, &UiInput::any_key(), None);
    assert_eq!(fe.mode(), Mode::Title);
    assert_eq!(loads(&c), vec![("blacktap/title".to_string(), 0, false)]);
    // From then on it behaves as desktop: idle → attract, touch exits.
    idle(&mut fe, 11.0, 0.1, None);
    assert_eq!(fe.mode(), Mode::Attract);
    assert!(fe.menu_button().is_none());
    fe.update(DT, &UiInput::tap(egui::pos2(10.0, 10.0)), None);
    assert_eq!(fe.mode(), Mode::Title);
    // A pad press in the mobile menu too.
    let mut fe = mobile();
    let c = fe.menu_button().expect("☰");
    fe.update(DT, &UiInput::tap(c), None);
    fe.update(DT, &UiInput::press(Nav::Down), None);
    assert_eq!(fe.mode(), Mode::Title);
}

// ---------------------------------------------------------------- pointer

#[test]
fn hover_moves_focus_and_click_confirms() {
    let mut fe = to_menu(false);
    draw_once(&mut fe, None, egui::vec2(1920.0, 1080.0));
    // Items are 85 points apart from y ≈ 380 (the 6-item menu).
    let ids = fe.focusables();
    let mut found = None;
    for y in (300..900).step_by(10) {
        let mut g = fe.clone();
        let input = UiInput {
            pointer: Some(egui::pos2(300.0, y as f32)),
            pointer_moved: true,
            any: true,
            device: GlyphFamily::Keyboard,
            ..Default::default()
        };
        let c = g.update(DT, &input, None);
        if g.focus_id() == "menu/settings" {
            assert_eq!(sounds(&c), vec!["ui_move"]);
            found = Some(y as f32);
            break;
        }
    }
    let y = found.unwrap_or_else(|| panic!("hovering never reached Settings ({ids:?})"));
    fe.update(DT, &UiInput::click_at(egui::pos2(300.0, y)), None);
    assert_eq!(fe.mode(), Mode::Settings);
    // Right click goes back.
    fe.update(DT, &UiInput { back_click: true, any: true, ..Default::default() }, None);
    assert_eq!(fe.mode(), Mode::Menu);
    // A click in the settings' ▶ arrow changes the value.
    let mut fe = to_menu(false);
    for _ in 0..3 {
        press(&mut fe, Nav::Down);
    }
    press(&mut fe, Nav::Confirm);
    press(&mut fe, Nav::TabNext);
    press(&mut fe, Nav::TabNext);
    draw_once(&mut fe, None, egui::vec2(1920.0, 1080.0));
    let before = fe.settings().audio.music;
    let mut changed = false;
    'outer: for y in (240..900).step_by(8) {
        for x in (1500..1800).step_by(8) {
            let mut g = fe.clone();
            g.update(DT, &UiInput::click_at(egui::pos2(x as f32, y as f32)), None);
            if g.settings().audio.music > before {
                changed = true;
                break 'outer;
            }
        }
    }
    assert!(changed, "clicking a ▶ arrow raised the music volume");
}

#[test]
fn hud_draws_through_the_front_end_in_play() {
    let mut fe = play();
    let mut frame = PlayFrame { ui: pav_ui::sample_ui_model(), ..Default::default() };
    frame.ui.markers[0].pos = Vec3::new(0.0, 0.0, -50.0);
    draw_once(&mut fe, Some(&frame), egui::vec2(1920.0, 1080.0));
}
