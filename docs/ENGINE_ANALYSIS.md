# Engine analysis — building *Assassin's Keg: Black Tap* on Pavilion

For an engine agent who has never seen this game. Pavilion (the engine of
[shardfall](https://github.com/michaelcrosato/shardfall)) was vendored at `0098196` and extended
in this repository; every engine change is listed in [`ENGINE_UPSTREAM.md`](../ENGINE_UPSTREAM.md).
The game is an *Assassin's Creed IV: Black Flag*-style level: a 1.2 km sea with three islands,
sailing and naval combat, boarding, free-running and climbing, stealth and assassinations, a
town, a fort, six Rounds, a front end with an attract mode, and a single-file browser build.

## What I used (and how much it carried)

| Engine part | Carried | Notes |
|---|---|---|
| `Sim` (fixed 60 Hz step, `SimState` clone = snapshot, rewind, replays, seeded `Rng`) | **A lot** | Determinism came for free: a sea battle, a boarding and a stealth section replay bit-exact (`crates/blacktap/tests/determinism.rs`). The game lives in `sim.state.play`, so it snapshots with everything else. |
| Games hook (`pav_core::games`: `Game` trait, `GameBox`, scenes by key, bot input) | **A lot** | The template for the whole game crate. Extended to v2 (see below). |
| Character controller (rapier capsule, walk/run/jump, ladders, swimming, ledge grab) | **A lot** | The base of all on-foot play; climbing, free-run, beams and leaps were added on top in `traversal.rs`. |
| Statics (regions, chunks, blocks, decor, props, terrain patches, versioned) | **A lot** | Every generated building, wall, palm and rock is a static; physics colliders and render batches follow from it. |
| Puppets, gear, moves, clips | **Much** | Every person is a biped puppet with generated gear; hats, coats, bracer and firearms were added as gear parts. |
| Renderer (`pav_render`: lit style, shadows, bloom, filters/looks, particles, outlines) and `pav_view` | **Much** | Extended with sea and sky passes, atmosphere, culling classes, LOD, x-ray highlights. |
| `pav_audio` (synth, voices) | **Much** | Grown into a data-driven bank, sequencer (ABC shanties), ambience and voice blips. |
| `pav_tools` (`pav` CLI/REPL, captures on lavapipe, `play bot=true`, snapshots, replays) | **A lot** | Every check in this repository goes through it; kit tools were added (`ui`, `attract`, `tour`, `ray`, `ocean`, and the game's own commands through `gcmd`: `ship`, `mission`, `perception`, `fighters`, `level_check`, `markers`). |
| Single-file web build (`scripts/pack_single_html.py`) | **All of it** | The `dist/` HTML is this packer's output. |
| Zones, params/tunables, looks (`looks.toml`), persistence hooks | Some | Gull Sight and Painted Seas are looks; tunables reach `pav set`. |

## What I didn't use, and why

- **Shardfall's ARPG** (`pav_core/src/arpg`: loot, skills, passive tree, monsters, levels): a
  different genre; the game crate has its own rules.
- **Rooms** (`rooms/*.toml`) and the pavilion world: rooms are small hand-placed boxes. A 1.2 km
  sea with islands needed heightfield terrain and generators, so levels are `LevelDef`s
  (islands + structures + vegetation) instead.
- **`nav.rs`** (one floor at y = 0): replaced for this game by `nav25` (layers: streets, roofs,
  wall walks).
- **Creature compiler, asset store, the Animation/Object/Creature Studios, KayKit figures**:
  every character here is a generated puppet with gear; props are procedural.
- **Pavilion Lite** (frozen), **co-op** (on hold), **the live bridge / MCP** (the CLI and REPL
  were enough headless).
- **Courses** (start/finish/gates) beyond the zone events.

## What I built that the engine should have had

Ranked by how much output each saved (the files are in [`ENGINE_UPSTREAM.md`](../ENGINE_UPSTREAM.md)):

1. **Hook v2 — the play frame** (`pav_core::play`): a game hands the view, app and tools a
   `PlayFrame` {camera hint, `UiModel`, `DrawList`, `Atmosphere`, `AudioState`,
   `FrameEffects`} each frame. Everything the player sees and hears became data a test can read.
2. **`pav_ui`**: a 10-foot, controller-first UI kit (theme, OFL fonts, glyphs for DualSense,
   Xbox and keyboard, data bindings, settings v2, a front-end state machine with attract and
   kiosk modes, the HUD drawn from `UiModel`) with a CPU rasteriser, so `pav ui` renders every
   screen headless.
3. **`pav_kit`**: missions (graphs with conditions, actions, checkpoints, optional objectives),
   dialog, ballistics, naval (sailing, captains, batteries), melee combat (tokens, tells,
   counters, assassinations), perception and heat, cinematics, the camera director, the demo
   showcase, the shipyard (lofted hulls, rigging, billowing sails).
4. **World generation** (`pav_core::island`, `structures`, `vegetation`): heightfield islands
   with beaches, cliffs, plateaus and graded roads; 27 structure kinds; scattered palms, jungle
   and stalk grass — from one TOML file.
5. **Ocean** (`pav_core::ocean` + the GPU sea pass): Gerstner waves identical on CPU and GPU,
   buoyancy, swimming in waves.
6. **Traversal** (`pav_core::traversal`): free-run, wall and pole climbing with corners, mantle,
   back-eject, ledge safety, jump assist, beams.
7. **`nav25`**: a layered walkable grid with A*, flow fields, ladder links and gates.
8. **Cue banks**: `SimEvent::Cue` resolved by data into sounds (`pav_audio`) and FX
   (`pav_view::cue_fx`).
9. **Rendering scale**: frustum and distance culling by category, two terrain LODs, an ocean LOD,
   camera-fitted shadows, quality presets, static batching — the town at High fits in 2,461
   instances.
10. Runtime game registration, checkpoints outside `SimState`, a key-value persistence API
    (native file + localStorage), `InputFrame.look/triggers`, data-driven bindings, haptics
    (gilrs and the browser Gamepad API), draw-list particle emitters, map contours from
    heightfields.

## Engine source I had to read, and why

- `character.rs` and the controller: to add parkour without breaking Shardfall's feel tests.
- `sim.rs` step order: zone events were updated after the game's `post` (lost to games).
- `statics.rs` (regions, chunk versions) and `pav_view/src/build.rs` (mesh keys, batching): to
  place a whole island's geometry and to find why some terrain chunks drew the wrong mesh.
- The renderer's pass setup, to add the sea and sky passes and the far plane.
- `pav_app` (`app.rs`, touch handling, the play path) to give hook games a front end, settings,
  audio and touch.
- `scripts/pack_single_html.py` and `build-web.sh` for the single-file build.

## Wrong or missing docs; bugs found

- **Terrain mesh keys collided** (`pav_view/src/build.rs` hashed (cx, cz, version) with FNV over
  whole words): some chunk pairs shared a key, so one drew the other's mesh and its land
  vanished. Fixed with an injective key (`pav_core::terrain::render_key`).
- **Zone events were lost to games** (zones updated after `post`); **bombs fired on
  PRIMARY/USE for every game** (fixed with `Weapon::None`). Both now have tests in
  `crates/pav_core/tests/games.rs`.
- **A dead character stays a standing capsule**: nothing in the engine lets the living pass a
  body, so two corpses wedged a fighter and the player in the fort for an hour of game time
  (found by the full-level gate on seed 2). Added `Sim::set_ghost` (the capsule becomes a sensor).
- **Raycasts miss before the first physics step** (the query pipeline is built by `step`): tools
  that probe a freshly loaded scene must step first. Undocumented.
- **A copied `target/` keeps stale `build.rs` output** (embedded data, material goldens): a fresh
  checkout that reuses another checkout's target dir reads old data until `build.rs` is touched.
- **Any drawn primitive with emissive above 0.5 spawns a point light** (`pav_view` `emit_shape`):
  undocumented; effect particles and glints must stay at 0.45 or they flood the scene with lights.
- **`pav repl`'s `load` keeps the previous session's camera settings**: a bot run that passes as a
  one-shot `pav play` can stall after a REPL `load`.
- **The `tools/list` size budget** (48,000 chars, `pav_tools/tests/asset_errors.rs`) leaves no room
  for a game's tools; a game's commands now share one advertised tool (`gcmd`) and still answer
  by name, and the budget was raised to 51,500 with the trade recorded.
- **The store index goes stale silently** when a core asset is added (`pav assets reindex=store`);
  the test that catches it says so, but nothing reminds the author at the time.
- `docs/MAKE_A_GAME.md` assumes a game's level is one room file: there is no guidance for large
  outdoor levels, streaming, terrain or water.
- The engine had no frustum culling or LOD, a ~300 m far plane and shadows ≤ 120 m: fine for
  rooms, not for a sea.

## If I could change one thing about the engine

**Make the game → presentation boundary a data contract that every consumer reads** — what
`PlayFrame` became here: the camera, a UI model, a draw list, atmosphere, audio state and
effects, produced by the game each frame and consumed identically by the renderer, the app, the
browser build and the tools.

Shardfall's own open issue says it: "The HUD, inventory, passive tree and touch controls (egui
in `pav_app`) are invisible to the tools and headless captures". That breaks doctrine 3
(verifiable without a display) exactly where games differ most. With the play frame, a HUD is a
`UiModel` a test asserts on and `pav ui` rasterises; a cinematic is a camera hint and a
letterbox value; a sound is a cue name in an event list; the attract mode is the same game with
the bot's input. Every later system in this repository — the front end, attract, captures at
three resolutions, the browser checks, determinism tests that include the camera — was cheap
because of it. Culling, a better renderer or more generators would each have saved one system;
this saved the verification of all of them.

## Rows for `docs/FEEDBACK.md`

| Game | Built on | Engine parts used | Added to the engine | Biggest gap | Would change |
|---|---|---|---|---|---|
| Assassin's Keg: Black Tap (AC IV-like, sea + islands + stealth) | Pavilion @ `0098196` | Sim, games hook, controller, statics, puppets, renderer, audio, `pav` tools, single-file web | Hook v2 play frame, `pav_ui`, `pav_kit`, island/structure/vegetation generators, ocean, traversal, nav25, cue banks, persistence, culling/LOD | Large outdoor worlds and game UI visible to tools | A data contract for everything the player sees and hears (`PlayFrame`) |
