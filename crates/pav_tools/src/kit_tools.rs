//! Tools for hook games with a front end (`Entry::front_end`, e.g. blacktap): the game's UI
//! rendered headlessly (`ui`, `capture ui=true`), attract mode run faster than real time
//! (`attract`), and the game's own commands (`ship`, `mission`, `perception`, `ocean`,
//! `level_check`, `tour`) through `Game::command`.

use std::path::PathBuf;
use std::time::Instant;

use anyhow::{Context, Result, anyhow, bail};
use pav_core::Sim;
use pav_core::play::{FrontEndInfo, PlayFrame};
use pav_ui::frontend::{Command, FrontEnd, Mode, Platform};
use pav_ui::glyph::GlyphFamily;
use pav_ui::input::{Nav, UiInput};
use pav_ui::{Bindings, HudEnv, Projector, Settings, Theme, raster};

use serde_json::{Value, json};

use crate::session::Session;
use crate::tools::{Args, Output, get_bool, get_str, get_u64};

/// The front end of the running game (or of `game=<key>`).
fn front_end(s: &Session, a: &Args) -> Result<FrontEndInfo> {
    let key = match get_str(a, "game") {
        Some(k) => k.to_string(),
        None => s.sim.play().map(|g| g.key().to_string()).unwrap_or_else(|| "blacktap".into()),
    };
    let entry = pav_core::games::find(&key).ok_or_else(|| anyhow!("no game '{key}'"))?;
    let make = entry.front_end.ok_or_else(|| anyhow!("game '{key}' has no front end"))?;
    Ok(make())
}

fn family(a: &Args) -> Result<GlyphFamily> {
    Ok(match get_str(a, "family").unwrap_or("playstation") {
        "playstation" | "ps" | "dualsense" => GlyphFamily::PlayStation,
        "xbox" => GlyphFamily::Xbox,
        "keyboard" | "kb" => GlyphFamily::Keyboard,
        other => bail!("family is playstation, xbox or keyboard (not '{other}')"),
    })
}

fn nav_of(name: &str) -> Result<Nav> {
    Ok(match name.trim() {
        "up" => Nav::Up,
        "down" => Nav::Down,
        "left" => Nav::Left,
        "right" => Nav::Right,
        "confirm" | "a" | "cross" => Nav::Confirm,
        "back" | "b" | "circle" => Nav::Back,
        "start" | "options" => Nav::Start,
        "tab_prev" | "l1" => Nav::TabPrev,
        "tab_next" | "r1" => Nav::TabNext,
        "context" => Nav::Context,
        "map" => Nav::Map,
        other => bail!("unknown nav '{other}'"),
    })
}

struct UiKit {
    theme: Theme,
    bindings: Bindings,
}

impl UiKit {
    fn new(info: &FrontEndInfo) -> Result<Self> {
        Ok(Self {
            theme: Theme::from_toml(&info.theme).context("ui theme")?,
            bindings: Bindings::from_toml(&info.bindings).context("bindings")?,
        })
    }

    /// The UI layer at `size` pixels (straight alpha): the HUD of `frame`, or the front end.
    fn render(
        &self,
        size: [u32; 2],
        view_proj: Option<glam::Mat4>,
        family: GlyphFamily,
        settings: &Settings,
        mut draw: impl FnMut(&egui::Context, &HudEnv),
    ) -> Vec<u8> {
        let ppp = size[1] as f32 / 1080.0;
        let pts = egui::vec2(size[0] as f32 / ppp, size[1] as f32 / ppp);
        let proj = match view_proj {
            Some(vp) => Projector::new(vp, pts),
            None => pav_ui::sample_projector(pts),
        };
        let theme = &self.theme;
        raster::render(
            size,
            ppp,
            |ctx| pav_ui::theme::install(ctx, theme),
            |ctx| {
                let env = HudEnv { theme, bindings: &self.bindings, family, settings, proj, time: 1.0 };
                draw(ctx, &env);
            },
        )
    }
}

fn mode_of(name: &str) -> Result<Mode> {
    Mode::ALL
        .iter()
        .copied()
        .find(|m| m.name() == name)
        .ok_or_else(|| anyhow!("unknown screen '{name}' (hud, or one of: {})", Mode::ALL.map(|m| m.name()).join(", ")))
}

fn size(a: &Args, w: u64, h: u64) -> Result<[u32; 2]> {
    let w = get_u64(a, "width", w)? as u32;
    let h = get_u64(a, "height", h)? as u32;
    if w == 0 || h == 0 || w > 8192 || h > 8192 {
        bail!("width/height must be 1..8192");
    }
    Ok([w, h])
}

fn write_png(path: &PathBuf, w: u32, h: u32, rgba: &[u8]) -> Result<Vec<u8>> {
    let png = pav_render::capture::encode_png(w, h, rgba)?;
    if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(path, &png)?;
    Ok(png)
}

/// The game's HUD drawn over a rendered frame (`capture ui=true`).
pub fn composite_hud(s: &mut Session, a: &Args, w: u32, h: u32) -> Result<(Vec<u8>, Value)> {
    let info = front_end(s, a)?;
    let kit = UiKit::new(&info)?;
    let (mut rgba, vp) = s.render_with_camera(w, h)?;
    let frame = s.sim.frame();
    let model = frame.play.as_ref().map(|p| p.ui.clone()).unwrap_or_default();
    let settings = Settings::default();
    let layer = kit.render([w, h], Some(vp), family(a)?, &settings, |ctx, env| pav_ui::hud::draw(ctx, &model, env));
    let coverage = raster::coverage(&layer);
    raster::composite_over(&mut rgba, &layer);
    Ok((rgba, json!({ "ui_coverage": (coverage * 1000.0).round() / 1000.0 })))
}

/// `ui`: renders a front-end screen or the HUD to PNG (over the live frame when a game runs).
pub fn t_ui(s: &mut Session, a: &Args) -> Result<Output> {
    let screen = get_str(a, "screen").unwrap_or("hud").to_string();
    let [w, h] = size(a, 1920, 1080)?;
    let fam = family(a)?;
    let info = front_end(s, a)?;
    let kit = UiKit::new(&info)?;
    let sample = get_bool(a, "sample", s.sim.play().is_none())?;
    let backdrop = get_bool(a, "backdrop", s.sim.play().is_some())?;
    let t = Instant::now();
    let (mut base, vp) = if backdrop {
        let (rgba, vp) = s.render_with_camera(w, h)?;
        (Some(rgba), Some(vp))
    } else {
        (None, None)
    };
    let live = s.sim.frame().play.as_deref().cloned();
    let frame: PlayFrame = match (&live, sample) {
        (Some(f), false) => f.clone(),
        _ => PlayFrame { ui: pav_ui::sample_ui_model(), ..Default::default() },
    };
    let settings = Settings::default();
    let mut reply = json!({ "screen": screen });
    let layer = if screen == "hud" {
        kit.render([w, h], vp, fam, &settings, |ctx, env| pav_ui::hud::draw(ctx, &frame.ui, env))
    } else {
        let mode = mode_of(&screen)?;
        let mobile = matches!(mode, Mode::MobileMenu | Mode::MobileAbout);
        let platform = Platform { mobile, kiosk: false, can_quit: true };
        let mut fe = FrontEnd::new(info.clone(), settings.clone(), platform, true);
        fe.show(mode);
        // Scripted focus: each step draws (the hit boxes) then updates with one press.
        let steps: Vec<Nav> = match get_str(a, "nav") {
            Some(list) if !list.trim().is_empty() => list.split(',').map(nav_of).collect::<Result<_>>()?,
            _ => Vec::new(),
        };
        let mut sounds = Vec::new();
        for nav in steps {
            let _ = kit.render([w / 4, h / 4], vp, fam, &settings, |ctx, env| fe.draw(ctx, Some(&frame), env));
            for c in fe.update(0.1, &UiInput::press(nav), Some(&frame)) {
                if let Command::Sound(n) = c {
                    sounds.push(n);
                }
            }
        }
        let fe_settings = fe.settings().clone();
        let layer = kit.render([w, h], vp, fam, &fe_settings, |ctx, env| fe.draw(ctx, Some(&frame), env));
        reply["mode"] = json!(fe.mode().name());
        reply["focus"] = json!(fe.focus_id());
        reply["focusables"] = json!(fe.focusables());
        reply["sounds"] = json!(sounds);
        layer
    };
    reply["ui_coverage"] = json!((raster::coverage(&layer) * 1000.0).round() / 1000.0);
    let rgba = match base.as_mut() {
        Some(b) => {
            raster::composite_over(b, &layer);
            std::mem::take(b)
        }
        None => layer,
    };
    let path = PathBuf::from(get_str(a, "out").map(String::from).unwrap_or(format!("out/ui-{screen}.png")));
    let png = write_png(&path, w, h, &rgba)?;
    reply["path"] = json!(path);
    reply["width"] = json!(w);
    reply["height"] = json!(h);
    reply["render_ms"] = json!(t.elapsed().as_millis());
    Ok(Output::Image { png, path: Some(path), meta: reply })
}

/// One attract segment as the `attract` tool saw it.
#[derive(Clone, Debug, Default, serde::Serialize)]
pub struct SegmentRun {
    pub scene: String,
    pub seed: u64,
    pub seconds: f32,
    pub caption: String,
    pub events: u32,
    pub mechanics: Vec<String>,
    pub stalled: bool,
    pub done: bool,
}

/// A headless attract run: the real front end, the real game with its bot, and a fake clock.
pub struct AttractRun {
    pub segments: Vec<SegmentRun>,
    pub loops: u32,
    pub stalls: u32,
    pub load_errors: Vec<String>,
    /// Ticks simulated and wall time spent.
    pub ticks: u64,
    pub wall: f32,
    /// Exit checks: any input in attract returned to the title on the same update.
    pub exits_ok: u32,
    pub exits_failed: u32,
    pub max_footprint: usize,
}

/// Runs the front end idle on the title until attract starts, then the playlist for `minutes`
/// of game time, stepping `step` seconds per front-end update. Every `exit_every` seconds of
/// attract (0 = never) a key press checks the same-frame exit, then idles back into attract.
pub fn run_attract(info: &FrontEndInfo, minutes: f32, step: f32, exit_every: f32, first_loop: u32) -> Result<AttractRun> {
    let mut fe =
        FrontEnd::new(info.clone(), Settings::default(), Platform { mobile: false, kiosk: false, can_quit: true }, false);
    let mut sim: Option<Sim> = None;
    let mut run = AttractRun {
        segments: Vec::new(),
        loops: first_loop,
        stalls: 0,
        load_errors: Vec::new(),
        ticks: 0,
        wall: 0.0,
        exits_ok: 0,
        exits_failed: 0,
        max_footprint: 0,
    };
    let t0 = Instant::now();
    let total = minutes * 60.0;
    let mut clock = 0.0f32;
    let mut since_exit = 0.0f32;
    let mut current: Option<SegmentRun> = None;
    let mut autopilot = false;
    let idle = UiInput::default();
    while clock < total {
        let frame = sim.as_mut().map(|s| s.frame());
        let play = frame.as_ref().and_then(|f| f.play.as_deref());
        let mut input = &idle;
        let press = UiInput::any_key();
        let in_attract = fe.mode() == Mode::Attract;
        if exit_every > 0.0 && in_attract && since_exit >= exit_every {
            input = &press;
            since_exit = 0.0;
        }
        let checking = std::ptr::eq(input, &press);
        let cmds = fe.update(step, input, play);
        if checking {
            if fe.mode() == Mode::Title {
                run.exits_ok += 1;
            } else {
                run.exits_failed += 1;
            }
        }
        run.max_footprint = run.max_footprint.max(fe.footprint());
        for c in cmds {
            if let Command::LoadScene { scene, seed, autopilot: auto } = c {
                if let Some(seg) = current.take() {
                    run.stalls += seg.stalled as u32;
                    run.segments.push(seg);
                }
                autopilot = auto;
                match Sim::new(&scene, seed) {
                    Ok(mut fresh) => {
                        if let Some(g) = fresh.state.play.as_mut() {
                            g.restore(&json!({ "demo": auto }));
                        }
                        sim = Some(fresh);
                        fe.on_scene_loaded();
                    }
                    Err(e) => {
                        run.load_errors.push(format!("{scene}: {e:#}"));
                        sim = None;
                    }
                }
                if auto {
                    if scene == info.demo.first().map(|d| d.scene.as_str()).unwrap_or("") && !run.segments.is_empty() {
                        run.loops += 1;
                    }
                    current = Some(SegmentRun { scene, seed, ..Default::default() });
                }
            }
        }
        // The game runs: the bot plays in attract; on the title the backdrop idles.
        if let Some(s) = sim.as_mut() {
            let hz = s.config.tick_rate.hz() as f32;
            let n = (step * hz).round() as u64;
            for _ in 0..n {
                let input = if autopilot { s.bot_input() } else { Default::default() };
                s.step(&input);
                let _ = s.drain_events();
            }
            run.ticks += n;
            if let (Some(seg), Some(demo)) = (current.as_mut(), s.frame().play.as_ref().and_then(|p| p.ui.demo.clone())) {
                seg.caption = demo.caption.clone();
                seg.events = demo.events;
                seg.mechanics = demo.mechanics.clone();
                seg.stalled |= demo.stalled;
                seg.done |= demo.done;
            }
        }
        if let Some(seg) = current.as_mut() {
            seg.seconds += step;
        }
        if fe.mode().is_attract() {
            since_exit += step;
            clock += step;
        }
    }
    if let Some(seg) = current.take() {
        run.stalls += seg.stalled as u32;
        run.segments.push(seg);
    }
    run.wall = t0.elapsed().as_secs_f32();
    Ok(run)
}

/// `attract`: runs the attract playlist headless and reports every segment.
pub fn t_attract(s: &mut Session, a: &Args) -> Result<Output> {
    let info = front_end(s, a)?;
    let minutes = crate::tools::get_f32(a, "minutes", 6.0)?.clamp(0.05, 600.0);
    let step = crate::tools::get_f32(a, "step", 0.5)?.clamp(1.0 / 60.0, 2.0);
    let exit_every = crate::tools::get_f32(a, "exit_every", 0.0)?.max(0.0);
    let run = run_attract(&info, minutes, step, exit_every, 0)?;
    let segs: Vec<Value> = run
        .segments
        .iter()
        .map(|g| {
            json!({
                "scene": g.scene, "seed": g.seed, "seconds": (g.seconds * 10.0).round() / 10.0,
                "events": g.events, "mechanics": g.mechanics, "stalled": g.stalled, "done": g.done,
            })
        })
        .collect();
    let thin = run.segments.iter().filter(|g| g.mechanics.len() < 3 && g.seconds > 5.0).count();
    Ok(Output::Json(json!({
        "minutes": minutes,
        "segments": segs.len(),
        "loops": run.loops,
        "stalls": run.stalls,
        "segments_under_3_mechanics": thin,
        "load_errors": run.load_errors,
        "exits_ok": run.exits_ok,
        "exits_failed": run.exits_failed,
        "ticks": run.ticks,
        "wall_seconds": (run.wall * 10.0).round() / 10.0,
        "front_end_footprint": run.max_footprint,
        "runs": segs,
    })))
}

/// A tool answered by the running game (`Game::command`).
fn game_command(s: &mut Session, name: &str, a: &Args) -> Result<Value> {
    let args = Value::Object(a.clone());
    if s.sim.play().is_none() {
        bail!("no game is running: `load scene=blacktap/...` first");
    }
    let out = s.sim.game_command(name, &args).ok_or_else(|| anyhow!("this game has no `{name}` command"))?;
    let v = out.map_err(|e| anyhow!(e))?;
    s.keep_events();
    s.sync_camera();
    Ok(v)
}

/// `gcmd cmd=<name> ...`: one of the running game's commands (`Game::command`).
pub fn t_gcmd(s: &mut Session, a: &Args) -> Result<Output> {
    let name = get_str(a, "cmd")
        .ok_or_else(|| anyhow!("cmd=<name>: ship, mission, perception, fighters, ocean, level_check, markers"))?
        .to_string();
    let mut rest = a.clone();
    rest.remove("cmd");
    Ok(Output::Json(game_command(s, &name, &rest)?))
}

/// A game command called by its own name (`pav ship do=sail`); None when the game has none.
pub fn try_game_command(s: &mut Session, name: &str, a: &Args) -> Option<Result<Output>> {
    s.sim.play()?;
    let reply = s.sim.game_command(name, &Value::Object(a.clone()))?;
    s.keep_events();
    s.sync_camera();
    Some(reply.map(Output::Json).map_err(|e| anyhow!(e)))
}

/// `ray`: what a ray from a point hits (tools for laying out levels and bot routes).
pub fn t_ray(s: &mut Session, a: &Args) -> Result<Output> {
    let from = crate::tools::vec_arg(a, "from")?.ok_or_else(|| anyhow!("from=[x,y,z]"))?;
    let dir = crate::tools::vec_arg(a, "dir")?.unwrap_or(vec![0.0, -1.0, 0.0]);
    if from.len() != 3 || dir.len() != 3 {
        bail!("from and dir are [x, y, z]");
    }
    let max = crate::tools::get_f32(a, "max", 50.0)?;
    let o = glam::Vec3::new(from[0], from[1], from[2]);
    let d = glam::Vec3::new(dir[0], dir[1], dir[2]);
    let r = |x: f32| (x * 1000.0).round() / 1000.0;
    Ok(Output::Json(match s.sim.raycast_block(o, d, max) {
        // Rapier's broad phase is filled by the first step: before it, nothing can be hit.
        None if s.sim.state.tick == 0 => {
            json!({ "hit": false, "searched": max, "note": "the scene hasn't stepped yet: run `step ticks=1` first" })
        }
        None => json!({ "hit": false, "searched": max, "note": "nothing within max= metres (raise max= to look further)" }),
        Some((h, b)) => json!({
            "hit": true,
            "point": [r(h.point.x), r(h.point.y), r(h.point.z)],
            "normal": [r(h.normal.x), r(h.normal.y), r(h.normal.z)],
            "distance": r((h.point - o).length()),
            "entity": h.entity.map(|e| e.0),
            "block": b.map(|b| json!({ "min": [r(b.min.x), r(b.min.y), r(b.min.z)], "max": [r(b.max.x), r(b.max.y), r(b.max.z)], "flags": b.flags, "material": b.material })),
        }),
    }))
}

/// `ocean`: the sea under a point (`at=[x, z]`), or the game sets the sea (`preset=storm`).
pub fn t_ocean(s: &mut Session, a: &Args) -> Result<Output> {
    if a.contains_key("preset") || a.contains_key("wind") {
        return Ok(Output::Json(game_command(s, "ocean", a)?));
    }
    let Some(ocean) = s.sim.state.ocean.as_ref() else { bail!("this scene has no ocean") };
    let at = crate::tools::vec_arg(a, "at")?.unwrap_or_else(|| vec![s.sim.state.focus.x, s.sim.state.focus.z]);
    if at.len() != 2 {
        bail!("at is [x, z]");
    }
    let p = glam::Vec2::new(at[0], at[1]);
    let surf = ocean.sea.sample(p, ocean.time);
    Ok(Output::Json(json!({
        "at": at, "time": ocean.time, "height": surf.height, "normal": surf.normal.to_array(),
        "max_height": ocean.sea.max_height(), "foam": ocean.sea.foam,
    })))
}

/// `tour`: the game's canonical vistas, captured into one contact sheet.
pub fn t_tour(s: &mut Session, a: &Args) -> Result<Output> {
    let list = game_command(s, "tour", a)?;
    let shots = list["shots"].as_array().cloned().unwrap_or_default();
    if shots.is_empty() {
        bail!("the game listed no tour shots");
    }
    let [w, h] = size(a, 640, 360)?;
    let stats = get_bool(a, "stats", false)?;
    let cols = 3usize;
    let rows = shots.len().div_ceil(cols);
    let (sw, sh) = (w as usize * cols, h as usize * rows);
    let mut sheet = vec![0u8; sw * sh * 4];
    let mut names = Vec::new();
    let t = Instant::now();
    for (i, shot) in shots.iter().enumerate() {
        let scene = shot["scene"].as_str().unwrap_or("blacktap/freeroam");
        let mut sess = Session::new(scene, shot["seed"].as_u64().unwrap_or(1))?;
        sess.gpu = s.gpu.take();
        // The caller's view settings and look (`set view.quality=low`, `look name=...`).
        sess.view = s.view.clone();
        sess.look = s.look.clone();
        let mut args = serde_json::Map::new();
        args.insert("shot".into(), json!(i));
        let setup = sess.sim.game_command("tour", &Value::Object(args.clone())).transpose().map_err(|e| anyhow!(e))?;
        sess.step(shot["ticks"].as_u64().unwrap_or(30));
        // Shots on something that moves are framed now, in their own scene.
        args.insert("frame".into(), json!(true));
        let framed = sess.sim.game_command("tour", &Value::Object(args)).transpose().map_err(|e| anyhow!(e))?;
        // One more tick shows what the frame call settled.
        sess.step(1);
        let pick = |k: &str| framed.as_ref().and_then(|f| f.get(k)).filter(|v| v.is_array()).unwrap_or(&shot[k]).clone();
        let floats = |v: Value| v.as_array().map(|v| v.iter().map(|x| x.as_f64().unwrap_or(0.0) as f32).collect::<Vec<_>>());
        let (eye, look) = (floats(pick("eye")), floats(pick("look")));
        if let (Some(e), Some(l)) = (eye, look) {
            if e.len() == 3 && l.len() == 3 {
                sess.shot = Some(pav_core::play::CameraHint {
                    eye: glam::Vec3::new(e[0], e[1], e[2]),
                    target: glam::Vec3::new(l[0], l[1], l[2]),
                    fov: shot["fov"].as_f64().unwrap_or(55.0) as f32,
                    far: shot["far"].as_f64().unwrap_or(4000.0) as f32,
                    cut: true,
                    fade_occluders: false,
                });
            }
        }
        let rgba = sess.render(w, h);
        let shot_stats = if stats && rgba.is_ok() { Some(crate::tools::render_stats(&mut sess)?) } else { None };
        s.gpu = sess.gpu.take();
        let rgba = rgba?;
        let (cx, cy) = (i % cols, i / cols);
        for y in 0..h as usize {
            let src = &rgba[y * w as usize * 4..(y + 1) * w as usize * 4];
            let dy = cy * h as usize + y;
            let start = (dy * sw + cx * w as usize) * 4;
            sheet[start..start + w as usize * 4].copy_from_slice(src);
        }
        let mut entry = json!({ "name": shot["name"], "scene": scene, "setup": setup });
        if let Some(st) = shot_stats {
            entry["stats"] = st;
        }
        names.push(entry);
    }
    let path = PathBuf::from(get_str(a, "out").unwrap_or("out/tour.png"));
    let png = write_png(&path, sw as u32, sh as u32, &sheet)?;
    let meta = json!({ "path": path, "shots": names, "width": sw, "height": sh, "render_ms": t.elapsed().as_millis() });
    Ok(Output::Image { png, path: Some(path), meta })
}
