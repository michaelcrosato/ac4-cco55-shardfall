# Content guide: Assassin's Keg — Black Tap

For the next coding agent: how to add each kind of content, with a worked example and the
commands that check it. Rules are Rust in `crates/blacktap/src/`; content is data in
`games/blacktap/`. Where a kind still needs Rust, the section names the functions. Field names
are the parsers' own (each section names the struct). Every example below was added to the
real data (or code), checked with the commands shown, and removed again (2026-10-17).

Run everything from the repository root after `cargo build -p pav_tools --bin pav` (in cloud
containers with `CARGO_BUILD_JOBS=4`). `pav` below means `./target/debug/pav`.

## The pipeline in 10 lines

```text
games/blacktap/*.toml, shanties/*.abc, ui/theme.toml   (build.rs embeds them all)
blacktap::data(path)   the file on disk when the cwd is the repo root, else the embedded copy
content::load()        game, camera, atmosphere, demo, credits, ships, missions/r1..r6, dialog
world::load(text)      tab1.toml | drydock.toml → World: markers, routes, npcs, perches, collectibles, hints
island::prebuilt_file  tab1_world.toml → pav_core island + structures + vegetation (once per process)
looks.rs               npcs.toml archetypes: look, combat, senses (once per process)
game::build            scenes blacktap[/round/N[@step] | /demo/N | /freeroam | /drydock | /title] → Sim
rounds.rs              MissionRun ticks the Round; `game = "..."` names are ACTS / CONDS (+ stealth.rs)
present.rs             draw list, camera, HUD, AudioState → the app's sounds/music/ambience/fx banks
pav tools, tests/      level_check, markers, ray, ship, mission, play, capture, tour, attract; cargo tests
```

| You edit | It shows |
|---|---|
| missions, dialog, atmosphere, ships, camera, demo, game.toml, tab1.toml, drydock.toml | at the next `load` |
| tab1_world.toml, drydock_world.toml, npcs.toml | in a new `pav` process (cached per process) |
| sounds, music, ambience; fx.toml | when the app starts (`pav_audio` tests read the files; `pav` reads fx at `load`) |
| a new `shanties/*.abc` | after a rebuild (the front end lists the embedded songs) |
| anything, for `cargo test -p blacktap` | by itself: tests run in `crates/blacktap` on the embedded copy, which cargo re-embeds |

## Verification tools

| Command | Checks |
|---|---|
| `pav level_check scene=blacktap` | Tab I's references: step markers, dialog lines, `game` names, atmospheres, `place_ship`/`player_at` markers, demo scenes build, structures on roads, NPC archetypes and routes, perch `leap_to`, routes of 2+ points, camera contexts |
| `cargo test -p blacktap --lib --test data --test drydock` | looks name known gear; `level_check`; every scene builds and runs; the front end (5 attract segments); the Drydock's stations and pads |
| `cargo test -p pav_audio --test bank --test render` | sounds, ambience, music, every ABC tune; each song, state and cue renders audibly |
| `cargo test -p pav_core --test structures` | every structure kind: deterministic, climbable, a walkable top, in the yard |
| `BT_SCENE=blacktap/round/4 cargo test -p blacktap --test trace -- --ignored --nocapture` | the bot plays a scene; a status line every 10 s (`BT_SEED`, `BT_SECS`, `BT_EVERY`) |
| `pav play scene=blacktap/round/4 bot=true ticks=5400` | the bot plays; `status`: round, step, flags, optional_done, stats, showcase |
| `pav mission scene=blacktap/round/4@leap` | steps, flags, counters, optional checklist; `goto=round/N@step`, `complete=<step>` |
| `pav markers scene=blacktap name=chapel` | world markers: tab1.toml's, structures' `<name>.<marker>`, `[marker_from]` aliases |
| `pav ray scene=blacktap ticks=1 from=[x,40,z]` | what a ray hits (down, `max=50`): point, block flags (needs a tick after loading) |
| `pav ship scene=blacktap/round/1` | the player's ship and every other ship (class, tag, hull) |
| `pav perception scene=blacktap/round/5 name=guard` | each NPC's position, awareness, who sees the player |
| `capture [ui=true]`, `tour`, `camera eye= look=`, `teleport pos=`, `ocean preset=<atmosphere>` | a screenshot (with HUD and overlays), the six vistas; a fixed shot, move the player, set a preset |
| `pav attract minutes=6` | the attract playlist headless: segments, mechanics shown, stalls (seconds of wall time) |

One-shot calls take `scene=`, `seed=` and `ticks=`; `printf 'load scene=...\nstep
ticks=60\ncapture\n' | pav repl` keeps one session. For bot runs use one-shot `pav play` or the
trace test: in a REPL, `load` keeps the previous session's camera settings, and a bot run after
it can differ from the same run started fresh.

---

## 1. An island

**Where:** `[[island]]` in `tab1_world.toml` (`pav_core::island::IslandDef`; unknown fields are
errors): `name outline seed beach shelf rise peaks cliffs noise flatten roads grass cell`, with
`peaks = [{ at, height, radius }]`, `cliffs = [{ from, to, height }]`, `noise = { scale, amp,
seed }`, `flatten = [{ polygon, height, blend, ground, color }]`, `roads = [{ points, width }]`,
`grass` (stalk-grass polygons: hiding). Grounds: `sand wet_sand grass jungle rock dirt cliff`.
x east, z south, metres. The map's name: `[[map_label]]` in `tab1.toml`. (`[[shape]]` there is
only drawn when there is no generated terrain.)

```toml
# tab1_world.toml — a cay north-east of Templanza
[[island]]
name = "barrel_cay"
seed = 31
outline = [[410, 312], [428, 304], [448, 310], [454, 328], [444, 346], [422, 350], [408, 336]]
beach = 8
shelf = 18
rise = 2
peaks = [{ at = [430, 328], height = 6, radius = 18 }]
noise = { scale = 12, amp = 0.6, seed = 4 }
grass = [[[424, 318], [436, 318], [436, 326], [424, 326]]]

# tab1.toml
[[map_label]]
at = [430.0, 296.0]
text = "Barrel Cay"
```
**Code:** none: `[vegetation]` plants it; collision, seabed and the map follow the heightfield.
**Verify:**
```sh
pav level_check scene=blacktap                     # "ok": true
pav ray scene=blacktap ticks=1 from=[428,40,322]   # "hit": true, ground at y 6.5
printf 'load scene=blacktap/freeroam\ncamera eye=[380,30,270] look=[430,4,328]\ncapture out=out/cay.png width=640 height=360\n' | pav repl
```
A parse error in tab1_world.toml is only logged: the level loses all terrain and structures,
and `level_check` shows it only through missing markers (`chapel_perch`…). `ray` after editing.

## 2. Structures: a placement, and a new kind

**Where:** `[[structure]]` in `tab1_world.toml` (`pav_core::structures::StructureDef`; unknown
fields are errors): `kind name at yaw size floors style seed base height to balcony awning open
sign cannons stairs pitched`. `yaw` 0/90/180/270 turns the front (local +z; south at 0)
clockwise; `style`: `ochre coral seafoam sky lime limestone timber`; `balcony`, `stairs`: `front
back left right none`; `base` overrides the floor height; `to` ends a line. Kinds: `house
chapel warehouse hall tavern stall stocks hay_cart pier quay harbour_wall fort_wall bastion
battery mortar_tower keep gate barracks lighthouse watchtower_ruin hut laundry_line beam
palisade crate_stack wanted_poster paving` (lines rest on roofs listed before them).

Structures publish markers `<name>.<marker>` (always `center`; per kind `door roof perch
platform mortar flagpole bell`…). Rules name them in tab1.toml's `[marker_from]`
(`alias = "name.marker"`, optionally `"name.marker+dx,dy,dz"`).

```toml
# tab1_world.toml
[[structure]]
kind = "hut"
name = "cay_hut"
at = [432, 334]
yaw = 180
seed = 6

# tab1.toml, in the existing [marker_from] table
cay_hut = "cay_hut.door"
```
**Verify:** `pav markers scene=blacktap name=cay_hut` → `cay_hut`, `cay_hut.center`,
`cay_hut.door` (y 7.4–7.7); `pav level_check scene=blacktap` (no footprint on a road). A
`[marker_from]` naming a missing marker is only a log warning; `level_check` sees it only if a
step or perch uses the alias.

**A new kind** (`crates/pav_core/src/structures.rs`): the variant in `enum Kind`, `Kind::ALL`
(bump its length), `name()`, `default_size()`; a generator using the builder `B`
(`foundation`, `mass`, `block(a, b, color, flags, material)` with `CLIMBABLE ROOF POLE BEAM`,
`decor_box`, `prop`, `ladder`, `label`, `area`, `marker`; local x across, y up, z to the front);
its arm in `generate_with`; one in `crates/pav_core/src/yard.toml` (a test wants every kind).

```rust
Kind::Well => well(&mut b, w, d),          // generate_with; default_size: Kind::Well => [2.0, 2.0]

/// A stone well: a square curb you can stand on, two posts and a beam with a bucket.
fn well(b: &mut B, w: f32, d: f32) {
    let (h, t) = (0.9, 0.3);
    b.foundation(Vec2::ZERO, w, d, 0.0, STONE_DARK);
    for (a, c) in [
        (Vec3::new(-w * 0.5, 0.0, -d * 0.5), Vec3::new(w * 0.5, h, -d * 0.5 + t)),
        (Vec3::new(-w * 0.5, 0.0, d * 0.5 - t), Vec3::new(w * 0.5, h, d * 0.5)),
        (Vec3::new(-w * 0.5, 0.0, -d * 0.5 + t), Vec3::new(-w * 0.5 + t, h, d * 0.5 - t)),
        (Vec3::new(w * 0.5 - t, 0.0, -d * 0.5 + t), Vec3::new(w * 0.5, h, d * 0.5 - t)),
    ] {
        b.block(a, c, LIMESTONE, CLIMBABLE | ROOF, Some(MAT_LIMESTONE));
    }
    for x in [-w * 0.5 + 0.1, w * 0.5 - 0.25] {
        b.block(Vec3::new(x, h, -0.08), Vec3::new(x + 0.15, h + 1.4, 0.08), TIMBER, POLE, Some(MAT_PLANKS));
    }
    b.block(Vec3::new(-w * 0.5, h + 1.4, -0.08), Vec3::new(w * 0.5, h + 1.55, 0.08), TIMBER_DARK, BEAM | ROOF, Some(MAT_PLANKS));
    b.decor_box(Vec3::new(0.0, h + 0.6, 0.0), Vec3::new(0.3, 0.3, 0.3), "#8a6a48", Some(MAT_PLANKS));
    b.marker("bucket", Vec3::new(0.0, h, 0.0));
}
```
**Verify:** `cargo test -p pav_core --test structures`. It drops a ray onto the highest
walkable block, so whatever is above that must be walkable too: with a plain `BEAM` the well
fails ("the roof … is walkable"); `BEAM | ROOF` passes.

## 3. A ship class

**Where:** `ships.toml`. `[class.<key>]` is `pav_kit::naval::HullClass` (all fields default):
`name length beam draft freeboard hp speed_half speed_full accel brake turn_still turn_mid
turn_full guns_per_side chasers swivels kegs crew masts mast_height muzzle ball_damage reload`
(m/s, °/s, m). `[style.<key>]` is `pav_kit::shipyard::ShipStyle`: `hull bottom wale strake deck
rail sail emblem` (hex), `rig` (`square`, `fore_and_aft`), `figurehead`. The hull, masts and
sails are generated from the numbers.

```toml
[class.cutter]
name = "Revenue Cutter"
length = 18.0
beam = 5.6
draft = 2.0
freeboard = 1.8
hp = 420.0
speed_half = 8.5
speed_full = 13.5
turn_mid = 26.0
turn_full = 20.0
guns_per_side = 4
chasers = 1
crew = 10
masts = 1
mast_height = 15.0
```
**Code:** which ship is which class is Rust: `g.sea.add_ship(&c.classes, "<class>", name, tag,
pos, heading, faction)` in `rounds.rs` (`spawn_*`), `game.rs` (the Black Tap is a `brig`),
`drydock.rs`. Paint comes from the *tag* (`present.rs` `style_for`; unknown tags paint as
`merchant`). An action that spawns any class at a marker:

```rust
// rounds.rs: "spawn_ship:" in ACTS, and in `act`:
"spawn_ship" => {
    // spawn_ship:<class>@<marker>: a ship of that class lying at a marker (a target).
    let (class, at) = arg.and_then(|a| a.split_once('@')).unwrap_or(("schooner", "r1_start"));
    let p = w.marker_xz(at);
    g.sea.add_ship(&c.classes, class, &format!("TS {class}"), class, p, 0.0, Faction::Temperance);
}
```
Then `{ game = "spawn_ship:cutter@r2_start" }` in a Round's `on_start`; a captain
(`g.sea.add_captain(id, CaptainKind::Patrol, route, None, false)`) makes it sail.

**Verify:** `cargo test -p blacktap --test data` (parses); `pav ship scene=blacktap/round/1`
lists `{"class": "cutter", "hull": 420, "tag": "cutter"}`; `camera eye= look=` at the marker and
`capture`. A misspelt class sails as `HullClass::default()` but is never drawn, misspelt fields
are ignored, and `level_check` checks neither class nor marker.

## 4. An NPC archetype and a route

**Where:** `[archetype.<kind>]` in `npcs.toml` (`looks::Archetype`): `look` (`LookDef`: `skin
shirt pants shoes helm helm_color trim hair coat coat_color stripes belt belt_strap gloves
bracer holsters pauldrons armor_color cape cape_color weapon weapon_color weapon_grip scale
bulk`; the header lists helms and weapons), `variants` (partial looks for crowds), `combat`
(`pav_kit::combat::CombatProfile`: `hp damage windup interval reach counter` (`kill`/`stagger`)
`blocks_light ranged speed`), `senses` (`pav_kit::perception::Senses`: `near far fov up down
peripheral peripheral_rate rate_near rate_far hearing`).

In `tab1.toml`: `[route]` (`name = [[x, z], ...]`, looped unless `once`) and `[[npc]]`
(`world::NpcSpawn`: `group kind at facing route once tags count`). `group` is the action that
places them: `town` (`spawn_town`), `tavern`, `courier`, `stocks` (`spawn_stocks_guards`),
`fort` (`spawn_fort_guards`), `drye` (`drye_balcony`); a new group needs an action in
`stealth::act` and `ACTS`. In `stealth.rs` `spawn`, `civilian` and `molasses` are not
fighters; the role is Follow (tag `escort`), Stocks (molasses), Gossip (tag `gossip`: a crowd
that hides the player), Patrol (a route), Wander (civilians) or Post (facing `facing`, degrees
from north). Tags with code behind them: `courier escort gossip critical` (never frozen).

```toml
# npcs.toml
[archetype.dockhand]
look = { skin = "#a87050", shirt = "#efe7d6", stripes = "#2c3e66", pants = "#5a4632", helm = "cap", helm_color = "#3a4a5a", weapon = "none" }
combat = { hp = 30.0, damage = 0.0, speed = 2.6 }

# tab1.toml: quay_walk in the existing [route] table, then the person
quay_walk = [[160.0, 40.0], [214.0, 40.0]]

[[npc]]
group = "town"
kind = "dockhand"
at = [160.0, 2.4, 40.0]
route = "quay_walk"
```
**Code:** none (any kind but `civilian`/`molasses` gets a fighter's body; with `damage = 0` he
only patrols). **Verify:**
```sh
pav level_check scene=blacktap        # without the archetype: "unknown archetype 'dockhand'"
cargo test -p blacktap --lib          # helm and weapon names are known
pav perception scene=blacktap/round/4 ticks=600 name=dockhand   # spawned by spawn_town, x 160 → 163
```

## 5. A mission (Round): steps, optional objectives, checkpoints

**Where:** `missions/r1.toml` … `r6.toml` (`pav_kit::mission::MissionDef`): `id title setup
on_start on_complete`, `[[step]]` (`id objective markers on_start done on_done fail checkpoint
bot`), `[[optional]]` (`id text track fail at_end`). `setup` runs only when the Round starts
from its own scene (Replay, tests, attract); `on_start` also when the previous Round flows in.

- **Conditions:** `"always"`, `"never"`, `{ flag }`, `{ all = [..] }`, `{ any = [..] }`,
  `{ not = .. }`, `{ timer = s }` (in this step), `{ before = s }` (the Round's clock),
  `{ counter = { name, at_least } }`, `{ game = "name" }` (`rounds::CONDS`).
- **Actions:** `{ flag }`, `{ unflag }`, `{ count = { name, add } }`, `{ line }`, `{ music }`,
  `{ atmosphere }`, `"checkpoint"`, `{ banner }`, `{ toast }`, `{ game = "name[:arg]" }` (`ACTS`).
- **Step markers:** `marker:<name> ship:<tag> tag:<ship tag> npc:<kind or tag>
  battery:west|north|mortar flotsam`.
- **Flags from data:** `on_<perch>_perch` (standing on a `[[perch]]`), `surveyed_<perch>`,
  `leaped`; the rules set the rest (`left_helm boarded boarding_won plunder_done has_key
  in_fort drye_dead flag_raised detected`…; `pav mission` shows them). Counters: `flotsam`,
  `board_counters`.
- **Checkpoints:** `checkpoint = true` saves as the step starts (so do Round starts and each
  battery falling); a step's `fail` Spills the player back to the last one.

Worked example, all data: a rooftop step at the end of Round 4, done by standing on a new perch,
with a line, a checkpoint, an optional objective and a bot hint (section 13).

```toml
# missions/r4.toml, after the "leap" step
[[step]]
id = "rooftop"
objective = "Climb onto the ochre house's roof"
markers = ["marker:casa_roof"]
on_start = [{ line = "r4_rooftop" }]
done = { flag = "on_casa_roof_perch" }
checkpoint = true
bot = "climb_to:casa_roof"

[[optional]]
id = "r4_rooftop_fast"
text = "Reach the rooftop within 4:00 of landing in the Round"
track = { all = [{ flag = "on_casa_roof_perch" }, { before = 240.0 }] }

# tab1.toml: casa_roof in [marker_from], then the perch
casa_roof = "casa_x10.roof"

[[perch]]
name = "casa_roof"
marker = "casa_roof"
reveals = 120.0

# dialog.toml
[line.r4_rooftop]
speaker = "pip"
text = "Up on the roofs, Captain — the whole plaza's yours from there."
```
**Verify:**
```sh
pav level_check scene=blacktap                       # the line and the step's marker resolve
pav mission scene=blacktap/round/4@rooftop           # "step": "rooftop", the optional listed
pav play scene=blacktap/round/4@leap bot=true ticks=600
#   flags: on_casa_roof_perch; optional_done: r4_rooftop_fast; stats.rounds_done: 1
BT_SCENE=blacktap/round/4@leap BT_SECS=60 cargo test -p blacktap --test trace -- --ignored --nocapture
```
**A new game condition or action:** its name in `CONDS` or `ACTS` (a trailing `:` takes an
argument) in `rounds.rs`, an arm in `eval` / `act` (land ones fall through to `stealth.rs`).

**A seventh Round** is code too: `content.rs` loads `r1..=6`; `game.rs` `parse_scene` clamps
`round/<1-6>` and `post` flows on while `self.round < 6`; `commands.rs` `mission goto` allows
1..6; `rounds.rs` `tab_complete`, `pour` (`/ 6.0`), `progress`, `restore_progress` assume six.
Add a `[[chapter]]` to `game.toml` and the scene to `tests/data.rs`.

## 6. Dialog lines and barks

**Where:** `dialog.toml` (`pav_kit::dialog::DialogBank`): `[speaker.<id>]` (`name color voice`),
`[line.<id>]` (`speaker text duration priority`; duration 0 = from the text's length; higher
priority cuts in), `[bark.<id>]` (`speaker lines cooldown`). `voice` is a blip voice,
`[speaker.<voice>]` in sounds.toml (`pitch vowels`).

Lines play from missions (`{ line = "id" }`, checked by `level_check`; example in section 5) or
code (`g.dialog.say(&c.dialog, "id")`). Barks play only from code: `g.dialog.bark(&c.dialog,
"id", seed)` picks a seeded line and starts the cooldown, but shows it only when no line is
playing or none is queued (else it is dropped, cooldown spent). Wired: `crew_cheer`,
`ballast_brace`. Written but never played: `guard_suspicious guard_alert guard_search
guard_giveup crier` (the guards' mind events play only the sound cues of the same names).
Wiring one, in `stealth.rs` `perceive`:

```rust
Some(MindEvent::Suspicious) => {
    events.push(SimEvent::cue("guard_suspicious", feet + Vec3::Y * 1.8));
    let c = g.content();
    g.dialog.bark(&c.dialog, "guard_suspicious", id.0 as u64);
}
```
**Verify:** `level_check` (mission lines); the Drydock Theatre's *Next line* pad plays every
line in turn; subtitles show in `capture ui=true`. The bark: `load scene=blacktap/round/5`,
`step ticks=400`, `teleport pos=[167,2.5,16]`, `step ticks=40`: `perception name=guard` shows a
guard `suspicious` and `snapshot_save` holds the bark's cooldown.

## 7. A cinematic

**Status:** no data-driven cinematics yet. The one scripted scene, *Last Words*, is code: the
action `last_words` (`stealth::act`) starts the timer `g.stealth.last_words` (lines at 3.5 and
7.5 s, over at 11.5 s); `stealth::cinematic_camera` orbits Drye, `stealth::letterbox` and
`stealth::white` add bars and mist (atmosphere `memory_mist`). The Drydock Theatre replays it.

`pav_kit::cinematic` has a data format and a player, not wired in: `[scene.<name>]` (`letterbox
length time_scale lines = [[seconds, "line_id"], ...]`) with `[[scene.<name>.shot]]` keys (`at
eye target fov cut`; metres from an anchor, x right, z toward the viewer, turned by its yaw);
`Playing::new(name, anchor, yaw)`, `tick(def, dt, skip_held)` → (lines, over), `camera(def)`.

```toml
# games/blacktap/cinematics.toml (new)
[scene.guns_silent]
letterbox = true
lines = [[0.5, "r3_done"]]
[[scene.guns_silent.shot]]
at = 0.0
eye = [0, 8, 34]
target = [0, 6, 0]
fov = 50
cut = true
[[scene.guns_silent.shot]]
at = 4.0
eye = [26, 18, 22]
target = [0, 8, 0]
fov = 42
```
Wiring (five edits; checked to build and play):
1. `content.rs`: `pub scenes: pav_kit::cinematic::Scenes` in `Content`; in `load()`
   `let scenes = crate::data("cinematics.toml").map(|t| pav_kit::cinematic::Scenes::parse(&t)).transpose().context("cinematics.toml")?.unwrap_or_default();`
2. `game.rs`: `#[serde(default)] pub cine: Option<pav_kit::cinematic::Playing>` in `BlackTap`;
   `cine: None` in `build`.
3. `rounds.rs`: `"cinematic:"` in `ACTS`; in `act`:
   ```rust
   "cinematic" => {
       // cinematic:<scene>@<marker>: a shot list anchored at a marker (default: the player).
       let (name, at) = match arg.and_then(|a| a.split_once('@')) {
           Some((n, m)) => (n, w.marker(m)),
           None => (arg.unwrap_or(""), None),
       };
       let anchor = at.or_else(|| BlackTap::player_feet(sim)).unwrap_or_default();
       g.cine = Some(pav_kit::cinematic::Playing::new(name, anchor, 0.0));
   }
   ```
4. `rounds.rs` `post`, after `stealth::post`:
   ```rust
   if let Some(mut p) = g.cine.take() {
       let c = g.content();
       if let Some(def) = c.scenes.scene.get(&p.name) {
           let (lines, done) = p.tick(def, dt, false);
           for l in lines {
               g.dialog.say(&c.dialog, &l);
           }
           if !done {
               g.cine = Some(p);
           }
       }
   }
   ```
5. `present.rs`: first in `camera`,
   `let c = g.content(); if let Some(h) = g.cine.as_ref().and_then(|p| p.camera(c.scenes.scene.get(&p.name)?)) { return Some(h); }`;
   and `letterbox: crate::stealth::letterbox(g).max(if g.cine.is_some() { 1.0 } else { 0.0 })`.

Play it from a step: `{ game = "cinematic:guns_silent@fort_keep" }`. To skip on a held ◯, tick
it from `BlackTap::pre` with `input.down(b::DROP)` instead of `false`.

**Verify** (the action in section 5's step's `on_start`):
`printf 'load scene=blacktap/round/4@rooftop\nstep ticks=90\ncapture ui=true out=out/cine.png\nstep ticks=200\ncapture ui=true out=out/cine2.png\n' | pav repl`
→ the keep from the south, letterboxed, then from the south-east.

## 8. A shanty (ABC) and a music state

**Where:** the tune in `shanties/<name>.abc` (the subset in `crates/pav_audio/src/abc.rs`:
`X T C M L Q K`, notes, accidentals, lengths, broken rhythm, tuplets, repeats and endings,
chords, chord symbols `"Am"`, lyrics `w:`); its arrangement `[song.<name>]` in `music.toml`
(`pav_audio::music::SongSpec`: `about abc tempo passes gap transpose gain humanize section
part`; parts are `PartSpec`: `inst role gain pan octave transpose stem stem_max pattern passes
bars sections legato humanize swing accent vel vowel voices call reverb echo strum low`; the
file's header explains instruments, roles and patterns). A tune with no `[song]` gets a
default band. `[state.<name>]`: `songs intro crossfade shuffle`; `[stinger.<name>]`: `song duck
gain`. Unknown fields are errors.

The game plays the state (or song) named by `g.music`, which missions (`{ music = "explore" }`)
and code set; `sailing` is the shanty rotation; `title`, `attract` are fixed. Stems follow the
intensity: the highest guard awareness, at least 0.5 (0.9 under fire).

```abc
X:20
T:The Dry Run
C:Original, for Assassin's Keg
M:6/8
L:1/8
Q:3/8=104
K:G
|:"G"GAB d2B | "C"c2e "G"d2B | "Am"A2c "D"B2A | "G"G3 "D"D3 |
"G"GAB d2B | "C"c2e "G"d2g | "D"f2d "C"e2c | "D"d3 "G"G3 :|
```
```toml
# music.toml
[song.the_dry_run]
about = "a quick jig for the tavern"
passes = 2
gap = 3
[[song.the_dry_run.part]]
inst = "fiddle"
role = "melody"
gain = 0.6
[[song.the_dry_run.part]]
inst = "guitar"
role = "chords"
gain = 0.7
[[song.the_dry_run.part]]
inst = "bodhran"
role = "drums"
stem = 0.3

[state.tavern]
about = "inside the Last Drop"
songs = ["the_dry_run", "drunken_sailor"]
crossfade = 1.5
```
Use it with `{ music = "tavern" }`, or add the song to `[state.sailing].songs`.

**Verify:** `cargo test -p pav_audio --test bank --test render` (parses; bars full, pickup +
last bar = one bar, range G2–A6; 30 s of every song and state render audible and unclipped).
The Drydock Jukebox's *Next song* plays every `[song]`. `level_check` does not check music
names: an unknown one logs `music: no state or song "x"` and the old music plays on.

## 9. A sound cue and an FX cue

A cue is a name the rules emit, `events.push(SimEvent::cue("name", pos))`; the app plays
`[cue.name]` from `sounds.toml` and shows `[cue.name]` from `fx.toml` (either may be missing).

**Sound** (`pav_audio::bank`; unknown fields are errors; the header documents every field):
`about` (required by the tests) `range bus positional reverb echo rumble = { low, high, ms }
stinger cooldown pitch_rand gain_rand`, and `[[cue.<name>.layer]]`: `wave` (`sine square saw
triangle noise pink brown dust pluck voice`), `freq freq_end glide filter cutoff cutoff_end q
attack decay sustain hold release delay drive gain stereo lfo`…

**FX** (`pav_view::cue_fx::FxBank`): `particles = [{ preset count spread size life up area
color offset }]`, `shock` (shockwave radius, m), `water` (dent in the sea, m), `flash = { color
intensity radius seconds }`. Presets (`pav_view/src/fx.rs`): `smoke steam sparks embers fountain
snow rain fireflies magic dust bubbles confetti`; any other name (a typo too) gives fire.

```toml
# sounds.toml
[cue.pickup_map]
about = "a chart unrolled: a paper flutter and a bright two-note chime"
range = 30
[[cue.pickup_map.layer]]
wave = "pink"
filter = "bp"
cutoff = 3200
decay = 0.25
gain = 0.5
[[cue.pickup_map.layer]]
wave = "triangle"
freq = 880
decay = 0.4
gain = 0.35
[[cue.pickup_map.layer]]
wave = "triangle"
freq = 1320
decay = 0.5
delay = 0.12
gain = 0.3

# fx.toml
[cue.pickup_map]
particles = [{ preset = "confetti", count = 14, spread = 1.2, size = 0.6, color = "#e8d9a8" }]
flash = { color = "#ffe2a0", intensity = 2.0, radius = 4.0, seconds = 0.2 }
```
**Code:** emit it where it happens (section 11 does); add cues the game must have to `CUES` in
`crates/pav_audio/tests/bank.rs`.

**Verify:** `cargo test -p pav_audio --test bank --test render` (parses, `about` set, under
12 s, audible); the Drydock Jukebox's *Sound bank* pad plays each cue in turn (the toast names
it). `fx.toml` has no test and `pav` ignores a parse error in it; a throwaway test is three
lines: `FxBank::parse(include_str!("../../../games/blacktap/fx.toml"))`, then
`bursts("pickup_map", Vec3::ZERO, 1.0)` (one burst of 14).

## 10. An atmosphere preset

**Where:** a table per preset in `atmosphere.toml` (`content::AtmosphereDef`, all optional):
`sun_azimuth sun_elevation sun_color sun_intensity ambient_sky ambient_ground ambient sky_zenith
sky_horizon fog_color fog_start fog_end cloud_cover cloud_color rain wetness wind exposure
saturation contrast bloom sea` (hex; `wind` [x, z] m/s; `sea`: `flat calm breezy rough storm`).

```toml
[overcast]
sun_azimuth = 210.0
sun_elevation = 30.0
sun_color = "#e6e2da"
sun_intensity = 0.7
ambient_sky = "#b8c2cc"
ambient = 0.7
sky_zenith = "#7d8a96"
sky_horizon = "#c4ccd2"
fog_color = "#bcc4ca"
fog_start = 160.0
fog_end = 1100.0
cloud_cover = 0.85
cloud_color = "#d0d4d8"
wind = [7.0, -4.0]
saturation = 0.85
sea = "rough"
```
**Use:** `{ atmosphere = "overcast" }` in a mission (a 4 s blend), `g.set_atmosphere("overcast",
seconds)` in code, an `atmos:overcast` pad (section 15).

**Verify:** `printf 'load scene=blacktap/freeroam\nocean preset=overcast\nstep ticks=10\ncapture out=out/overcast.png width=640 height=360\n' | pav repl`.
`level_check` covers presets named by missions, the drydock test those named by pads. Typos in
fields are ignored and an unknown `sea` gives no sea state: look at the capture.

## 11. A collectible, and a new collectible kind

**Where:** `[[collectible]]` in `tab1.toml` (`world::Collectible`: `kind name at`; `y = 0` puts
it on the ground). `name` is the save key: keep it unique. Kinds: `sheet bottle chest keg_tap`.
Taken on foot within 1.8 m, or (at `y < 0.6`) by sailing within 12 m. A surveyed perch reveals
those within its `reveals` (at least 150 m) on the map. Flotsam is separate: one crate at each
marker `flotsam_1..6`.

```toml
[[collectible]]
kind = "chest"
name = "chest_barrel_cay"
at = [436.0, 0.0, 330.0]
```
**Verify:** `printf 'load scene=blacktap/round/5\nteleport pos=[436,8,330]\nstep ticks=90\nplay\n' | pav repl`
→ `stats.collectibles` 1, `reales` 250. (`level_check` does not check kinds.)

**A new kind** is code in four places, plus its cue (section 9): `stealth.rs` `post` (the
pickup `match c.kind`: cue, toast, reales) and `draw` (shape, colour); `map.rs` `icons` (unknown
kinds get no icon) and `pav_ui/src/map.rs` `icon_name` / `paint.rs` `icon` for the legend;
`present.rs` `results`, whose total `"{} / 26"` (20 entries + 6 flotsam) is written in.

```rust
"map_piece" => ("pickup_map", "Map Piece: a corner of Drye's chart", 0),       // stealth.rs, pickup
"map_piece" => (Shape::Box { half: Vec3::new(0.3, 0.02, 0.22) }, "#e8d9a8"),   // stealth.rs, draw
k @ ("sheet" | "bottle" | "chest" | "map_piece") => k,                         // map.rs
```
Checked with `kind = "map_piece"` at `[426.0, 0.0, 330.0]` and `teleport pos=[426,8,330]`: the
toast *Map Piece: a corner of Drye's chart* shows.

## 12. An upgrade

**Status:** code only (no `upgrades.toml`). The *Shipwright* tiles after the Round 2 boarding
offer four; the choice is saved as a flag `upgrade_<key>` and re-applied on load
(`restore_progress`). The tile row draws at most four (`pav_ui` `hud.rs` `tile_row`): a fifth
would be selectable but invisible, so replace one or widen `tile_row`. Three edits: `foot.rs`
`plunder_tiles` (the `Tile { name, desc, cost, icon, disabled }`), `foot.rs` `apply_choice`
(the key at that index), `rounds.rs` `apply_upgrade` (the effect).

Worked example: *Extra Timber* in place of *Swift Rigging* (which sets a flag nothing reads, so
it does nothing today):
```rust
Tile { name: "Extra Timber".into(), desc: "+6 timber for repairs".into(), cost: "free".into(), icon: "repair".into(), disabled: false },
let up = ["reinforced_hull", "heavy_shot", "extra_timber", "more_kegs"][choice.min(3)];
"extra_timber" => g.sea.timber += 6,
```
**Verify:** the Drydock UI Gallery's *Tiles* pad shows the Shipwright tiles:
`printf 'load scene=blacktap/drydock\nteleport pos=[45.5,2.1,24]\nstep ticks=20\ncapture ui=true out=out/tiles.png\n' | pav repl`.

## 13. A bot hint

**Where:** `[[bot_hint]]` in `tab1.toml` (`world::BotHint`): `name`, `points` ([x, y, z]; `y = 0`
is on the ground at any height), `do` (per point: `walk run climb jump dive crouch aboard
interact wait`; others walk). The bot heads for the point after the segment nearest it and
moves *to point k as `do[k]` says*: a climb is the wall's foot and the top, both `climb` (it
free-runs into the nearest wall toward the next point), then a `walk` onto the roof.

A step's `bot` skill names the hint: `climb_to:<x>` follows `r4_climb_<x>`; `infiltrate`,
`raise_flag`, `escape_to_ship` follow `r6_infiltrate`, `r6_flag`, `r6_dive`. Skills (`bot.rs`
at sea, `bot_land.rs` ashore): `raise_sails collect_flotsam tag_restraint fight_ships board
bombard sail_to:<marker> leave_helm swim_ashore climb_to:<x> survey leap get_key free_molasses
infiltrate assassinate:<kind> raise_flag escape_to_ship wait`; `melee`, `choose` on boarding.

```toml
# For section 5's step (bot = "climb_to:casa_roof"): from the hay cart, round the corner of
# casa_x10 and up its east wall onto the roof.
[[bot_hint]]
name = "r4_climb_casa_roof"
points = [[164.0, 0.0, 14.0], [164.0, 0.0, 22.0], [163.0, 2.4, 22.0], [162.0, 9.1, 22.0], [158.0, 9.1, 22.0]]
do = ["walk", "walk", "climb", "climb", "walk"]
```
**Verify:** `BT_SCENE=blacktap/round/4@leap BT_SECS=60 cargo test -p blacktap --test trace --
--ignored --nocapture` prints the bot on the roof at 10 s and in Round 5 by 20 s. `ray ticks=1`
gives heights for the points. Pick a plain wall: the same hint up the seafoam house's front
(balcony, awning) stalled at 5 m. `level_check` does not check hints or their `do` lengths.

## 14. An attract segment

**Where:** `[[segment]]` in `demo.toml` (`pav_core::play::DemoSegment`: `name scene caption seeds
seconds`), played in order by the bot. `blacktap/demo/<n>` is a Round and start step chosen in
`game.rs` `parse_scene`, run with the showcase personality; caption and time limit come from
the `n`th segment. Changing a caption, seeds or limit is data only.

**A new segment** is code too: its Round and step in the `["demo", n]` match of `parse_scene`,
both `clamp(1, 5)` raised; `tests/data.rs` expects five segments and
`pav_tools/tests/attract.rs` every scene to be `blacktap/demo/*`.

```toml
[[segment]]
name = "Gunboats"
scene = "blacktap/demo/6"
caption = "Gunboats — two at once, broadside to broadside"
seeds = [61, 62, 63, 64, 65]
seconds = 45
```
```rust
6 => (2, None),          // game.rs parse_scene: Round 2 from its own setup
```
**Verify:** `pav play scene=blacktap/demo/6 bot=true ticks=2700` → `status.showcase`: `done`,
`stalled: false`, 7 mechanics (seed 1). Then `pav attract minutes=8`: all six played, no stalls,
but on its first seed (61) this one showed only 2 mechanics (`segments_under_3_mechanics: 1`;
the attract test wants 3), so it needs tuning (start step, seconds). The watchdog ends a
segment after 25 s with no showcase event and no 10 m of travel: Round 1 from its setup
(`6 => (1, None)`) stalls.

## 15. A Drydock station

**Where:** `[[station]]` in `drydock.toml` (`drydock::StationDef`): `key name kind` (`sea
traversal combat presentation`), `at` (the sign; `y = 0` on the ground), `facing` (the way it
reads, degrees from north), `afloat` (sign in the water, pads are rings the ship sails
through), `radius` (where its guide shows), `learn` (the guide's line), `pads = [{ at, effect,
label, radius }]`. Effects (`drydock.rs` `EFFECTS`, `trigger`): `ship:board|refit|home`,
`map:open`, `ui:tiles|wheel|results|map|cinematic|off`, `wind:veer|back`, `flotsam:`,
`gunnery:`, `duel:`, `mortar:`, `hulk:`, `lift:<marker>`, `slowmo:`, `survey:`,
`ring:spar|stand_down|slow|fresh`, `tools:restock|wake`, `yard:cones|reset`, `heat:up|clear`,
`poster:`, `market:`, `theatre:last_words|line|banner`, `song:prev|next`, `mix:`, `sound:`,
`atmos:<preset>`, `weather:squall`, `camera:<context>`, `rumble:tap|broadside|hull|blast`.
People: `[[npc]]` groups `ring range yard market`; geometry: `drydock_world.toml`.

```toml
[[station]]
key = "lookout"
name = "Lookout"
kind = "traversal"
at = [-14.0, 0.0, -14.0]
facing = 180.0
radius = 6.0
learn = "Lifts to every perch; survey from the top, then take the Leap of Spirits."
pads = [
  { at = [-16.0, 0.0, -11.0], effect = "lift:climb_perch", label = "Lift: roof" },
  { at = [-12.0, 0.0, -11.0], effect = "atmos:overcast", label = "Overcast" },
]
```
(`atmos:overcast` needs section 10's preset.) **Code:** none with existing effects; a new one
is a name in `EFFECTS`, an arm in `trigger` calling an `fx_*` function, a rule in `check`.

**Verify:**
```sh
cargo test -p blacktap --test drydock          # bad effects, missing lift markers; every land pad fires
printf 'load scene=blacktap/drydock\nteleport pos=[-12,2.1,-11]\nstep ticks=30\ncamera eye=[-14,8,-1] look=[-14,2,-14]\ncapture ui=true out=out/lookout.png\n' | pav repl
```
→ the sign, both pads, the guide and the *Weather: Overcast* toast.

---

## Other data files

`game.toml` (`content::GameInfo`: title, ids, the title/new-game/training scenes, chapters);
`camera.toml` (`pav_kit::camera::CameraContexts`; `level_check` wants the game's 11 contexts);
`ambience.toml` (`pav_audio::AmbienceBank`; `present.rs` `audio` sets the layers' levels);
`bindings.toml` (`pav_ui::Bindings`; never renumber a `bit`); `ui/theme.toml` (`pav_ui::Theme`;
unknown names are errors); `credits.toml`; `drydock_world.toml` (geometry, as tab1_world.toml).

## What no check catches

Islands, structures, vegetation, music, sounds and the theme reject unknown fields; missions,
tab1.toml, ships, npcs, atmospheres and fx ignore them. Nothing checks a broken
`tab1_world.toml` (beyond missing markers), `[marker_from]` targets, music names, collectible
kinds, bot hints, FX presets or `fx.toml`, or classes and markers named in code or `spawn_*`
arguments. After such an edit, look: `markers`, `ray`, `ship`, `capture`, a trace.
