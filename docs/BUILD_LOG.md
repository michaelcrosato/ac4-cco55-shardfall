# Build log — Assassin's Keg: Black Tap

Newest first. Each entry: what was built, how to verify it (exact commands), decisions and why,
open problems. This file and the ticks in `PLAN.md` §14 are the memory across context
compactions: after any restart read §14, then the top of this file, then `git log -20`.

---

## 2026-10-17 — Performance budgets, quality presets, LOD, looks; content guide; tools

**Built**
- Render/perf (agent, merged `eb2298d`): hook-game quality presets **Low / Medium / High**
  (Settings → Display → Quality; `pav_view/src/quality.rs`): Low = 0.5 MP render scale, no GI /
  sun shafts / haze, half ocean grid, 50 % vegetation, 1024 shadow map; Medium = 1 MP, 2048,
  75 % plants (the browser default); High = full, 4096. **Two terrain LODs** (half-resolution
  meshes with skirts past 160 m: the island vista 536k → 178k triangles, no seams in
  `out/tour/seam_lod60.png`), **camera-fitted shadows**, far plane 1,800 m from the atmosphere,
  distance classes on every static and puppet detail, plants thinned with distance plus a far
  forest, screen-size culling, static batching; looks **Painted Seas**, **Memory Mist**,
  **Storm** (Settings → Display → Visual Style; `out/tour/looks.png`); `history_seconds` = 10.
- `docs/CONTENT_GUIDE.md` (agent, `4704845`): every content kind with a worked example that was
  applied, checked with the tools and reverted. Its findings were fixed: Swift Rigging (+12 %
  sail speed), guard barks and the town crier, bark cooldowns, the results' collectible total,
  the map legend, the `r6_drye` hint, a duplicate marker, `level_check` on a broken geometry
  file, `ray` before the first step.
- `pav`: the game's commands fold into one advertised tool, `gcmd`, and still answer by name
  (`pav ship`, `pav level_check`, `pav fighters`...); the store index regenerated.

**Verified** (`tour stats=true`, High, 1920×1080; `out/tour/tour_stats_1080.jsonl`)

| Viewpoint | Instances after culling | Triangles | Ocean vertices | Particles |
|---|---|---|---|---|
| Helm at sea (R2) | 1,402 | 468k | 80,513 | 530 |
| Town street (R5 crowd) | **2,296** | 403k | 80,513 | 530 |
| Rooftop over town | **2,461** | 329k | 80,513 | 530 |
| Fort courtyard | 733 | 114k | 80,513 | 530 |
| Jungle | 2,914 (1,710 plants) | 796k | 80,513 | 530 |
| Storm finale | 1,485 | 512k | 80,513 | 672 |

- Budgets (§9.18): ≤ 2,500 instances at High in town — street 2,296, rooftop 2,461 (Medium
  2,104 / 2,292, Low 1,654 / 1,745); ocean ≤ 120k vertices — 80,513 (Medium 45,793, Low
  20,801); particles ≤ 20k — at most 2,676 slots. Sim ≥ 240 ticks/s in the dev build:
  `pav bench` blacktap 23,235, round/5 938, freeroam 1,249, round/6 (slowest, ~120 NPCs, the
  fort) **389 ticks/s**. 60 fps on an RTX 3060 Ti can't be measured here (no GPU: lavapipe and
  SwiftShader); the presets and these counts are the proxy.
- Art checklist (`out/tour/tour.png`: the six viewpoints, ships, characters, the island from
  the sea) — ships pass; buildings pass (the barracks' back wall is plain); terrain passes after
  the fort plateau became packed earth; sky passes (sun, clouds, storm with rain and lightning);
  ocean passes (swell, glints, shallows, surf, storm foam).
- `cargo test -p pav_view --test quality`, `-p pav_tools` (all suites, after the store reindex
  and the tools/list budget), clippy `-D warnings` on all nine crates.

**Decisions**
- `tools/list` budget 48,000 → 51,500 chars: the kit tools (`ui`, `attract`, `tour`, `ray`,
  `ocean`, `gcmd`) after folding eleven tools into six; recorded in the test's comment.

---

## 2026-10-17 — The whole Tab by the bot on 3 seeds; map, nav25, Drydock, gear

**Built**
- **Map chart** (`crates/blacktap/src/map.rs`): coastlines and two hill contours traced from the
  heightfields (`pav_core::island::contours`, marching squares + Douglas–Peucker), roads,
  building footprints and place names (`[[map_label]]`); live icons for the player, ships,
  flotsam, objectives, the fort, the tavern, lookouts (surveyed or not) and the collectibles a
  survey revealed. The map opens zoomed ×2 on the player.
- **nav25** (agent, merged `6918de9`): a layered walkable grid (streets, roofs, wall walks) with
  A*, flow fields, ladder links and gates. Guards path with it when investigating, searching,
  running to the bell and returning to post; fighters, the bot's melee and the bot's walking
  go round walls with cached flow fields (`crates/blacktap/src/paths.rs`, `steer`).
- **Drydock** (agent, merged `6224296`): `blacktap/drydock`, every station of §9.19 with signs,
  guides and pads; 8 tests (`crates/blacktap/tests/drydock.rs`).
- **Gear** (agent, merged `fcc07dd`): tricorne, bicorne, bandana, straw hat, beaked hood; coats,
  hair, stripes, the Tap bracer, holsters; pistol, musket, cutlass; look variants per archetype
  worn by crowds, guards, boarders and our crew.
- Engine fixes: roads ramp out of plateaus (no 3 m step at Dryport's edge); terrain chunk mesh
  keys are injective (`pav_core::terrain::render_key`: chunks drew each other's meshes);
  broadside guns train ±12° toward a lead point; `safe_heading` heads for deep water when
  already in the shallows; the `ray` tool reports its range on a miss; `pav fighters`.
- Level fixes found by the bot and `level_check` (now reports structures standing on roads):
  the camp fence and watchtower off the fort road, the road bypasses the smugglers' hut;
  Round 6's route leaves Dryport round the waterfront row; a checkpoint when the Restraint duel
  starts; the bot lets go of walls its route doesn't climb, runs off ledges it is stuck on,
  steps onto the battlement perch before raising the flag.
- HUD: marker health bars stack under the icon; compass letters give way to marks.

**Verified**
- `cargo test -p blacktap --test full_level -- --ignored` (bot, Round 1 → TAB COMPLETE):
  seed 1 **788 s**, seed 2 **772 s**, seed 3 **788 s** — 3 passed.
- `cargo test -p blacktap --test rounds -- --ignored` (each Round from its own start, 5 seeds):
  R1 24–26 s, R2 93–148 s, R3 149–230 s, R4 50 s, R5 201 s, R6 144 s — 6 passed (30 runs).
- `cargo test -p pav_core --test games`: 6 passed, incl. `zone_events_reach_the_games_post` and
  `hook_games_can_turn_the_bombs_off`. `cargo test -p pav_core --test nav25`: 9 passed.
  `cargo test -p blacktap` quick suites (data, determinism ×3, drydock ×8, persist, rounds R1).
- `cargo test -p pav_tools --test attract -- --ignored` (= `pav attract minutes=30`, headless
  front end + game + bot, fake clock): **30 segments, 5 seed sets (11–15 … 51–55) and the start
  of a sixth loop, 0 stalls, 0 load errors, every segment ≥ 3 mechanics** — e.g. Rule the Waves
  seed 15: board, boarding_won, brace, counter, disabled, finisher, telegraph; Last Call:
  gull_sight, leap_of_spirits, take_helm, waterspout. Log: `out/attract30.log`.
- `cargo test -p blacktap --test data`: 5 passed, incl. `the_tab_holds_every_collectible`
  (4 sheets, 5 bottles, 8 chests, 3 Keg Taps, 6 flotsam) and `three_difficulties`
  (Landlubber 0.6 < Sailor 1.0 < Captain 1.3).
- `pav level_check` on `blacktap/round/6`: ok, no problems. Captures looked at:
  `out/ui/pause_map_live-1080.png` (the chart), `out/ui/hud_fix-1080.png` (markers).

**Decisions**
- The bot uses flow fields (a pure function of the level and the goal, cached) rather than
  per-bot paths, so it stays a stateless function of the game and replays stay exact.
- Ledge safety stays for walking; the bot runs only when stuck.

---

## 2026-10-17 — Phases 2–5: Tab I world, all six Rounds played by the bot, attract

**Built**
- Tab I geometry (`games/blacktap/tab1_world.toml`, generated by `pav_core::island` /
  `structures` / `vegetation` from the world-gen agent, merged `1079106`): Isla Templanza with
  Dryport (≈22 buildings, chapel, plaza, stocks, hay carts, hall, tavern, warehouse, quay, pier,
  moles, laundry lines), Fort Abstinence on the cliff-top headland (walls, four bastions, gate,
  keep against the north wall, barracks, west and north batteries, mortar tower), jungle
  (ruins, watchtower, camps, stalk grass), Gull Rock lighthouse, Hook Key. Cached per process,
  placed per scene (`crates/blacktap/src/island.rs`). Rules' markers alias structure markers
  (`[marker_from]` in `tab1.toml`).
- Traversal (agent, merged `a9b6b6f`): free-run, vaults, wall/pole climbing with corners, mantle,
  back-eject, ledge safety, jump assist, beams, dives; `TraversalState`; 16 tests.
- Land rules (`crates/blacktap/src/stealth.rs`): brains (posts, patrols, escorts, gossip,
  wanderers, the courier, Drye), sight with raycasts + hearing + concealment (grass, hay, crowds),
  states and alerts, bodies, heat/posters, bells and reinforcements, tools (darts, Smoke Keg,
  Bung Line), pistols and quick-shot, pickpocket, the stocks, the smugglers' tunnel, Last Words,
  the Tap flag, perches/survey/Leap of Spirits (`traverse.rs`), free-roam contract.
- `pav` tools: `ui`, `attract`, `ship`, `mission`, `perception`, `ocean`, `level_check`, `tour`,
  `markers`, `ray`; `capture ui=true`; `player` shows the traversal state. Game commands go
  through the new `Game::command` hook.
- FX bank (`pav_view::cue_fx`, `games/blacktap/fx.toml`): 45 cues → particles, flashes,
  shockwaves, water dents; audio v2 (agent, merged `97f0f25`): bank, sequencer, 6 shanties +
  title theme + state music, ambience.

**Verified**
- `cargo test -p blacktap --test rounds -- --ignored round_N_five_seeds` (bot, efficient):
  R1 24–26 s, R2 132–345 s, R3 250–605 s, R4 50 s, R5 91 s, all 5 seeds each.
- `cargo test -p blacktap --test determinism`: snapshot + replay, rewind and from-start replay
  exact in a sea battle (R2), a boarding (R2@board) and a stealth section (R5): 3 passed.
- `pav attract minutes=6 step=0.5`: 7 segments, 0 stalls, every segment ≥ 3 mechanics
  (e.g. Shadow of the Keg: finisher, free_molasses, gull_sight, nightcap_dart).
- Captures looked at: `out/tab1_over.png`, `out/tab1_town.png`, `out/tab1_fort.png`,
  `out/tab1_plaza.png`, `out/chapel_e.png`, `out/r2_hud3.png` (HUD over a broadside).
- Browser (`node scripts/browser_check.mjs dist/…html`, SwiftShader 0.09 fps): boots to the
  title, 11 s idle → attract, a key → title, phone boots into attract, a tap stays in attract,
  no console errors; the ☰ tap failed (touch never reached the front end) — fixed in the app,
  to re-verify with the next dist build.

**Decisions**
- The chapel's Crow's Perch counts from the belfry floor under its plank (the plank sits on the
  belfry roof, which a climber can't reach); the keep roof doubles as the battlement Leap of
  Spirits into the sea, so the finale flows flag → leap → swim → helm.
- Drye paces the courtyard with two escorts instead of standing on an unreachable balcony.

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
