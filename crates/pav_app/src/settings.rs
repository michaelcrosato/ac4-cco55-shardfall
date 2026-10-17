//! Startup settings: `shardfall.toml` next to the executable, overridden by command-line flags.

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// "vulkan" (default) or "dx12".
    pub backend: String,
    pub vsync: bool,
    pub width: u32,
    pub height: u32,
    pub fullscreen: bool,
    /// Scene to start in (empty = Shardfall's town; `world` = the pavilion). Command line:
    /// --scene NAME or --room NAME.
    pub scene: String,
    pub seed: u64,
    /// Live agent bridge address ("" = off). Command line: --bridge [ADDR].
    pub bridge: String,
    /// Open the animation workspace. Native startup also enables the live agent bridge.
    pub animation_studio: bool,
    /// Optional first clip in the animation workspace.
    pub animation_clip: String,
    /// Open the object workspace. Native startup also enables the live agent bridge.
    pub asset_studio: bool,
    pub asset_name: String,
    /// Open the native SpawnForge creature workspace.
    pub creature_studio: bool,
    pub creature_name: String,
    /// A scripted gamepad to play (command line only: --pad-script FILE).
    #[serde(skip)]
    pub pad_script: String,
    /// Developer mode: the engine's overlays, F-keys, rewind, scene list and studios in a
    /// game with a front end (`--dev`, `?dev=1`).
    #[serde(skip)]
    pub dev: bool,
    /// Store-display kiosk: start in attract mode and return to it after 60 s without input
    /// (`--kiosk`, `?kiosk=1`).
    #[serde(skip)]
    pub kiosk: bool,
}

/// The start scene when none is named: the first game with a front end (its title screen),
/// else Shardfall's town.
fn default_scene() -> String {
    pav_core::games::all().into_iter().find(|g| g.front_end.is_some()).map(|g| g.key.to_string()).unwrap_or_else(|| "town".into())
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            backend: "vulkan".into(),
            vsync: true,
            width: 1600,
            height: 900,
            fullscreen: false,
            scene: String::new(),
            seed: 1,
            bridge: String::new(),
            animation_studio: false,
            animation_clip: String::new(),
            asset_studio: false,
            asset_name: String::new(),
            creature_studio: false,
            creature_name: String::new(),
            pad_script: String::new(),
            dev: false,
            kiosk: false,
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
const HEADER: &str = "# Shardfall startup settings. Delete this file to restore defaults.\n# backend = \"vulkan\" or \"dx12\"\n";

impl Settings {
    /// In the browser: defaults, plus `?room=NAME&seed=N` from the page address.
    #[cfg(target_arch = "wasm32")]
    pub fn load(_args: &[String]) -> (Self, String) {
        let mut s = Settings::default();
        let query = web_sys::window().and_then(|w| w.location().search().ok()).unwrap_or_default();
        if let Ok(p) = web_sys::UrlSearchParams::new_with_str(&query) {
            if let Some(r) = p.get("room").filter(|r| !r.is_empty()) {
                s.scene = format!("world/{r}");
            }
            if let Some(r) = p.get("scene").filter(|r| !r.is_empty()) {
                s.scene = r;
            }
            if let Some(v) = p.get("seed").and_then(|v| v.parse().ok()) {
                s.seed = v;
            }
            s.animation_studio = p.get("animation_studio").is_some_and(|v| v != "false");
            s.animation_clip = p.get("clip").unwrap_or_default();
            s.asset_studio = p.get("asset_studio").is_some_and(|v| v != "false");
            s.asset_name = p.get("asset").unwrap_or_default();
            s.creature_studio = p.get("creature_studio").is_some_and(|v| v != "false");
            s.creature_name = p.get("creature").unwrap_or_default();
            s.dev = p.get("dev").is_some_and(|v| v != "0" && v != "false");
            s.kiosk = p.get("kiosk").is_some_and(|v| v != "0" && v != "false");
        }
        if (s.animation_studio || s.asset_studio || s.creature_studio) && s.scene.is_empty() {
            s.scene = "empty".into();
        }
        if s.scene.is_empty() {
            // The game's title screen (or Shardfall's town); the engine's pavilion is `world`.
            s.scene = default_scene();
        }
        (s, "page address (?room=NAME&seed=N)".into())
    }

    /// Loads settings (creating the file with defaults on first run), then applies CLI flags.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn load(args: &[String]) -> (Self, String) {
        let path = crate::boot::exe_dir().join("shardfall.toml");
        let mut note = String::new();
        let mut s = match std::fs::read_to_string(&path) {
            Ok(text) => match toml_from_str(&text) {
                Ok(s) => {
                    note = format!("{}", path.display());
                    s
                }
                Err(e) => {
                    log::warn!("{} is invalid ({e}); using defaults", path.display());
                    Settings::default()
                }
            },
            Err(_) => {
                let s = Settings::default();
                let _ = std::fs::write(&path, format!("{HEADER}{}", toml_to_string(&s)));
                note = format!("{} (created)", path.display());
                s
            }
        };
        let mut it = args.iter().peekable();
        while let Some(a) = it.next() {
            match a.as_str() {
                "--animation-studio" => {
                    s.animation_studio = true;
                    s.asset_studio = false;
                    s.creature_studio = false;
                    if it.peek().is_some_and(|v| !v.starts_with("--")) {
                        s.animation_clip = it.next().cloned().unwrap_or_default();
                    }
                }
                "--asset-studio" | "--studio" => {
                    s.asset_studio = true;
                    s.animation_studio = false;
                    s.creature_studio = false;
                    if it.peek().is_some_and(|v| !v.starts_with("--")) {
                        s.asset_name = it.next().cloned().unwrap_or_default();
                    }
                }
                "--creature-studio" => {
                    s.creature_studio = true;
                    s.asset_studio = false;
                    s.animation_studio = false;
                    if it.peek().is_some_and(|v| !v.starts_with("--")) {
                        s.creature_name = it.next().cloned().unwrap_or_default();
                    }
                }
                "--bridge" => {
                    s.bridge = match it.peek() {
                        Some(v) if !v.starts_with("--") => it.next().cloned().unwrap_or_default(),
                        _ => pav_tools::bridge::DEFAULT_ADDR.into(),
                    }
                }
                "--backend" => s.backend = it.next().cloned().unwrap_or(s.backend),
                "--scene" => s.scene = it.next().cloned().unwrap_or(s.scene),
                "--room" => s.scene = it.next().map(|r| format!("world/{r}")).unwrap_or(s.scene),
                "--seed" => s.seed = it.next().and_then(|v| v.parse().ok()).unwrap_or(s.seed),
                "--pad-script" => s.pad_script = it.next().cloned().unwrap_or_default(),
                "--no-vsync" => s.vsync = false,
                "--vsync" => s.vsync = true,
                "--fullscreen" => s.fullscreen = true,
                "--windowed" => s.fullscreen = false,
                "--dev" => s.dev = true,
                "--kiosk" => s.kiosk = true,
                _ => {}
            }
        }
        if let Ok(b) = std::env::var("PAV_BACKEND") {
            s.backend = b;
        }
        if s.animation_studio || s.asset_studio || s.creature_studio {
            if s.scene.is_empty() {
                s.scene = "empty".into();
            }
            if s.bridge.is_empty() {
                s.bridge = pav_tools::bridge::DEFAULT_ADDR.into();
            }
        }
        if s.scene.is_empty() {
            // The game's title screen (or Shardfall's town); the engine's pavilion is `world`.
            s.scene = default_scene();
        }
        (s, note)
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn toml_from_str(t: &str) -> Result<Settings, String> {
    // Tiny key = value parser (the file is flat), so the app does not need a TOML dependency.
    let mut s = Settings::default();
    for line in t.lines() {
        let line = line.split('#').next().unwrap_or("").trim();
        let Some((k, v)) = line.split_once('=') else { continue };
        let (k, v) = (k.trim(), v.trim().trim_matches('"'));
        match k {
            "backend" => s.backend = v.into(),
            "vsync" => s.vsync = v == "true",
            "width" => s.width = v.parse().map_err(|_| "bad width")?,
            "height" => s.height = v.parse().map_err(|_| "bad height")?,
            "fullscreen" => s.fullscreen = v == "true",
            "start_room" if !v.is_empty() => s.scene = format!("world/{v}"),
            "seed" => s.seed = v.parse().map_err(|_| "bad seed")?,
            "bridge" => s.bridge = v.into(),
            _ => {}
        }
    }
    Ok(s)
}

#[cfg(not(target_arch = "wasm32"))]
fn toml_to_string(s: &Settings) -> String {
    format!(
        "backend = \"{}\"\nvsync = {}\nwidth = {}\nheight = {}\nfullscreen = {}\n# start_room = \"playground\"\nseed = {}\n# Live agent bridge (pav live / pav mcp --live), e.g. \"127.0.0.1:7878\"\nbridge = \"{}\"\n",
        s.backend, s.vsync, s.width, s.height, s.fullscreen, s.seed, s.bridge
    )
}
