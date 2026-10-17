# Build log — Assassin's Keg: Black Tap

Newest first. Each entry: what was built, how to verify it (exact commands), decisions and why,
open problems. This file and the ticks in `PLAN.md` §14 are the memory across context
compactions: after any restart read §14, then the top of this file, then `git log -20`.

---

## 2026-10-17 — Phases 1–3 in progress: hook v2, kit, sea, renderer, UI kit

**Built**
- Hook v2 (`pav_core::play`): `PlayFrame` {camera hint, `UiModel`, `DrawList`, `Atmosphere`,
  `AudioState`, `FrameEffects`} reaches `pav_view` (draw list, camera, atmosphere), `pav_app`
  (front end, HUD, audio) and `pav_tools`. Runtime game registry (`games::register`), ocean
  (`pav_core::ocean`: 6 Gerstner slots, presets, seabed), checkpoints outside `SimState`,
  persistence `Store` (native file + localStorage), `InputFrame.look/triggers`,
  `Character.scripted` (game-placed poses), `Weapon::None`.
- `pav_kit`: camera director, demo, mission graphs, dialog, ballistics, naval (sailing, captains,
  batteries), combat (vitals, tokens, counters, assassinations, pistols, tools), perception +
  heat, cinematics, shipyard (lofted hulls, rigging, billowing sails with the emblem).
- `blacktap`: Tab I markers/routes, R1–R6 mission graphs, sea module (helm, AI ships, forts,
  mortar, flotsam, kegs), boarding (swing, deck arena, surrender, plunder), bot.
- Renderer (agent C, merged `cc2c918`): sea + sky passes (GPU Gerstner identical to the sim's),
  clouds, rain, lightning, ray fog, distance culling classes, Gull Sight x-ray silhouettes.
  `pav_view::atmos::apply` is now called after the basic atmosphere; hook games set
  `cull_small/vegetation/structure = 120/180/600 m` (`ViewSettings`, tunable).
- `pav_ui` (agent B, merged `b0ca4c9`): theme + OFL fonts, data bindings, DualSense/Xbox/keyboard
  glyphs, settings v2, front-end state machine (16 modes incl. attract, kiosk, mobile menu),
  HUD from `UiModel`, CPU rasteriser. 45 tests pass (`cargo test -p pav_ui`).

**Verified**
- `./target/debug/pav play scene=blacktap/round/1 bot=true seeds=1 ticks=36000` → R1 complete
  (24.5 s, optional done). Round 2 → complete in 147 s (gunboats sunk, Restraint disabled,
  boarded, plunder) and flows into R3. R3 after the battery rebalance: mortar down by ~280 s.
- Render agent contact sheet of the six atmosphere presets
  (`cargo run -p pav_view --example sea_sky`): looked at it — sun glints, clouds, storm rain.

**Open**
- R3 batteries still slow for the bot; R4–R6 need traversal (agent running), stealth, nav.
- `pav_app` play path waits on the audio v2 API (agent A).

---

## 2026-10-17 — Phase 0: bootstrap

**Executor (confirmed with `get_session`, claude-code-remote MCP):** `session_context.model` =
`claude-opus-5-5`, `external_metadata.last_served_model` = `claude-opus-5-5`, `effort_level` =
`xhigh`. Recorded in the README's Source Information.

**Built**
- Vendored shardfall @ `0098196` at the repository root (commit `79061e4`, 1,359 files, 87 MB,
  `pavilion-lite/` left out); engine README → `docs/ENGINE_README.md`.
- `scripts/check.sh` (the gate, §15.1), `scripts/setup-game.sh` (wasm32 target, wasm-bindgen
  0.2.129, Playwright), the session hook calls it; `ENGINE_UPSTREAM.md`; this log.

**Installs** (all scripted in `scripts/setup-game.sh`, called from `.claude/hooks/session-start.sh`):
`rustup target add wasm32-unknown-unknown` (already present); wasm-bindgen-cli 0.2.129 (already
at `/usr/local/bin` in this image; the script installs it with cargo when missing); Playwright
1.56.1 (global npm, already present; Chromium 1194 at `/opt/pw-browsers`).

**Verified**
- `cargo build --locked -p pav_tools --bin pav`: 2 m 07 s (build cache copied from the planning
  checkout).
- `./target/debug/pav play scene=starter bot=true seeds=3` → `wins 3/3`.
- `cargo test --locked -p pav_core --test games` → 4 passed.
- `./target/debug/pav capture scene=starter ticks=60 width=640 height=360 out=out/p0_starter.png`
  → 927 ms on lavapipe; looked at it: the starter arena renders correctly.

**Decisions**
- Copied the planning checkout's `target/` (same commit, same toolchain) to skip a cold build.
