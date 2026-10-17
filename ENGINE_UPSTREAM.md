# Engine upstream

This repository vendors the **Pavilion** engine from **shardfall**:

- Source: https://github.com/michaelcrosato/shardfall
- Commit: `0098196` ("Co-op plan on hold (#74)"), workspace version 0.1.0
- What was copied: `git ls-files -- . ':!:pavilion-lite'` (the file set of
  `scripts/package-agent.sh`), imported unchanged as commit `79061e4` ("Vendor shardfall engine @
  0098196"). The engine README moved to `docs/ENGINE_README.md`.

Every local change to the engine is listed below, grouped by crate, so it can be sent upstream or
re-applied on a newer engine. "Engine" means every crate except `crates/blacktap` (the game) and
everything outside `games/blacktap/`. The two new crates (`pav_kit`, `pav_ui`) are
game-agnostic. `git diff 79061e4 -- crates/pav_* scripts web` shows the exact changes.

## New engine crates

| Crate | What it is | Files |
|---|---|---|
| `pav_kit` | Game kit on the hook: `mission` (graphs of steps with conditions, actions, optional objectives, checkpoints, `forced` steps), `dialog` (lines, barks, subtitles), `ballistics` (shots, gravity, water and ground impacts, `elevation_for`), `naval` (sailing with wind and sails, rudder, grounding, collisions, damage, fire, disabling, sinking, repair, brace; AI captains; batteries and mortar; broadside guns that train ±12° toward an aim), `combat` (vitals, attack tokens, tells, counters, assassinations, pistols, tools), `perception` (sight cones with raycasts, hearing, concealment, awareness states, heat), `cinematic`, `camera` (orbit camera with contexts, director cuts), `demo` (showcase personality and watchdog for attract), `shipyard` (lofted hulls, rigging, billowing sails with an emblem). | `crates/pav_kit/src/*.rs` (unit tests inside) |
| `pav_ui` | 10-foot, controller-first UI kit: theme and OFL fonts, glyphs (DualSense, Xbox, keyboard and mouse), data bindings, settings v2 (controls, camera, audio, display, accessibility, gameplay), the front-end state machine (title, menus, chapters, settings, credits, confirm, attract and its card, pause with map/objectives/settings, mobile menu and about, kiosk), the HUD drawn from `UiModel`, the world map, and a CPU rasteriser for headless captures. | `crates/pav_ui/src/**`, `crates/pav_ui/tests/*`, `assets/fonts/PirataOne`, `assets/fonts/AlegreyaSans` (OFL, with licences) |

## `pav_core`

| Change | Files |
|---|---|
| **Hook v2**: `PlayFrame` {camera hint, `UiModel`, `DrawList`, `Atmosphere`, `AudioState`, `FrameEffects` with rumble} produced by `Game::frame`; `Game::front_end`, `tunables`, `progress`/`restore`, `command` (game-specific tool commands via `Sim::game_command`); runtime game registration (`games::register`). | `src/play.rs`, `src/games/mod.rs`, `src/frame.rs`, `src/lib.rs` |
| `SimEvent::Cue { cue, pos, gain, pitch }` (game sounds and effects by name, resolved by data banks). | `src/frame.rs` |
| Zone events run before the game's `post` (they were lost to games); test `zone_events_reach_the_games_post`. | `src/sim.rs`, `tests/games.rs` |
| `Weapon::None` so the fire buttons don't throw bombs or shoot for hook games; test `hook_games_can_turn_the_bombs_off`. | `src/character.rs`, `tests/games.rs` |
| `InputFrame.look` (camera delta) and `triggers` (analog L2/R2), serde-defaulted; button names for all 32 bits. | `src/input.rs` |
| Checkpoints outside `SimState` (`Sim::request_checkpoint`, save/load), persistence `Store` API (native file; localStorage in the app). | `src/sim.rs`, `src/persist.rs` |
| `Character.scripted` (game-placed poses: leaps, cinematics). | `src/character.rs` |
| Ocean: Gerstner waves (6 slots, presets), CPU height/normal/velocity identical to the GPU; seabed; swimming in waves. | `src/ocean.rs`, `src/character.rs`, `src/world.rs` |
| **Islands**: heightfields from outlines with beaches, shelves, peaks, cliffs, noise, plateaus and graded roads (roads ramp out of plateaus), terrain patches, materials, climbable cells, seabed; `contours` (marching squares) for maps; `LevelDef`/`build_level`. | `src/island.rs`, `tests/island.rs` |
| **Structures**: 27 generator kinds (houses, fort walls, bastions, gates, keep, batteries, towers, chapel, pier, laundry lines, palisades, huts, ruins...), markers, areas (hide, soft, perch), block flags (climbable, roof, pole, beam). | `src/structures.rs`, `tests/structures.rs`, `src/yard.toml` |
| **Vegetation**: palms, jungle, rocks, stalk grass scattered by rules around footprints. | `src/vegetation.rs` |
| **Traversal**: free-run, wall and pole climbing with corners, mantle, back-eject, ledge safety, jump assist, beams, leaps; `TraversalState`; `SimEvent::Land`. | `src/traversal.rs`, `src/character.rs`, `tests/traversal.rs` |
| **nav25**: layered walkable grid (streets, roofs, wall walks), A* with string-pulling, flow fields, ladder links, gates, weighted paths. | `src/nav25.rs`, `tests/nav25.rs` |
| `terrain::render_key`: an injective render-mesh key per terrain chunk (the view's FNV key collided and chunks drew each other's meshes). | `src/terrain.rs` |
| Puppet gear: `tricorne`, `bicorne`, `bandana`, `straw_hat`, `beaked_hood` helms; trim, coat skirts, hair, stripes, belt strap, bracer, holsters; `pistol`, `musket`, `cutlass` weapons; `bulk`. Additive (old looks unchanged). | `src/puppet.rs`, `anim/moves.toml` (`aim`, `aim_musket`) |
| `Sim::raycast_block` (hit plus the static block hit). | `src/world.rs` |
| `Mesh.cull` category for draw-list meshes. | `src/play.rs` |
| `DrawItem::Emitter { key, def, pos, drift }` (particle emitters that live while the game draws them; `drift` leans smoke downwind) and `DrawItem::Distortion` (heat haze, spray). | `src/play.rs` |

## `pav_view` and `pav_render`

| Change | Files |
|---|---|
| Play frames reach the view: game camera, draw list (prims, meshes, lines, decals, lights, sails, rigging), highlights and Gull Sight x-ray, atmosphere. | `pav_view/src/build.rs`, `src/play_view.rs`, `src/atmos.rs` |
| Sea and sky passes: GPU Gerstner ocean matching the CPU, foam, shallows from the seabed, sky with sun and clouds, rain, lightning, ray fog; far plane from the atmosphere. | `pav_render/src/sea.rs`, `src/shaders/sea.wgsl`, `src/renderer.rs`, `src/scene.rs` |
| Frustum culling and distance culling classes (small 120 m, vegetation 180 m, structures 600 m; tunable). | `pav_render/src/cull.rs`, `pav_view/src/build.rs` |
| FX bank: cue names → particle bursts, shockwaves, water dents, flashes from data (`fx.toml`). | `pav_view/src/cue_fx.rs`, `src/fx.rs`, `pav_render/src/fx.rs` |
| Terrain chunk meshes keyed by `pav_core::terrain::render_key`. | `pav_view/src/build.rs` |
| Draw-list emitters and distortions tracked by key; the game's wind reaches cue particles; `fx.toml` particle layers take `wind`, `drag`, `gravity`, `floor`. | `pav_view/src/play_view.rs`, `src/build.rs`, `src/cue_fx.rs` |
| **Performance and quality** (PLAN §9.18): hook-game quality presets Low/Medium/High (`view.quality`: render scale, shadow map size `Sun::shadow_map` and reach, ocean grid, plant share, terrain LOD, effects); half-resolution terrain LOD past 160 m with skirts; camera-fitted sun shadows; distance classes on statics and puppet details; plants thinned with distance plus a sparse far forest; screen-size culling (`Scene::cull_pixels`); static batching of plain pieces per region; per-class render stats (`capture stats`, `tour stats=true`). | `pav_render/src/{renderer,scene,cull}.rs`, `pav_view/src/{quality,batch,build,atmos}.rs`, `pav_view/tests/quality.rs` |
| Looks *Painted Seas* (low-radius Kuwahara on the environment, characters crisp), *Memory Mist*, *Storm*; a look's grade stacks on the game's atmosphere; Settings → Display → Visual Style. | `pav_view/src/looks.toml`, `pav_view/src/atmos.rs`, `pav_ui/src/settings.rs`, `pav_app/src/app/play.rs` |

## `pav_audio`

| Change | Files |
|---|---|
| Audio v2: data sound bank (cues → layered synth voices), mixer with pools, buses, reverb and a limiter; an ABC-notation sequencer for shanties with a crew choir; music states; ambience layers; voice blips (babble); offline rendering for tests. | `src/bank.rs`, `src/mixer.rs`, `src/abc.rs`, `src/music.rs`, `src/ambience.rs`, `src/voice.rs`, `src/dsp.rs`, `src/instruments.rs`, `src/render.rs`, `tests/*` |

## `pav_app`

| Change | Files |
|---|---|
| **Haptics**: each tick's `FrameEffects.rumble` cues are collected once by the sim host and played while the game is being played (not menus, pause or attract) when Vibration is on: `gilrs::ff` natively (low → strong motor, high → weak), the Gamepad API's `vibrationActuator.playEffect("dual-rumble")` in the browser (called through `js_sys::Reflect`; web-sys `Navigator` feature). | `src/rumble.rs`, `src/simhost.rs`, `src/app/play.rs`, `Cargo.toml` |
| Hook games get a front end (`pav_ui`), settings pushed to the game, HUD, audio v2, cue FX, touch (taps reach the front end; the phone ☰), Continue/New Game with saved progress, checkpoint saves, localStorage persistence on the web, developer/kiosk gating, Assassin's Keg branding when it is the start scene. | `src/app.rs`, `src/app/play.rs`, `src/settings.rs`, `src/simhost.rs`, `src/store.rs`, `src/main.rs` |

## `pav_tools`

| Change | Files |
|---|---|
| Kit tools: `ui` (any front-end screen or the HUD to PNG at any size and glyph family), `attract` (headless attract runs with stall and exit checks), `tour` (contact sheet, `stats=true`), `ray`, `ocean`, and `gcmd` — the running game's commands (`ship`, `mission`, `perception`, `fighters`, `level_check`, `markers`), which also answer by their own names; `capture ui=true family= stats=`; `player` shows the traversal state. The `tools/list` budget test is 51,500 chars (was 48,000). | `src/kit_tools.rs`, `src/tools.rs`, `src/session.rs`, `tests/attract.rs`, `tests/asset_errors.rs` |
| Store index regenerated for the new core materials and props (`pav assets reindex=store`). | `store/STORE.tsv` |

## Scripts and web

| Change | Files |
|---|---|
| `scripts/check.sh` (the gate) and `--full`; `scripts/setup-game.sh` (wasm32 target, wasm-bindgen-cli, Playwright), called by the session hook. | `scripts/check.sh`, `scripts/setup-game.sh`, `.claude/hooks/session-start.sh` |
| `scripts/build-game.sh`: the single-file game HTML into `dist/`; `scripts/browser_check.mjs`: headless Chromium checks (desktop and phone emulation, attract, ☰, console errors, screenshots). | `scripts/build-game.sh`, `scripts/browser_check.mjs`, `scripts/build-web.sh`, `scripts/pack_single_html.py` |
