# Build log — Assassin's Keg: Black Tap

Newest first. Each entry: what was built, how to verify it (exact commands), decisions and why,
open problems. This file and the ticks in `PLAN.md` §14 are the memory across context
compactions: after any restart read §14, then the top of this file, then `git log -20`.

---

## 2026-10-17 — Final verification: the gate, the browser build, the Definition of Done

**Built** (since the last entry)
- **Bodies stop blocking** (found by the full-level gate: seed 2 stood still in the fort for an
  hour of game time). Two escorts' corpses wedged the bot against Drye's guard: a dead character
  stayed a standing capsule, and steering ignores entity hits. Engine: `Sim::set_ghost` turns a
  character's capsule into a sensor (`Character.ghost`, kept through sleep and snapshots); every
  kill ghosts its body (`rounds::on_kill`). Test `ghosts_let_the_living_walk_through`
  (`pav_core/tests/movement.rs`). The bot trace can save states (`BT_SAVE_AT=600,700`) for
  `pav snapshot_load`, which is how the stall was found.
- Phone ☰: the front end took taps only as "any input", which phone taps deliberately aren't, so
  the button never opened the overlay; it now reads the tap first, and the app publishes the
  button's rectangle after drawing, in CSS pixels. Test
  `a_phone_tap_on_the_button_opens_the_menu_as_the_app_sends_it` (`pav_ui/tests/frontend.rs`).
- Buoyancy: `ships_ride_the_waves` (`pav_kit` naval) — hulls heave, pitch and roll on a sea and
  sit level on a flat one. Free roam's first convoy sails 30 s in.
- The engine's own demos follow the new core assets: store index and Library Hall regenerated
  (26 materials), the Store Showcase shows the eight new materials; `pav_tools` store tests.
- Docs: `README.md` (Source Information, how to play, controls, build and test, features,
  credits), `docs/ENGINE_ANALYSIS.md`, `docs/CONTENT_GUIDE.md` (every content kind with a worked
  example), `ENGINE_UPSTREAM.md` (every engine change, by crate).

**Verified**
- **`scripts/check.sh --full` on `a0c43f3`: "check.sh: all green (full)", exit 0, 21.7 min.**
  fmt; clippy `-D warnings` on the nine crates (native) and `pav_app` (wasm32); WGSL through
  Tint (every module 0 errors); tests: **612 passed, 0 failed** (14 ignored: the long runs and
  opt-in suites); long runs: `rounds` 6 passed (each Round × 5 seeds, 178 s), `full_level` 3
  passed (111 s), `attract_thirty_minutes_no_stalls` passed (≥ 5 seed sets, 0 stalls, 0 load
  errors, every segment ≥ 3 mechanics; 110 s). The run before it, on `c54c4f8`, failed on
  `full_level_seed_2` — the stall fixed above.
- `scripts/check.sh` (the quick gate) is the first half of the same run, green: fmt, clippy
  `-D warnings` on the nine crates (native) and `pav_app` (wasm32), WGSL through Tint (every
  module 0 errors), the tests of `pav_core pav_kit pav_ui blacktap pav_audio pav_tools pav_view`.
- Full level after the fix (`cargo test -p blacktap --test full_level -- --ignored`): seed 1
  **874 s**, seed 2 **782 s**, seed 3 **746 s** (Round 6 at 692 / 596 / 560 s).
- **dist** (`scripts/build-game.sh` on `a0c43f3`, committed in `c7976c3`):
  `dist/Assassins-Creed-IV-Black-Flag-AssassinsKegBlackTap-W9-2026-10-17-Claude-Opus-5.5.html`,
  13.5 MB (13,452,628 bytes; the wasm is 33.8 MB, 10.0 MB gzipped), one file:
  `<title>Assassin's Keg — Black Tap</title>`, the description and theme-color metas, an
  inline SVG keg-tap favicon, the wasm and glue inlined (no other loads).
- **Browser** (`node scripts/browser_check.mjs dist/…html out/browser`, Chromium 1194, WebGPU on
  SwiftShader, `file://`): **10/10 PASS** — desktop boots to the title; 11 s idle → attract
  (DEMO PLAY); a key in attract → the title; no console errors; settings saved to localStorage
  (`blacktap.settings`); after a reload the app finds settings and progress; phone boots into
  attract; a tap on the game stays in attract; tap ☰ → the minimal overlay; no console errors.
  SwiftShader drew 0.05 fps here (the gate was running beside it), so the script waits on
  wall-clock frames and now allows 3 minutes for a page load; the exit on the same frame is the
  front-end test's job (`any_input_leaves_attract_on_the_same_update_and_is_consumed`).
  Screenshots: `out/browser/desktop_title.png` (the title, the build ID, the browser's sound
  hint), `desktop_attract.png` (DEMO PLAY over the Second Round broadside, "PRESS ANY BUTTON"),
  `desktop_back_to_title.png`, `phone_attract.png` (only ☰ over the demo), `phone_menu.png`
  (Sound, Quality, About, Close). Looked at all five: no engine overlays, no placeholder text.

### §14 evidence map

| Box | Evidence |
|---|---|
| A1 Title, ID, naming | README header; HTML `<title>` and file name (above); About screen (`games/blacktap/credits.toml`, `crates/blacktap/tests/data.rs::front_end_info_is_complete` checks name, tagline and `AssassinsKegBlackTap-W9`); PR title "Assassin's Keg: Black Tap [AssassinsKegBlackTap-W9]" |
| A2 README | `README.md`: Source Information (model and effort from `get_session`, Phase 0 entry), how to play, controls, building and testing, features, credits, documents |
| A3 Engine analysis | `docs/ENGINE_ANALYSIS.md`: used, not used, built (ranked, files in `ENGINE_UPSTREAM.md`), source read, wrong docs and bugs, the one change (the `PlayFrame` data contract), `FEEDBACK.md` row |
| A4 Content guide | `docs/CONTENT_GUIDE.md`: every §16.3 kind, each example applied, checked with the tools and reverted (Performance entry) |
| A5 dist HTML | this entry: 1 file, 13.5 MB (≤ 25 MB), `file://` in Chromium, desktop and phone screenshots |
| A6 Final message | the session's closing message (§16.4: built, how to play, Source Information, engine analysis, limits, PR) |
| B1 Vendored, upstream list | Phase 0 entry (`79061e4` = shardfall `0098196`); `ENGINE_UPSTREAM.md` |
| B2 Registration, crates | `pav_core::games::register` (blacktap registers at runtime: `data.rs::every_scene_builds`, `pav play scene=blacktap/...`); `crates/blacktap`, `crates/pav_kit`, `crates/pav_ui` |
| B3 Hook v2; starter | Phases 1–3 entry (PlayFrame reaches view, app, tools); `pav_core/tests/games.rs::the_starter_bot_wins_on_ten_seeds`, `standing_still_gets_caught`, `snapshots_rewind_and_replays_are_exact` |
| B4 Input, bindings, glyphs | `InputFrame.look/triggers` (Phases 1–3); `pav_ui/tests/data.rs::blacktap_bindings_match_the_plan`, `glyphs_follow_the_device`; `out/ui/glyphs.png`, HUD sheets for DualSense/Xbox/keyboard |
| B5 Cues and banks | `pav_audio/tests/bank.rs::sound_bank_parses_with_every_cue`, `render.rs::every_cue_is_audible`; `blacktap/tests/vfx.rs::visual_cues_have_a_look` |
| B6 Zones, bombs | `games.rs::zone_events_reach_the_games_post`, `hook_games_can_turn_the_bombs_off` (Tab entry) |
| B7 Persistence | `blacktap/tests/persist.rs`, `pav_ui/tests/data.rs::settings_round_trip`; browser: "settings saved to localStorage" (`blacktap.settings`), "after a reload the app finds settings and progress" |
| B8 Checkpoints | `games.rs::checkpoints_live_outside_the_state` |
| B9 Dev/kiosk, branding | `pav_app` `--dev`/`?dev=1` and `--kiosk`/`?kiosk=1`; `frontend.rs::kiosk_starts_in_attract_and_returns_after_a_minute`; window title, log and storage keys say Assassin's Keg (`blacktap.*` in the browser check) |
| B10 Culling, LOD, shadows, presets | Performance entry (`tour stats=true` table, `out/tour/seam_lod60.png`); `pav_view/tests/quality.rs`, `atmos.rs::the_far_plane_reaches_the_horizon` |
| C1 Ocean, buoyancy, swimming | `atmos.rs::the_gpu_ocean_moves_like_the_simulation`; `naval.rs::ships_ride_the_waves`; `traversal.rs::swimmer_rides_the_waves`, `swimmer_dives_and_resurfaces` |
| C2 Atmosphere | `atmos.rs::the_atmosphere_reaches_the_scene`, `a_setting_sun_lights_from_above_the_horizon_and_lightning_flashes`, `sea_and_sky_render`; `vfx.rs::lightning_strikes_on_a_schedule_in_heavy_rain_only`; storm finale capture (`out/vfx/storm_escape.png`) |
| C3 Islands | `pav_core/tests/island.rs` (5); the tour sheet (`out/tour/tour.png`) |
| C4 Structures, materials | `structures.rs::every_kind_builds_with_flags_and_walkable_roofs`, `the_yard_has_every_kind_and_lines_rest_on_roofs`; 26 core materials (store test) |
| C5 Tab I layout | Phases 2–5 entry; `data.rs::level_check_passes` (every mission's markers, dialog lines and routes resolve) and `pav level_check` on `blacktap/round/6` clean; the full level by the bot |
| C6 2.5D nav | `pav_core/tests/nav25.rs` (9); guards and the bot path with it (Tab entry) |
| D1–D7 Sea | Rounds 2–3 by the bot × 5 seeds (gunboats, Restraint, boarding, fort, mortar); `naval.rs` (`beam_reach_is_fastest_and_irons_slowest`, `ships_ground_on_shallows`, `weapon_by_bearing`), `ballistics.rs` (2); `determinism.rs::sea_battle_is_exact`, `boarding_is_exact`; Drydock `sea_pads`, `boarding_the_hulk_with_dummies_about`; attract segments log brace, telegraph, disabled, boarding_won |
| E1–E2 Traversal, perches | `traversal.rs` (16 + swimming); Drydock `traversal_and_combat_pads`; Rounds 4 and 6 (survey, Leap of Spirits, dive) |
| E3 Combat, tools | `combat.rs` (tokens, counters, the assassination matrix, falls, pistol); Drydock pads; attract logs counter, finisher, nightcap_dart |
| E4 Perception, heat | `perception.rs` (3); `determinism.rs::stealth_section_is_exact`; Round 5 × 5 seeds |
| E5 Brains, crowds | Phases 2–5 entry; Round 5–6 crowds; `pav perception` |
| E6 Gull Sight | x-ray silhouettes (Phases 1–3); `gull_palette` follows Settings → Accessibility → Colour-blind; `vfx.rs::gull_sight_pulses_a_ring` |
| F1 Six Rounds | `rounds.rs` × 5 seeds each; `full_level.rs` × 3 |
| F2 Collectibles, free roam | `data.rs::the_tab_holds_every_collectible`; survey reveals on the map (Tab entry); free roam convoys, contract, tavern; Replay a Round and Continue/New Game (`frontend.rs::new_game_continue_and_quit`, `chapters_load_their_scene`) |
| F3 Difficulties | `data.rs::three_difficulties` |
| F4 Drydock | `blacktap/tests/drydock.rs` (8) |
| G1 Characters, art | Tab entry (gear); Performance entry (art checklist on `out/tour/tour.png`) |
| G2 VFX | VFX entry, `vfx.rs` (8) |
| G3 Audio, haptics | `pav_audio` suites (abc 15, bank 5, dsp 7, render 12: every song, state, cue and ambience layer renders); 6 shanties + title theme; `vfx.rs::rumble_lives_for_one_tick` |
| G4 10-foot UI | `pav_ui/tests/raster.rs::every_screen_rasterises`, `spot_checks_at_other_sizes`; `frontend.rs::focus_graphs_reach_every_item`, `back_always_returns`; `out/ui/*-720.png`, `*-1080.png`, `*-2160.png` |
| H1 Attract | `frontend.rs::idle_on_the_title_starts_attract_mode`, `any_input_leaves_attract_on_the_same_update_and_is_consumed`, `after_the_last_segment_a_card_then_the_next_seed`, `an_hour_of_attract_mode_does_not_grow`; browser desktop checks |
| H2 30-min attract | `pav_tools/tests/attract.rs::attract_thirty_minutes_no_stalls` (= `pav attract minutes=30`; in `--full`), passed on `a0c43f3`; the detailed run in the Tab entry: 30 segments, 5 seed sets, 0 stalls (`out/attract30.log`) |
| H3 Mobile | `frontend.rs::mobile_stays_in_attract_and_touch_does_not_exit`, `mobile_menu_opens_from_the_button`; browser phone checks and screenshots |
| H4 Kiosk | `frontend.rs::kiosk_starts_in_attract_and_returns_after_a_minute`; `--kiosk` / `?kiosk=1` |
| I1 `check.sh` | this entry: the quick half of the `--full` run on `a0c43f3`, green |
| I2 `check.sh --full` | this entry: all green (full) on `a0c43f3` |
| I3 Determinism | `determinism.rs` (3), Drydock `a_run_is_exact` |
| I4 Performance | Performance entry (budgets table) |
| I5 Release build | browser: no console errors on desktop or phone; screenshots show no engine overlays (developer tools only with `--dev`); no placeholder text in `games/blacktap` or the UI strings |

---

## 2026-10-17 — The VFX list, ambient life, haptics

**Built** (agent, merged `6dd7900`; `crates/blacktap/src/vfx.rs`, `games/blacktap/fx.toml`, 50 cues)
- Naval: muzzle flash, flame, sparks and wind-drifted smoke from every gun port (`cannon_fire`,
  `chain_fire`, `swivel_fire`, `muzzle_smoke`), batteries' volleys; cannonball streaks and mortar
  glows; big/small splashes; hull splinters; deck fire (flames, downwind smoke, heat haze,
  crackle); sinking bubbles then floating debris (`Sea::wrecks`); powder-keg explosion with
  debris; mortar launch on the emplacement and impacts; telegraph rings and glowing gun ports.
- On foot: steel sparks (`sword_clash`, `sword_hit`, `counter`); pistol flash and puff; dart
  trails; a billowing Smoke Keg cloud; sand and dust footfalls chosen by the ground's material.
- Water and weather: bow spray and wake; rain streaks, sea rings and splashes round the player;
  lightning on a seeded schedule (sky flash, forked bolt, light, crack, thunder, rumble);
  waterspouts (a rotating column, spray and mist, haze).
- Ambient life (pure functions of time, seed, camera and level — never sim state): gull flocks
  (follow-the-leader boids round the coasts, Gull Rock, the harbour and the Black Tap), jungle
  leaf fall, dusk fireflies over grass, torches with flickering lights on the fort's walls,
  bastions, gate, harbour and doors, the lighthouse lamp; a `dusk` preset.
- Moments: the survey swirl with gulls sweeping the perch, the Leap of Spirits' gold trail and
  feathers, hay and water landings, collectible sparkles, the Gull Sight pulse ring.
- Engine: `DrawItem::Emitter` and `DrawItem::Distortion`; the game's wind reaches cue particles.
- **Haptics**: rumble cues live one tick in `FrameEffects.rumble`; the app plays them while the
  game is played and Vibration is on — `gilrs::ff` natively, the Gamepad API's
  `vibrationActuator.playEffect("dual-rumble")` in the browser. Broadsides, hits, rams, shells
  near the ship, hard landings, assassinations and thunder rumble.

**Verified**
- `cargo test -p blacktap --test vfx`: 8 passed — `drawing_never_changes_the_simulation`,
  `a_burning_ship_draws_flames_smoke_and_haze`, `gulls_wheel_over_the_coast_and_the_ship`,
  `dusk_lights_the_torches_and_wakes_the_fireflies`,
  `lightning_strikes_on_a_schedule_in_heavy_rain_only`, `gull_sight_pulses_a_ring`,
  `visual_cues_have_a_look` (every visual cue has an FX entry), `rumble_lives_for_one_tick`.
- Captures looked at: `out/vfx/r2_battle_fire.png`, `out/vfx/storm_escape.png` (gull flock,
  waterspouts, spray), `out/vfx/title_gulls.png` (wake and bow spray); the agent's 33 captures
  were tuned until each effect read (broadsides, splinters, deck fire, sinking and wreckage,
  mortar launch, lightning, waterspout, gulls, survey, leap, hay landing, darts, Smoke Keg,
  pistol, sword sparks, footfalls, sparkles, Gull Sight ring, fort and town at dusk).
- clippy `-D warnings` native and wasm32 for the touched crates.

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
