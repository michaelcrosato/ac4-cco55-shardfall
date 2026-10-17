# Assassin's Keg: Black Tap

**`[AssassinsKegBlackTap-W9]`** · *Nothing is watered down; everything is on tap.*

An original spiritual successor to *Assassin's Creed IV: Black Flag*, as one polished level: a
1.2 km stretch of Caribbean sea with three islands. Captain Cask sails the brig *Black Tap*
against the Temperance Fleet — trading broadsides with gunboats, chain-shotting and boarding the
schooner *Restraint*, shelling Fort Abstinence's batteries under its mortar — then swims ashore,
climbs the chapel tower of Dryport, surveys the bay, takes the Leap of Spirits, slips through
the town after the courier with the cellar key, frees Mother Molasses from the stocks, crawls
the smugglers' tunnel into the fort, ends Commodore Drye, raises the Tap flag over the keep and
dives off the battlement to escape through a storm. It runs in the browser from one HTML file
and natively, on the Pavilion engine from [shardfall](https://github.com/michaelcrosato/shardfall),
extended here into a foundation for the full game.

## Source Information

| | |
|---|---|
| **Initial Date** | 2026-10-17 |
| **SOURCE** | *Assassin's Creed IV: Black Flag* (Ubisoft Montreal, 2013; PS3, PS4, Xbox 360, Xbox One, Wii U, PC) |
| **GAME ENGINE** | Pavilion (shardfall), https://github.com/michaelcrosato/shardfall at commit `0098196` ("Co-op plan on hold (#74)"), workspace 0.1.0: Rust 1.98.1 (edition 2024), wgpu 30 (WebGPU; Vulkan/DX12 natively), rapier3d 0.36, egui 0.36, winit 0.30.13, gilrs 0.11, cpal 0.18, glam 0.33, wasm-bindgen 0.2.129. Vendored and extended in this repository ([`ENGINE_UPSTREAM.md`](ENGINE_UPSTREAM.md) lists every engine change). |
| **Environment** | Claude Code 2.1.302 in an Anthropic-managed cloud container (claude.ai/code remote session launched from the Claude desktop app); Ubuntu 24.04.5 LTS, Linux 6.18 x86_64, 4 vCPUs, 15 GiB RAM, no GPU (Mesa lavapipe for headless rendering); headless Chromium 1194 via Playwright 1.56.1 (WebGPU on SwiftShader) for browser checks; Node 22, Python 3. Targets: the browser (Chrome/Edge with WebGPU, one self-contained HTML file), native Linux and Windows. |
| **Model and Effort Level** | Planning: Anthropic Claude Opus 5.5 (`claude-opus-5-5`), effort max (confirmed from session metadata). Implementation: Anthropic **Claude Opus 5.5** (`claude-opus-5-5`), effort **xhigh** — confirmed with `get_session` (`session_context.model` = `claude-opus-5-5`, `external_metadata.last_served_model` = `claude-opus-5-5`, `effort_level` = `xhigh`). |

The engine evaluation is separate from this block: [`docs/ENGINE_ANALYSIS.md`](docs/ENGINE_ANALYSIS.md).

## How to play

- **Browser:** open
  [`dist/Assassins-Creed-IV-Black-Flag-AssassinsKegBlackTap-W9-2026-10-17-Claude-Opus-5.5.html`](dist/)
  in Chrome or Edge (WebGPU), straight from disk (`file://`) — no server, no other files. A
  gamepad is best (DualSense and Xbox glyphs follow the last device you touched); keyboard and
  mouse work too. On a phone the page plays the attract demo; tap ☰ for the menu and About.
- **Native:** `cargo run --release -p pav_app -- --scene blacktap` (add `--kiosk` for a store
  display that falls back to the demo after 60 s; `--dev` for developer tools).
- **The slice:** six Rounds (*First Round — Fair Winds* to *Last Orders — Tapped Out*), free roam afterwards
  (convoys, Mother Molasses' contract, the tavern), 26 collectibles (shanty sheets, bottles,
  chests, Keg Taps, flotsam), three difficulties, Replay a Round, Continue / New Game, and the
  **Drydock** (*Extras → Training*): a cay with a station for every mechanic.
- Leave the title or menus untouched for 10 seconds and the AI plays the real game in the
  attract mode; any input returns to the title on the same frame.

### Controls (rebindable data: [`games/blacktap/bindings.toml`](games/blacktap/bindings.toml))

**On foot**

| DualSense | Action | Keyboard / mouse |
|---|---|---|
| Left stick / Right stick | Move / camera | W A S D / mouse |
| R1 (hold) | Free-run (high profile) | Shift |
| ✕ | Jump · Parkour Up · climb · Leap of Spirits at a perch | Space |
| ◯ | Drop · Parkour Down · dodge · dive | C |
| □ | Attack · Assassinate (when the prompt shows) | Left mouse |
| △ | Counter (on a tell) · interact · survey at a perch | E |
| L2 (hold) / R2 | Aim pistol or tool / fire or throw | Right mouse / Left mouse while aiming, F |
| L1 (hold) | Tool wheel | Tab |
| R3 | Gull Sight | Q |
| L3 | Crouch / sneak | Ctrl |
| D-pad ↑ / ↓ / ← → | Grog Ration / whistle / previous, next tool | R / G / 1, 2 |
| Touchpad · Options | Map · pause | M · Esc |

**At the helm**

| DualSense | Action | Keyboard / mouse |
|---|---|---|
| Left stick ↑ ↓ / ← → | Sails (furled · half · full) / steer | W S / A D |
| R1 (hold) | Ramming speed | Shift |
| Right stick | Camera — look to a side to choose the weapon | Mouse |
| L2 (hold) / R2 | Aim (arcs) / fire: broadside to the side you look at, chain shot ahead, Keg Drop astern | Right mouse / Left mouse |
| □ | Swivel gun on a highlighted weak point | Left mouse |
| △ | Brace · hold when slow: leave the helm | E |
| ✕ | Board (enemy disabled and close) | Space |
| L1 (hold) | Spyglass (R2 tags) | Z |
| D-pad ↑ / ← → | Repair (Timber) / previous, next shanty | R / ← → |

**Menus:** D-pad or left stick to move, ✕ select, ◯ back, L1/R1 tabs, △ context action
(keyboard: arrows, Enter, Esc, Q/E, Tab; mouse hover and click).

## Building and testing

```sh
scripts/setup-linux.sh && scripts/setup-game.sh   # once: apt packages, lavapipe, wasm32, Playwright
scripts/check.sh                                   # the gate: fmt, clippy -D warnings, tests, wasm clippy
scripts/check.sh --full                            # + each Round on 5 seeds, the full level on 3, 30-min attract
scripts/build-game.sh                              # the single-file HTML into dist/
node scripts/browser_check.mjs dist/<file>.html out/browser   # desktop + phone checks in Chromium
```

Everything is checkable headless with the `pav` tool (`cargo run -p pav_tools --bin pav -- help`):
`load scene=blacktap/round/3`, `play bot=true`, `capture`, `ui screen=pause_map family=xbox`,
`attract minutes=30`, `ship`, `mission goto=round/5@key`, `perception`, `fighters`,
`level_check`, `tour`, `markers`, `ray`, snapshots, rewind and replays. The bot plays every
Round (`cargo test -p blacktap --test rounds -- --ignored`) and the whole Tab
(`--test full_level`). Adding content: [`docs/CONTENT_GUIDE.md`](docs/CONTENT_GUIDE.md).

## Features and systems

- **Sea** ([PLAN §9.6–9.8](PLAN.md)): Gerstner ocean (same waves on CPU and GPU), sailing with
  wind and three sail levels, grounding, ramming; broadsides, chain shot, swivel, Keg Drop with
  aim arcs and lead assist; damage, fire, disabling, sinking, repair and brace; AI gunboats,
  schooner, frigate and convoys; forts with batteries and a telegraphed mortar tower; boarding
  with plunder and upgrades; spyglass tagging, flotsam, waterspouts and a storm.
- **Land** (§9.9–9.13): free-running, wall/pole climbing, corners, beams, ledges, leaps, swimming
  and diving; Crow's Perches with surveys that reveal the map; melee with attack tokens, tells,
  counters and finishers; five assassinations; pistols; darts, Smoke Keg, Bung Line; guards with
  sight, hearing, awareness states, bells, bodies, heat and posters; crowds and stalk grass;
  Gull Sight with through-wall highlights; guards that path round the town (2.5D nav).
- **World** (§5, §9.3–9.5): Isla Templanza (Dryport, Fort Abstinence, jungle, ruins, the
  smugglers' cave), Gull Rock and Hook Key — generated from one data file (islands, 27 structure
  kinds, vegetation); atmosphere presets (golden hour, sunset, storm) with sky, clouds, fog, rain,
  lightning.
- **Presentation** (§9.17, §11): characters with pirate and Temperance gear, data-driven FX and
  sound banks, synthesized shanties (ABC) and a title theme, ambience and voice blips, haptics,
  a 10-foot controller-first UI (every screen, HUD, map, onboarding, accessibility).
- **Attract mode and platforms** (§12): 10 s idle → the AI plays a five-segment showcase;
  phones play the demo with a ☰ menu; a kiosk flag.
- **Engine** (§8, [`ENGINE_UPSTREAM.md`](ENGINE_UPSTREAM.md)): hook v2 play frame, `pav_kit`,
  `pav_ui`, island/structure/vegetation generators, ocean, traversal, nav25, cue banks,
  persistence, culling and LOD.

## Credits

- Engine: Pavilion, from [shardfall](https://github.com/michaelcrosato/shardfall) (vendored at
  `0098196`; changes in [`ENGINE_UPSTREAM.md`](ENGINE_UPSTREAM.md)).
- Fonts: Pirata One and Alegreya Sans, SIL Open Font License (`assets/fonts/*/OFL.txt`).
- Music: traditional sea shanties (public domain), arranged in ABC and synthesized; the title
  theme is original.
- Everything else — terrain, buildings, ships, characters, effects, sound — is procedural.
- *Assassin's Creed IV: Black Flag* is Ubisoft's; this is an original homage with its own names,
  story and code.

## Documents

[`PLAN.md`](PLAN.md) (the build plan and Definition of Done) ·
[`docs/BUILD_LOG.md`](docs/BUILD_LOG.md) (what was built and verified, newest first) ·
[`docs/ENGINE_ANALYSIS.md`](docs/ENGINE_ANALYSIS.md) ·
[`docs/CONTENT_GUIDE.md`](docs/CONTENT_GUIDE.md) ·
[`ENGINE_UPSTREAM.md`](ENGINE_UPSTREAM.md) ·
the engine's own docs: [`docs/ENGINE_README.md`](docs/ENGINE_README.md), [`AGENTS.md`](AGENTS.md).

## Naming

Fifty playful titles were generated from the source's name and one was drawn at random with a
cryptographic RNG, plus a random 2-character ID: *Assassin's Keg: Black Tap* →
`AssassinsKegBlackTap-W9`. The full list is in [`PLAN.md`, Appendix A](PLAN.md#appendix-a-the-50-titles-and-the-pick).
