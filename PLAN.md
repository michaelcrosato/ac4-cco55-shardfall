# Assassin's Keg: Black Tap — Master Plan

**`[AssassinsKegBlackTap-W9]`** · An original spiritual successor to *Assassin's Creed IV: Black Flag*,
built on the Pavilion engine from [shardfall](https://github.com/michaelcrosato/shardfall).

This file is the complete hand-off for the executing agent. It was written in a planning session
that read the engine's docs, ran its tools, built and browser-tested its web build, and had four
explorers survey its code. Everything below is grounded in that survey (file:line pointers are
to the shardfall checkout at commit `0098196`; after vendoring they are the same paths in this
repo). Where the plan says "decide", decide and log it; where it says "must", it is part of the
Definition of Done (§14).

---

## Contents

0. [How to use this plan](#0-how-to-use-this-plan)
1. [Source information](#1-source-information)
2. [Identity and naming](#2-identity-and-naming)
3. [What made the source special, and our pillars](#3-what-made-the-source-special-and-our-pillars)
4. [Fiction, tone and glossary](#4-fiction-tone-and-glossary)
5. [The level: Tab I, "The Dry Run"](#5-the-level-tab-i-the-dry-run)
6. [Systems coverage matrix](#6-systems-coverage-matrix)
7. [The engine as found (planning survey)](#7-the-engine-as-found-planning-survey)
8. [Architecture](#8-architecture)
9. [Subsystem specifications](#9-subsystem-specifications)
10. [Controls](#10-controls)
11. [UI and UX: the 10-foot front end](#11-ui-and-ux-the-10-foot-front-end)
12. [Attract mode, kiosk and mobile](#12-attract-mode-kiosk-and-mobile)
13. [Build phases](#13-build-phases)
14. [Definition of Done](#14-definition-of-done)
15. [Verification and QA](#15-verification-and-qa)
16. [Deliverables and reporting](#16-deliverables-and-reporting)
17. [Risks, scope guards and decisions already made](#17-risks-scope-guards-and-decisions-already-made)
- [Appendix A: the 50 titles and the pick](#appendix-a-the-50-titles-and-the-pick)
- [Appendix B: engine pointers (where to hook)](#appendix-b-engine-pointers-where-to-hook)
- [Appendix C: shanty repertoire and fonts](#appendix-c-shanty-repertoire-and-fonts)
- [Appendix D: preliminary engine analysis (planning view)](#appendix-d-preliminary-engine-analysis-planning-view)

---

## 0. How to use this plan

You are the executing agent (intended: Claude Opus 5.5 at the user's "Extra Effort" setting).
The user will `/clear` and start you with a short `/goal` that points here.

1. **Read this whole file first.** It is long on purpose: it replaces a day of exploration.
   Then read, in the engine (after Phase 0 vendors it): `START_HERE.md`, `docs/MAKE_A_GAME.md`
   (whole), `AGENTS.md` ("Key rules", "Adding a tool"), `docs/DOCTRINE.md`. Open
   `docs/REFERENCE.md` sections only when a step needs them. Don't read `docs/HISTORY.md`,
   `docs/ASSET_SYSTEM_PLAN.md`, `docs/AGENT_ASSET_STORE.md` or `pavilion-lite/`.
2. **Work the phases in order (§13).** Phase 1 builds a *walking skeleton* of the whole product
   (front end, attract loop, game crate, HTML build) before any system gets deep. Every phase
   has exit criteria; don't start the next phase with the current one's exits red.
3. **Keep two files current; they are your memory across context compactions and sessions:**
   - `docs/BUILD_LOG.md`: newest-first dated entries: what was built, how to verify it (the
     exact commands), decisions made and why, open problems. Add an entry at every milestone and
     before any long-running step.
   - The checkboxes in §14 of this file: tick an item only when its evidence exists (a passing
     test, a tool output, a capture you looked at). Put the evidence pointer in BUILD_LOG.
   After any compaction or restart: read §14, then the top of BUILD_LOG, then `git log -20`.
4. **Commit early, push often** to `claude/great-bell-od09bn` (the container is ephemeral).
   Open the draft PR after the first push and keep its description current (phase status,
   how to play, how to verify). Never push to another branch.
5. **Decide, don't ask**, for anything that isn't genuinely the user's call; log the decision in
   BUILD_LOG. This plan's decisions (§17) stand unless building proves one wrong; then change it,
   log why, and update this file. The user is not watching live; don't stall on questions.
6. **Doctrine and engine rules apply** (AGENTS.md "Key rules"): `pav_core` stays free of
   graphics/window/audio crates; all play state is `Clone` + serde; same inputs and seed give
   the same state (no clock, no unseeded randomness, no `HashMap` iteration into state); no
   threads, blocking calls or file access reachable on wasm32; **no binary assets except fonts**
   (OFL, with licence files) and the final HTML build in `dist/`. Everything else is procedural
   code or readable text/data.
7. **Check with tools, not by reading engine code** (doctrine 3, "Check selectively"): `pav`
   captures, `play bot=true seeds=N`, filmstrips, `see`, UI captures, audio captures, browser
   screenshots. Look at every capture you take (small sizes: 640×360 or 960×540). Use the `pav`
   CLI/REPL directly (`./target/debug/pav …`): the engine's `.mcp.json` server only loads in a
   new session after vendoring.
8. **Use subagents for parallel, well-fenced work** (Agent tool, `isolation: "worktree"` when
   they edit code): e.g. the music sequencer, the UI kit, the ocean shader, the ship mesh
   generator. Give each one crate or module to own, a written interface, and tests to pass;
   review and integrate their diffs yourself. Don't run two agents on the same files.
9. **Compile-time hygiene** (4 cores, ~2 min per `pav` rebuild): batch edits, use
   `cargo check -p <crate>` while iterating, keep balance numbers in data (TOML) so they reload
   without a rebuild, and keep game code out of `pav_core` (it has its own crate, §8.2). Watch
   the disk (fixed allowance): delete stale `target/` artifacts if it fills
   (`cargo clean -p <crate>`), never commit `target/`.
10. **Install whatever you need** (the user authorised it): apt packages, rustup targets and
    components, cargo tools, npm packages, fonts. Record every install in BUILD_LOG and add it
    to `.claude/hooks/session-start.sh` (or a `scripts/setup-*.sh` it calls) so a fresh
    container reproduces the environment.
11. **Finish properly** (§16): tests, clippy and fmt green; the single-file HTML built and
    browser-checked (desktop and phone emulation); README, engine analysis and content guide
    written; BUILD_LOG final; PR updated; the final chat message carries the Source Information
    block and the engine analysis.

---

## 1. Source information

Keep this block, updated, in the README (and the HTML's about/credits screen), separate from the
engine evaluation.

| | |
|---|---|
| **Initial Date** | 2026-10-17 (the date of the first prompt, from the session clock). |
| **SOURCE** | *Assassin's Creed IV: Black Flag* (Ubisoft Montreal, 2013; PS3, PS4, Xbox 360, Xbox One, Wii U, PC). |
| **GAME ENGINE** | Pavilion, the engine of **shardfall**: https://github.com/michaelcrosato/shardfall at commit `0098196` ("Co-op plan on hold (#74)"), workspace version 0.1.0. Rust 1.98.1 (edition 2024), wgpu 30 (WebGPU; Vulkan/DX12 natively), rapier3d 0.36, egui 0.36, winit 0.30.13, gilrs 0.11, cpal 0.18, glam 0.33, wasm-bindgen 0.2.129. Vendored into this repository and extended here. |
| **Environment** | Claude Code 2.1.302 in an Anthropic-managed cloud container (a claude.ai/code remote session launched from the Claude desktop app). Ubuntu 24.04.5 LTS, Linux 6.18 x86_64, 4 vCPUs, 15 GiB RAM, no GPU: Mesa lavapipe (software Vulkan) for headless captures, headless Chromium 1194 via Playwright 1.56.1 (WebGPU on SwiftShader) for browser checks, Node 22, Python 3. Targets: native Linux (dev), Windows `.exe` (cross-compiled, optional), and the browser (Chrome/Edge with WebGPU) as one self-contained HTML file. |
| **Model and Effort Level** | Planning session: Anthropic **Claude Opus 5.5** (`claude-opus-5-5`), effort **max**, confirmed from the session's metadata (`get_session`). Execution session: the user intends Opus 5.5 at "Extra Effort"; **the executor must confirm its own model and effort with `get_session` (claude-code-remote MCP; `session_context.model`, `external_metadata.last_served_model`, `effort_level`) and record exactly what it reports, never a guess.** |

---

## 2. Identity and naming

- **Title pick.** Fifty playful riffs on the source's name were generated (Appendix A; every
  one alters the trademark rather than using it verbatim), one was drawn with Python's
  `secrets` CSPRNG, and a 2-character alphanumeric ID was drawn the same way:
  **#45 "Assassin's Keg: Black Tap" → `[AssassinsKegBlackTap-W9]`**. This is final.
- **Display title:** *Assassin's Keg: Black Tap*. Short: *Assassin's Keg*. Tagline: *Black Tap*.
- **Unique ID:** `AssassinsKegBlackTap-W9` (use it in the README header, the HTML filename, the
  about screen and the PR title).
- **HTML `<title>`:** `Assassin's Keg — Black Tap` (the ai-hub wall reads "Name — Tagline"), with
  `<meta name="description">` (one sentence), `<meta name="theme-color" content="#0f3b4c">`
  (deep teal), and an inline SVG favicon of a brass keg tap (add an `--icon` option to
  `scripts/pack_single_html.py`).
- **HTML filename** (ai-hub convention `Source-Title-date-model`):
  `dist/Assassins-Creed-IV-Black-Flag-AssassinsKegBlackTap-W9-<build date YYYY-MM-DD>-<model>.html`,
  e.g. `...-2026-10-17-Claude-Opus-5.5.html`, with `<model>` from what `get_session` reports.
  It must be ready to drop into `michaelcrosato/ai-hub/games/` (don't push to ai-hub: not in
  this task's scope; the user adds it).
- **Keys:** game/scene key `blacktap` (`load scene=blacktap`, `?scene=blacktap`); developer
  level `blacktap/drydock`; crate `crates/blacktap`; data folder `games/blacktap/`.
- **Branding:** window title, boot log, error boxes, save/pref file names and localStorage keys
  say Assassin's Keg (`blacktap_*`), not Shardfall, when the game is the start scene. Shardfall
  and the pavilion stay in the build as the engine's demos, reachable only in developer mode.

---

## 3. What made the source special, and our pillars

### 3.1 What *Black Flag* did that players remember
1. **No seams between sea and land.** Sail your own ship, leave the wheel, dive off the bow, swim
   ashore, climb a tower, come back, take the helm — no loading, no menus.
2. **Naval combat with weight.** Broadsides aimed by looking to a side, chain shot ahead, fire
   barrels astern, mortars, swivel guns on weak points, bracing, ramming, boarding by grapple,
   and forts taken from the sea and then on foot.
3. **The pirate fantasy.** A ship that is home and progression (upgrades from plunder), a crew
   that sings shanties while you sail, a Caribbean of islands, wrecks, sandbars and treasure.
4. **The Assassin core.** Rooftop and treetop parkour, environmental stealth (bushes), air and
   ledge assassinations, Eagle Vision, viewpoints that synchronize the map and end in a leap of
   faith, a hidden blade.
5. **Atmosphere.** Turquoise shallows, golden sunsets, storms with rogue waves and waterspouts,
   towns with flat roofs and laundry lines, jungle ruins.
6. **Structure.** Short "memories" with optional objectives for "full synchronization", and an
   open world of collectibles between them.

### 3.2 Pillars for *Assassin's Keg*
| # | Pillar | Means in this slice |
|---|---|---|
| P1 | **Sail, swim, climb, strike — no seams** | One continuous level (sea + islands); the ship is the hub; helm ⇄ deck ⇄ sea ⇄ shore ⇄ rooftops without loads. |
| P2 | **Broadside & Board** | Readable, punchy naval combat with clear arcs, telegraphs and a boarding melee. |
| P3 | **Hidden in the grog** | Systemic stealth with legible feedback: awareness arcs, hiding spots, search states, heat. |
| P4 | **Every rooftop a road** | Intent-driven free-running and climbing that rarely does what you didn't mean. |
| P5 | **A song on every wave** | Lush procedural Caribbean (ocean, sky, weather) and procedural crew shanties. |
| P6 | **Played straight, named crooked** | Puns in names and barks; presentation, feel and atmosphere taken seriously. |

### 3.3 Modernize and improve (what we change on purpose)
| In the source | In *Assassin's Keg* |
|---|---|
| Sticky, accidental parkour (scaling walls you meant to walk past) | Explicit **Parkour Up / Parkour Down** intent (free-run + ✕ / free-run + ◯), no auto-climb at walking pace, ledge safety (you don't step off a roof unless you push off it deliberately). |
| Instant "desync" for leaving mission areas or failing tails | Soft boundaries with a warning; "Spilled" (fail) only for story-critical failures; instant reload from snapshot checkpoints. Eavesdrop and tailing missions are cut. |
| Opaque detection | Per-guard awareness arcs over heads, off-screen threat pips, last-known-position ghost, distinct states (Suspicious → Searching → Alerted → Combat → Lost). |
| Counter-spam combat | Telegraphed attacks with a tight counter window; archetypes that need different answers (brute: guard-break; musketeer: close in or break line of sight; officer: feints). |
| Broadside aim friction | The camera swings to the side you look at, predictive arcs with a lead marker, adjustable aim assist. |
| Mortar damage from nowhere | Every incoming mortar and broadside is telegraphed (red impact rings / glowing gunports) with time to react. |
| Dense menus and HUD | 10-foot UI: big tiles, ≤ 7 items per list, minimal auto-hiding HUD, contextual prompts only. |
| Accessibility gaps | Subtitles on by default with speaker colours, text size, colour-blind Gull Sight palettes, hold/toggle choices, invert and sensitivity, aim assist, camera shake and motion sliders, three difficulties. |

---

## 4. Fiction, tone and glossary

**Setting.** 1717, the Saltwind Isles: a fictional Caribbean archipelago at the height of the
golden age of piracy.

**The Brotherhood of the Keg** ("Kegsmen"): smugglers and free sailors who guard the common
hand's liberty, which they toast in rum. Creed: *"Nothing is watered down; everything is on
tap."* Tenets: stay your blade from the barkeep (innocents); hide in plain sight, preferably
behind a barrel; never give up the Keg. Weapon: the **Tap Blade**, a hidden blade that springs
from a brass spigot bracer.

**The Temperance Order**: governors and admirals who would dry out the isles: outlaw rum, seize
every still and rule through scarcity. Motto: *"Order through Abstinence."* Symbol: an upturned
cup in a laurel (deliberately unlike any real order's cross).

**Cast.**
- **Captain Rowan Cask** (the player; they/them in docs): master of the brig *Black Tap*.
  Hooded rum-red and charcoal greatcoat with a beaked hood, brass Tap Blade bracer, cutlass,
  two flintlocks.
- **"Old Bung" Abernathy**, quartermaster: tutorial voice, leads the shanties.
- **Pip Carrow**, navigator: calls the wind and the targets.
- **Ballast**, the ship's parrot: one-word barks on the HUD's subtitle line ("BRACE!").
- **Commodore Josiah Drye**, the target: master of Fort Abstinence.
- **Captain Sobers**, Drye's lieutenant, commands the schooner *TS Restraint*.
- **Mother Molasses**, the Brotherhood's contact, taken and locked in the stocks of Dryport.

**Tone.** The names, barks, collectibles and menus are puns; the visuals, music, mechanics and
pacing are played straight and beautiful. Family-friendly (alcohol is the running joke, never
the point); no crude humour; no real-world trademarks.

**Glossary (source term → ours).** Use these names in code identifiers' comments, UI strings
and docs consistently.

| Source | *Assassin's Keg* |
|---|---|
| Sequence / Memory | **Tab** / **Round** (the last one is **Last Orders**) |
| Synchronization % / Full sync | **Pour %** / **Full Pour** |
| Desynchronized | **Spilled** |
| Eagle Vision | **Gull Sight** |
| Viewpoint / synchronize | **Crow's Perch** / **Survey** |
| Leap of Faith | **Leap of Spirits** (into hay carts or deep water) |
| Hidden Blade | **Tap Blade** |
| Notoriety | **Heat** (wanted posters, town criers) |
| Animus fragments / sea shanties / chests | **Message Bottles** / **Shanty Sheets** / **Treasure Chests** |
| Assassin bureau caches | **Keg Taps** (hidden Brotherhood caches) |
| Smoke bomb / sleep dart / berserk dart / rope dart | **Smoke Keg** / **Nightcap Dart** / **Grog Dart** (the guard stumbles and swings at his friends) / **Bung Line** |
| Fire barrels (stern weapon) | **Keg Drop** |
| Templars / Assassins | **Temperance Order** / **Brotherhood of the Keg** |
| Fort capture | **Last Call** at the fort (raise the Tap flag) |
| Health potion | **Grog Ration** |

---

## 5. The level: Tab I, "The Dry Run"

One continuous level: a sea with three islands. It must take a first-time player about 30
minutes through six Rounds, then stay open as a free-roam sandbox. It doubles as the engine's
proving ground: every system in §6 is exercised here.

### 5.1 World layout
Coordinates: x east, z south, metres, origin at the centre of the play space. Sea level y = 0.

```
                                N (-z)
  (-450,-300) EXTRACTION, open sea NW            storm front rolls in from the east (R6)

        (-120,-160) GULL ROCK: lighthouse,                (330,-190) FORT ABSTINENCE
                    Crow's Perch #1                       W battery (270,-200), N battery
                                                          (340,-250), mortar tower (380,-160),
                    (110,-150) west cove                  keep + bell tower, flagpole
                    (quiet anchorage, chest)
                                    ISLA TEMPLANZA (centre ~(220,-80), ~320 x 220 m):
                                    jungle, ruins, stalk grass, road to the fort gate,
                                    Smugglers' Cave (300,-120) -> fort cellar tunnel

  (-250,120) HOOK KEY:              (100,10) SW beach      (180,20) DRYPORT: docks, plaza,
  sandbar, wreck, chest             = Round 4 landing      stocks, chapel tower (Crow's Perch #2)

  (-300,250) flotsam field          patrol lanes: gunboats + schooner ~(-150..150, 50..250)

  (-420,380) START: the Black Tap at the helm, heading NE, golden hour
                                S (+z)
```
- **Play space:** 1,200 m × 1,200 m of sea. Soft boundary at 560 m from the centre ("The trade
  winds won't carry the *Black Tap* further"; the ship is turned gently back), hard clamp at
  600 m. Beyond: ocean to a fogged horizon.
- **Isla Templanza** (~320 m × 220 m, heightmap terrain to ~40 m):
  - **Fort Abstinence** on the north-east headland on 12–18 m cliffs: 8 m curtain walls, four
    bastions, the **West Battery** (270,−200), the **North Battery** (340,−250), the **Mortar
    Tower** (380,−160), a keep with the Commodore's balcony and a **bell tower** (alarm), a
    courtyard, an armory and the flagpole.
    Three ways in: the north **sea-cliff climb** onto the wall walk; the **Smugglers' Cave**
    tunnel into the cellar (needs the cellar key); the **main gate** off the jungle road
    (heavily guarded; distraction tools).
  - **Dryport**, the town in the south bay (~120 m × 80 m): a stone pier and docks with a
    moored Temperance sloop, harbour wall, ~24 Spanish-colonial buildings of 2–3 storeys with
    **flat roofs and parapets** (runnable), balconies, beams, awnings (they break falls),
    laundry lines between roofs (tightropes), the **Plaza de la Sobriedad** with market stalls,
    palms, the town crier and **the stocks** (Mother Molasses), the guarded **Bonded Rum
    Warehouse**, **Temperance Hall**, the boarded-up tavern **The Last Drop**, and the **Chapel
    of St. Ascetic** with a 24 m bell tower (**Crow's Perch #2**) and a hay cart below it.
  - **Jungle interior:** palm forest, ferns, **stalk-grass** patches (hiding), a creek, a road
    from town to the fort gate with patrols, two guard camps, a ruined watchtower, and three
    **Keg Tap** caches in ruins.
  - **Beaches and coves:** the south-west beach below Dryport (the Round 4 landing), south
    beaches by town, and the west cove (a quiet anchorage with a chest).
- **Gull Rock**: a sea stack with a lighthouse (28 m), **Crow's Perch #1**, climbable from the
  water; Leap of Spirits into deep water.
- **Hook Key**: a sand cay with palms, a half-buried wreck, a chest and a Shanty Sheet.
- **Sea:** flotsam field, patrol lanes (two gunboats, the schooner), shallows (turquoise) around
  every island, deep water (sapphire) elsewhere; the storm strait between Isla Templanza and the
  extraction point for the finale (waterspouts).

### 5.2 Collectibles and economy (light)
| Kind | Count | Where | Reward |
|---|---|---|---|
| Shanty Sheets | 4 | Hook Key, a Dryport rooftop, a fort battlement, floating off Gull Rock | A new crew shanty in the rotation (and the jukebox) |
| Message Bottles | 5 | Beaches and rooftops | One lore card each (short, funny) |
| Treasure Chests | 8 | 3 at sea (wreck, cay, rock), 5 on land | Reales (currency) |
| Keg Taps | 3 | Jungle ruins, Smugglers' Cave, chapel crypt | Lore + a Tap Blade upgrade token (unlocks the double assassination) |
| Flotsam | 6 | At sea | Timber and Iron (ship repair and upgrades) |

Upgrades are a tiny tree, chosen on big tiles after plunder (§9.12): *Reinforced Hull*,
*Heavy Shot*, *Swift Rigging*, *More Kegs*. Reales buy one upgrade at the Brotherhood
quartermaster in free roam. No other economy.

### 5.3 Rounds (beat sheets)
Each Round lists: objectives → systems taught or exercised → optional *Full Pour* objectives →
checkpoints. Times are for a first-time player.

**First Round — "Fair Winds"** (~3 min, sea)
- Start at the helm, golden hour, the crew mid-shanty. Old Bung's prompts: raise sails, steer,
  read the wind (into the wind is slow, a beam reach is fastest). Collect three flotsam crates by
  sailing through them. Spyglass-tag the *TS Restraint*. Gull Sight at sea shows ship class and
  cargo.
- Systems: sailing model, wind, sail levels, camera at the helm, flotsam pickup, spyglass,
  Gull Sight (sea), shanty music, onboarding prompts, objective markers.
- Full Pour: collect all three flotsam within 2:00.
- Checkpoint: start.

**Second Round — "Broadside"** (~5 min, sea → boarding)
- Two gunboats intercept: broadsides (look to a side, aim, fire), chain shot (look ahead), swivel
  gun on glowing weak points, brace when an enemy's gunports glow. Sink or disable them. Then the
  *TS Restraint*: disable it (hull under 25%), close in, **Board** (prompt) → grapple cinematic →
  deck fight (six crew, an officer, Captain Sobers) → victory → plunder (repair) and one upgrade
  pick on tiles.
- Systems: cannons and ballistics, damage and sinking, enemy captain AI, brace, boarding
  transition, melee (attacks, counters, Tap Blade kills), pistols, crew allies on deck, loot,
  upgrade UI.
- Full Pour: disable the schooner with chain shot; three counter-kills while boarding.
- Checkpoints: start of Round; before boarding.

**Third Round — "Batteries Not Included"** (~5 min, sea vs fort)
- Approach Fort Abstinence. The batteries fire broadsides; the Mortar Tower lobs shells with red
  impact rings. Destroy both batteries and the tower (health bars appear when targeted). The guns
  fall silent: "Take her by land!"
- Systems: static gun emplacements, telegraphed area attacks, threat ranges, fort health UI,
  heavy shot (if upgraded), music intensity.
- Full Pour: destroy the Mortar Tower first; keep the hull above 50%.
- Checkpoint: start of Round.

**Fourth Round — "Ashore"** (~4 min, sea → land)
- Sail round to the south bay, furl sails off the south-west beach, leave the helm, dive off the
  bow, swim ashore, run up into Dryport. Climb the chapel tower (free-climb tutorial: Parkour
  Up / Parkour Down, ledges, corners), **Survey** (a 360°
  pan; the map reveals collectibles, fort routes, guard posts), **Leap of Spirits** into the hay
  cart.
- Systems: helm ⇄ deck ⇄ water transitions, swimming in waves, climbing (walls, beams, corners,
  mantle), Crow's Perch survey with map reveal, leap of spirits, fall damage, map screen.
- Full Pour: also survey Gull Rock; reach the top of the chapel within 60 s of landing.
- Checkpoints: landing; after the survey.

**Fifth Round — "Under the Influence"** (~6 min, land stealth)
- A courier with two escorts walks a route from Temperance Hall to the docks carrying the cellar
  key. Get the key (pickpocket from behind or assassinate). Free Mother Molasses from the stocks
  (three guards; distract with a whistle or Grog Dart, or fight). Heat rises with crimes seen;
  tear down wanted posters to lower it.
- Systems: perception (sight, hearing, hiding in stalk grass, bodies), guard states and search,
  civilians and crowds, Gull Sight marking, tools (whistle, Nightcap and Grog darts, Smoke Keg),
  assassinations (ground, air, from hiding, from a ledge), pickpocket, heat and posters, dialog
  and subtitles.
- Full Pour: never detected; air-assassinate the courier; free Mother Molasses without a kill.
- Checkpoints: start; key obtained.

**Last Orders — "Tapped Out"** (~7 min, fort infiltration → escape)
- Enter Fort Abstinence by any of the three routes (restricted area: awareness fills faster).
  Find Commodore Drye with Gull Sight (gold silhouette), on the keep balcony with a brute and a
  musketeer; if alarmed he runs for the bell tower (a short chase). Assassinate → **Last Words**
  (a slow-motion scene in white sea-mist: a short exchange of lines with subtitles) → raise the
  Tap flag (**Last Call**: "FORT CAPTURED") → Heat 3, alarm bells, reinforcements → dive from the
  battlement into the sea (the crew has brought the *Black Tap* under the cliffs) → take the helm
  → the storm rolls in over 60 s, the frigate *TS Abstemious* gives chase, waterspouts cross the
  strait → reach the open-sea marker → **TAB COMPLETE**.
- Systems: restricted areas, alarms and reinforcements, target behaviour (flee), cinematic camera
  and scripted scenes, flag interaction, weather transition (sky, sea state, rain, lightning,
  wind), hazards at sea (waterspouts), pursuit AI, results screen.
- Full Pour: assassinate Drye from the air or from hiding; reach him undetected; no hull damage
  during the escape.
- Checkpoints: start; after the assassination; at the helm for the escape.

**Results → free roam.** Results screen: Pour % (main path 70% + optional objectives), time,
reales, collectibles, assassinations, ships sunk, times detected; tiles: *Continue (Free Roam)*,
*Replay a Round*, *Title*. In free roam the fort flies the Tap flag, patrols thin out, a
Temperance convoy spawns at sea every few minutes (systemic naval encounter), remaining
collectibles stay, and Mother Molasses at the reopened tavern offers a **contract**: assassinate
a tax collector whose patrol route is chosen from the town's routes at random (a small
procedural contract, proving that content scales by data).

### 5.4 Pacing curve
Sea calm (R1) → sea combat (R2) → siege (R3, peak 1) → quiet arrival and vista (R4, breather
and wonder) → tension (R5, stealth) → climax (R6: infiltration → assassination → chaos →
storm escape, peak 2) → release (results at dawn-after-storm light, free roam).

---

## 6. Systems coverage matrix

Every system appears in the level, in the developer level **The Drydock** (§9.19), in at least
one attract-mode segment (§12) where it shows well, and in tests (§15).

| System | Rounds | Drydock station | Attract segment | Tests (examples) |
|---|---|---|---|---|
| Front end: title, menus, pause, settings, results | all | UI Gallery | title cards | focus graph, screens render, settings persist |
| Attract mode / kiosk / mobile | — | — | all | idle → demo at 10 s, any input exits, loop, watchdog |
| Game camera director | all | Camera Bench | all | collision keeps eye clear, blends, cinematic shots |
| Ocean, waves, buoyancy | R1–R4, R6 | Sea State pads | Rule the Waves | CPU/GPU wave parity, floating crates |
| Sky, time of day, weather | R6 (storm), title | Weather pads | Last Call | transitions are smooth and deterministic |
| Island terrain, vegetation | R4–R6 | Terrain patch | Ashore | heightfield collider matches mesh, nav on slopes |
| Structures (buildings, fort, docks) | R4–R6 | Prefab row | Shadow of the Keg | generator deterministic, climb surfaces flagged |
| Ship sailing (wind, sails, turning, grounding) | R1–R3, R6 | Sailing lane | Rule the Waves | speed vs wind angle, grounding damage |
| Naval weapons (broadside, chain, swivel, kegs) | R2, R3, R6 | Gunnery range | Rule the Waves | ballistics hit at range, chain slows, kegs explode |
| Ship damage, sinking, AI captains | R2, R6 | Duel buoy | Rule the Waves | AI keeps broadside angle, disables then flees |
| Forts (batteries, mortars, telegraphs) | R3 | Mortar yard | Storm the Fort | telegraph precedes impact by ≥ 1.2 s |
| Boarding | R2 | Boarding hulk | Rule the Waves | transition, arena, victory → loot |
| Helm ⇄ deck ⇄ water transitions | R4, R6 | Pier | Ashore | leave helm only when slow, ladder back aboard |
| Swimming and diving in waves | R4, R6 | Pier | Ashore | surface follows waves, climb out on ladders |
| Free-run, climb, ledges, corners, mantle | R4–R6 | Climbing wall | Ashore, Shadow of the Keg | climb a 6 m wall, wrap a corner, no accidental climb |
| Leap of Spirits, fall damage, rolls | R4, R6 | Haystack tower | Ashore | safe landing zones, damage curve |
| Ropes and tightropes | R5 | Rope run | Shadow of the Keg | balance and slide |
| Melee, counters, assassinations | R2, R5, R6 | Combat ring | Shadow of the Keg | counter window timing, assassination validity |
| Pistols and tools (darts, Smoke Keg, Bung Line) | R2, R5, R6 | Tool range | Shadow of the Keg | darts change AI state, smoke blinds |
| Perception and stealth | R5, R6 | Stealth yard | Shadow of the Keg | sees in open, not in grass, LOS blocked |
| Guard AI states, search, alarms | R5, R6 | Stealth yard | Shadow of the Keg | state transitions, alarm calls reinforcements |
| Civilians and crowds | R5 | Market | Shadow of the Keg | flee from combat, blend |
| Heat (notoriety) | R5, R6 | Poster wall | Last Call | rises with witnessed crimes, falls with posters |
| Gull Sight | R1, R5, R6 | every station | Shadow of the Keg | highlights through walls, colour-blind palettes |
| Missions, objectives, checkpoints | all | Mission board | all | graph valid, every Round completable, reload |
| Dialog, subtitles, cinematics | R2, R5, R6 | Theatre | Last Call | lines timed, skippable, camera shots |
| Collectibles, survey, map reveal | R1, R4, free roam | Map table | Ashore | counts persist, reveal radius |
| Upgrades | R2, free roam | Shipwright | — | effects apply to stats |
| Audio: SFX bank, ambience, positional | all | Jukebox | all | bank parses, events map, no silence |
| Music: shanties, adaptive stems | all | Jukebox | all | sequencer renders, transitions on state |
| Haptics (rumble) | all | Rumble pads | — | events map to patterns |
| Persistence (settings, progress) | all | — | — | round trip native and web |
| Agent tools for all of the above | — | — | — | each new tool has a test |

---

## 7. The engine as found (planning survey)

Pavilion is a deterministic, headless-first Rust engine built for agents: a CPU simulation at
60 ticks/s (`pav_core`, rapier physics), a WebGPU renderer (`pav_render` + `pav_view`), a small
synth (`pav_audio`), an agent tool layer (`pav_tools`: the `pav` CLI/REPL/MCP), and the window
and browser app (`pav_app`, egui UI). A new game implements the `pav_core::games::Game` hook
(`pre`, `post`, `hud`, `status`, `outcome`, `bot`) with its level as a room file. Measured here:
the starter's bot sweep runs in 0.4 s; a 640×360 lavapipe capture takes 0.7–8 s; a dist wasm
build takes ~3 min and packs into an 11 MB single HTML that boots in headless Chromium in under
1 s (WebGPU on SwiftShader renders ~0.5 fps, enough for screenshots).

### 7.1 What we reuse (as is or nearly)
| Area | What exists | Pointer |
|---|---|---|
| Determinism, rewind, snapshots, replays | `Sim::step`, history every 20 ticks, CBOR snapshots, `state_hash`, replay files | `sim.rs:488-727`, `history.rs` |
| Hook games + bot + tests | `Game` trait, `GameBox` serde, `run_bot`, `pav play bot=true seeds=N`, `tests/games.rs` pattern | `games/mod.rs`, `pav_tools/src/play_tools.rs` |
| Character controller | Instant/momentum/committed/grid models, jump (buffer, coyote, variable), crouch/crawl, ladders, ledge hang/shimmy/mantle (`movement.ledge_grab`), swimming in water zones, knockback, stun, dash, moving platforms (kinematic) | `character.rs:299-1281` |
| Puppets and animation | Biped and 5 other body plans, procedural walk/IK, gear (helm incl. hood, cape, belt, gloves, pauldrons), held weapons/props by socket, 31 moves as data, 437 embedded clips (climb, ledge hang, swim, sword, block, pistol aim/shoot/reload, sneak, deaths, rail leaning, drinking, dancing, sitting...), 4,770 CMU takes on disk | `puppet.rs`, `moves.rs`, `clips.rs`, `anim/` |
| Physics | rapier3d 0.36; static block colliders with tags; kinematic capsules; dynamic props; joints, chains, soft bodies | `physics.rs`, `statics.rs`, `sim.rs:279-306` |
| Levels as data | Room files: ASCII layers + legend (blocks, ladders, props, zones, markers, labels, grass), `[[object]]`, `[[npc]]`, `[camera]`, `[view]`, `[params]`, `[game]`; hot reload; `room_check` | `rooms/_template.toml`, `room.rs`, `level.rs` |
| Rendering | Sun + 64 point lights (4 shadowed), shadows, MSAA, bloom, SSGI, ACES, outlines, cel/flat/lit, per-part styles, Kuwahara oil paint, halftone, grading, haze + volumetric shafts + halos, distortion, screen transitions, GPU particles (13 presets), SDF spheres/capsules, procedural materials (planks, brick, cobbles, shingles, rough rock, wrought iron, stripes...), SDF world text | `pav_render/`, `pav_view/build.rs`, `looks.toml`, `fx.rs` |
| Props | Format-2 props of named primitive parts (box, sphere, capsule, cylinder/cone with arcs, tube, torus, wedge, lathe, extrude) with sockets; LIB/core has 59 (barrel, cask, tap_barrel, crates, sacks, chest, rowboat, market stalls, lanterns, torches, chains, cage, table, bench, rocks, rubble, broken column, hay bale, gate, fence, well...) | `assets/props/`, `docs/ASSET_STUDIO.md` |
| Tools | capture, filmstrip, see, camera_bench, look bench, input, play, bench, record/replay, rewind, snapshot, room_check, levelmap (Shardfall), audio_capture, MCP + live bridge | `pav help`, `docs/REFERENCE.md` |
| Builds | Native Linux, Windows cross-compile, browser (wasm + WebGPU), `--single-file` HTML with `--title/--description/--theme-color/--scene` | `scripts/build-web.sh`, `scripts/pack_single_html.py` |

### 7.2 Gaps this game must fill (engine work)
| Gap | Today | Verdict |
|---|---|---|
| Game crate outside `pav_core` | Games compile inside `pav_core::games` (static `GAMES`) | **Extend**: runtime registration, separate crate |
| Game-driven camera | Focus is always the player; only room `[camera]`, pad cues and user controls; no collision; `front_cut` deletes geometry under 30° tilt; right stick is aim, not orbit | **Extend**: `Game::camera` hint, `InputFrame.look`, spring arm, cutaway rules |
| HUD and menus | Hook games get text lines only; pad menus use a virtual cursor; default egui theme and fonts; no title, no attract, no release gating; engine demo text everywhere | **Build**: `pav_ui` 10-foot kit, front-end state machine, attract, kiosk/dev gating |
| Drawing from game state | No hook; `emit_asset` only inside the view; `RenderFrame.game` is Shardfall-only | **Extend**: `Game::draw` → draw list in the frame |
| Runtime view/effects | Workaround via `courses.view` + `view_serial`; hit-stop, shake, tint, transitions are Shardfall-only | **Extend**: `Game::atmosphere`, `Game::effects` |
| Sky | Flat clear colour; fog by horizontal distance; sun colour hard-coded | **Build**: sky pass (gradient, sun, clouds), sun/ambient colours, view-ray fog |
| Ocean | Ripple pools ≤160×160 cells, opaque, no swell, not deterministic | **Build**: deterministic Gerstner function in `pav_core` + LOD ocean renderer |
| Weather | Rain particle preset only; wind moves only grass/leaves | **Build**: weather state, camera-following rain, lightning, wind on particles/sails |
| Islands | Terraced world terrain only; `TerrainPatch` + `set_terrain` accept any mesh | **Build**: heightmap island generator + material shading + vegetation scatter |
| Buildings, fort, docks, palms, cannons | Not in the library | **Build**: parametric structure generators + new props/families |
| Ships | Car (dynamic raycast vehicle) and arcade helicopter; vehicles ignore game input overrides; dynamic bodies can't carry walkers | **Build**: kinematic ships in the gameplay kit |
| Free-climb, free-run, leap, fall damage | Ladders and ledges only; SPRINT bit unused | **Extend** `character.rs` |
| Swimming in waves | Surface = zone top (flat) | **Extend**: water surface provider |
| Health, death, melee, counters, assassinations | No character health in core; Shardfall's combat is welded to its data | **Build** in the kit (reuse moves, clips, `hit_character`, puppet topple) |
| Projectiles for cannons | Stop only at fixed colliders, no impact events, no water test | **Build** kit ballistics |
| Perception and AI | One-cone guards that reset you on "SPOTTED"; brains walk straight lines; nav is one floor at y=0 | **Build** perception + brains; **extend** nav to 2.5D layers |
| Missions, dialog, cinematics | None | **Build** in the kit |
| Zone events for games | Zones update after `post` (events lost) | **Fix** order / deliver to the game |
| Bombs on PRIMARY/USE | Every character throws bombs unless input is masked | **Fix**: `bombs.weapon = "none"` |
| Sound and music | Closed event → patch match; no loops, LFO, reverb, buses or music | **Extend** `pav_audio`: data bank, voices, sequencer, ambience |
| Custom events | 39 fixed `SimEvent`s | **Extend**: `SimEvent::Cue` (sound + FX by name) |
| Persistence for games | Only Shardfall's hero save | **Build** a key-value store API |
| PlayStation glyphs, rumble, analog triggers | Xbox-named text glyphs; no force feedback; triggers are digital at 0.35 | **Extend** input + glyph painter + haptics |
| Rendering scale | No frustum culling or LOD; far plane ≤ ~300 m by default; shadows ≤ 120 m | **Extend**: culling, LOD, far plane, shadow follow |
| Tunables for games | `[game]` table only | **Extend**: `Game::tunables` |

---

## 8. Architecture

### 8.1 Repository layout (after Phase 0)
The engine is vendored at the repository root, so every engine script, test and doc path works
unchanged (tools expect to run from the root).

```
/                               vendored shardfall @ 0098196 (tracked files minus pavilion-lite/)
├── PLAN.md                     this plan (keep; tick §14 as you go)
├── README.md                   the game's README (engine README moved to docs/ENGINE_README.md)
├── ENGINE_UPSTREAM.md          source URL + commit + every local engine change (kept current)
├── AGENTS.md, CLAUDE.md        engine rules + a short "This repository" header (game first)
├── crates/
│   ├── pav_core/               engine sim (extended: registry, hook v2, ocean fn, climb, nav 2.5D…)
│   ├── pav_render/             renderer (extended: sky, ocean, terrain shading, culling/LOD…)
│   ├── pav_view/               frame → scene (extended: camera hint, draw list, weather, FX cues…)
│   ├── pav_audio/              synth (extended: data bank, voices, sequencer, ambience, reverb)
│   ├── pav_ui/        NEW      10-foot UI kit on egui (no winit/wgpu): theme, focus, screens, HUD
│   ├── pav_kit/       NEW      gameplay kit: vitals, melee, ranged, perception, brains, naval,
│   │                           ballistics, mission, dialog, cinematic, heat, inventory, demo
│   ├── blacktap/      NEW      the game: rules glue, content loading, bot, demo playlist, UI model
│   ├── pav_tools/              agent tools (extended: ocean, ship, mission, perception, ui, attract, tour)
│   └── pav_app/                window/browser app (extended: front end, attract, dev gating, haptics)
├── games/blacktap/             all game data (TOML, ABC), embedded by blacktap/build.rs, hot-reloaded
├── assets/fonts/               OFL fonts + OFL.txt (the only new binary files)
├── dist/                       the final single-file HTML (committed)
└── docs/  BUILD_LOG.md · ENGINE_ANALYSIS.md · CONTENT_GUIDE.md · ENGINE_README.md · (engine docs)
```

**Vendoring procedure (Phase 0):** clone `https://github.com/michaelcrosato/shardfall` (public;
`GIT_LFS_SKIP_SMUDGE=1 git clone --depth 1`), check out `0098196`, copy
`git ls-files -- . ':!:pavilion-lite'` into this repo's root (the same file set as
`scripts/package-agent.sh`), keep our `README.md` content (move the engine's to
`docs/ENGINE_README.md`), merge `.gitignore`s, then `repo-setup`-style setup:
`.claude/hooks/session-start.sh` (apt packages, `cargo build -p pav_tools --bin pav`) and
`rustup target add wasm32-unknown-unknown`. All files are < 2.6 MB (largest: `anim/cmu/*.json`);
the tree is ~87 MB. Commit the import as one commit ("Vendor shardfall engine @ 0098196") before
any change, so engine diffs stay reviewable.

### 8.2 Crate boundaries
- **Engine crates stay game-agnostic.** Nothing in `pav_*` names Black Tap, rum or pirates. Ship
  physics is "vessels", the climb system is "traversal", missions are "objectives": usable by the
  next game. Content-specific numbers live in `games/blacktap/*.toml`.
- **`pav_kit` holds systems a game composes inside its hook** (state lives in the game struct,
  so snapshots, rewind and replays cover it). `pav_kit` depends only on `pav_core`; widen
  `pav_core`'s API (`pub(crate)` → `pub`) where the kit needs it and log each widening in
  `ENGINE_UPSTREAM.md`.
- **`blacktap` is thin**: data loading, the Round scripts the data can't express, the bot, the
  demo playlist and the `UiModel` composition. Target: under ~25% of new lines.
- **Registration:** `pav_core::games::register(Entry)` (a `OnceLock`/`Mutex` list consulted by
  `find`, `scenes::menu` and `GameBox` deserialisation) beside the static `GAMES`.
  `blacktap::register()` is called first thing in `pav_app::main`, the `pav` binary, and every
  test that loads the scene (idempotent).
- **Data embedding:** `crates/blacktap/build.rs` embeds every file under `games/blacktap/` (same
  pattern as `pav_core/build.rs` does for `game/`), keyed by relative path; when running from the
  repository root the files on disk win (hot reload at `load`), as `games::level_text` does.

### 8.3 Game hook v2 (`pav_core::games::Game`)
All new methods have defaults, so the starter keeps compiling. All new types are plain data in
`pav_core` (no graphics), produced in `Sim::frame()` into a new `RenderFrame.play: PlayFrame`
and consumed by `pav_view` (draw list, atmosphere, effects), `pav_app` (camera, audio, haptics,
UI via `pav_ui`) and `pav_tools` (captures reproduce the same picture headlessly).

```rust
pub trait Game: Clone + Serialize + DeserializeOwned + Send + Sync + 'static {
    // v1 (unchanged): KEY, pre, post, hud, status, outcome, bot
    fn camera(&self, sim: &Sim) -> Option<CameraHint> { None }          // §9.5
    fn ui(&self, sim: &Sim) -> UiModel { UiModel::default() }            // §11.4 HUD + overlays
    fn draw(&self, sim: &Sim, out: &mut DrawList) {}                     // §9.17 game-state drawing
    fn atmosphere(&self, sim: &Sim) -> Option<Atmosphere> { None }       // §9.2 sky/sea/weather/grade
    fn audio(&self, sim: &Sim) -> AudioState { AudioState::default() }  // §9.13 music + ambience
    fn effects(&self, sim: &Sim) -> FrameEffects { FrameEffects::default() } // time scale, shake,
                                                                         // tints, transition, rumble
    fn tunables(&mut self, _v: &mut dyn ParamVisitor) {}                 // panel + `pav set game.*`
    fn front_end(&self) -> Option<FrontEndInfo> { None }                 // title, theme, demo list
    fn progress(&self) -> Option<serde_json::Value> { None }             // §9.16 save
    fn restore(&mut self, _v: &serde_json::Value) {}
}
```
- `CameraHint { mode: Orbit{target, yaw, pitch, distance, shoulder} | Shot{eye, target} , fov,
  lag, blend, snap, cut_occluders: bool }`
- `UiModel { hud: Vec<HudItem>, overlay: Option<Overlay>, prompts: Vec<Prompt>, subtitles,
  banners, toasts, markers: Vec<WorldMarker>, awareness: Vec<AwarenessArc>, demo: Option<DemoCaption> }`
- `DrawList { items: Vec<DrawItem> }` with `DrawItem::Asset{key, placement, palette}`,
  `Prim{shape, xform, color, look}`, `Mesh{generator_key, params, xform}` (ships, sails),
  `Decal{..}` (telegraph rings), `Highlight{entity, color}` (Gull Sight), `Trail{..}`.
- `Atmosphere { time_of_day, sky, sun, fog, clouds, sea_state, wind, rain, lightning, wetness,
  grade }` — a full description the view blends toward.
- `AudioState { music: Option<MusicCue{song, intensity, stems}>, ambience: Vec<(LayerId, f32)>,
  listener }`.
- `FrameEffects { time_scale, shake, tints: Vec<(EntityId, Rgb, f32)>, transition: Option<..>,
  rumble: Vec<RumbleCue> }`.
- **`SimEvent::Cue { cue: CueId, pos, gain, pitch }`**: game-defined one-shot sounds and FX by
  name, resolved by data banks in `pav_audio` and `pav_view` (fixes the closed lists).
- **Zone events reach the game:** run `update_zones` before `play_post` (or queue its events
  into the next `pre`), so Checkpoint/Splash/Pad/Respawn are visible to game code.
- **`bombs.weapon = "none"`** (new value) so PRIMARY/USE don't throw bombs or fire the blaster.

### 8.4 Input v2
- `InputFrame` gains `look: Vec2` (right stick or mouse delta, per tick; the game owns camera
  yaw/pitch, so replays include the camera) and `triggers: [f32; 2]` (analog L2/R2), both
  `#[serde(default)]`.
- Button names become data: `NAMES` covers all 32 bits; a game's `bindings.toml` maps physical
  inputs (keys, mouse buttons, wheel, pad buttons incl. L3/R3, D-pad, Start/Options,
  Select/Create, touchpad click, trigger thresholds) to named actions per bit. `pav_app` builds
  its `Bindings` from it; prompts look up the glyph for an action on the last-used device.
- Gamepad: keep gilrs (natively and in the browser, where its backend uses the Gamepad API —
  untested in this engine so far: verify in Phase 1 with a scripted Playwright gamepad or a
  quick manual note, and fall back to reading `navigator.getGamepads()` through web-sys if it
  fails); add DualSense/Xbox family detection from the pad name for glyph sets; force feedback
  via `gilrs::ff` natively and `GamepadHapticActuator`
  (`vibrationActuator.playEffect("dual-rumble")`) via web-sys on wasm.
- Menus never receive game input and the game never receives menu input (as today).

### 8.5 Frame flow with the new pieces
```
input (pav_app: keys/mouse/pad → InputFrame{move_dir, look, buttons, triggers})
   │      attract: InputFrame ← Game::bot (TimeControl.autopilot), real input only exits
   ▼
Sim::step → behaviours → Game::pre (kit: vessels, brains, melee, perception inputs)
          → characters (traversal: free-run/climb/swim-in-waves) → physics → projectiles
          → zones (now before post) → Game::post (kit: ballistics, vitals, mission, heat, dialog)
   ▼
Sim::frame → RenderFrame { objects, statics, events (incl. Cue), hud, play: PlayFrame {
             camera, ui, draw, atmosphere, audio, effects } }
   ▼
pav_view (camera rig drive, sky/ocean/terrain, draw list, FX cues, highlights) → pav_render
pav_app  (pav_ui front end + HUD from UiModel, audio state → pav_audio, haptics)
pav_tools (same frame → capture PNG with UI composited; tools read the same data)
```

### 8.6 Determinism, snapshots and memory
- Every new gameplay state (ships, brains, perception meters, missions, camera yaw/pitch, heat,
  inventory) lives in the game struct or entities: `Clone` + serde, `BTreeMap` not `HashMap`.
- Big immutable data (heightfields, nav layers, structure meshes, parsed TOML) sits behind
  `Arc` and is `#[serde(skip)]`-rebuilt from the level when missing, so the 20-tick history
  clones stay cheap. Set `sim.history_seconds` low for this game (e.g. 10 s) and confirm with
  `bench` that history doesn't dominate; rewind is a developer feature here.
- The ocean function is pure (`f(x, z, t, sea_state)`), so swimming, buoyancy and ballistics are
  deterministic; the GPU renders the same function for looks only.
- Checkpoints are in-memory snapshots (`SimState` clones). Saves on disk/localStorage store
  progress, not snapshots (§9.16).

---

## 9. Subsystem specifications

Each spec gives the design, the data, where the code goes, the tools and tests, and what
"done" looks like. Numbers are starting values: put them in data and tune them with the tools.

### 9.1 Ocean and buoyancy
- **`pav_core::ocean` (pure math, engine):** `SeaState { waves: [Wave; 6] (direction, wavelength,
  amplitude, steepness, speed), detail, wind }` with presets `calm` (swell ~0.25 m), `breezy`,
  `rough`, `storm` (~1.8 m) and `blend(a, b, t)`. Functions: `displacement(p, t)`,
  `height(p, t)` (invert the Gerstner horizontal shift with 2–3 fixed-point iterations so the
  height is at the queried point), `normal(p, t)`, `sample(p, t) -> Surface{height, normal,
  velocity}`. Time is sim time. The active sea state lives in `SimState` (set by the game each
  tick), so swimming, ships, floating props and ballistics all read the same surface.
- **Swimming in waves:** a water-surface provider in `character.rs`: when an ocean is active, a
  character is in water where `feet < ocean.height` and the seabed (island heightfield) is
  deeper than 1.3 m; wading in between; the swim float height follows the surface. Keep water
  zones for any small calm pools.
- **Buoyancy:** dynamic props (flotsam, barrels, kegs) get an upward force from submerged depth at
  1–4 sample points plus linear/angular drag (extend `zone_props` in `destruct.rs`), so cargo
  bobs and drifts with the waves.
- **Renderer (`pav_render` + `pav_view`):** a camera-centred concentric LOD grid (≈6 rings, 1 m
  spacing near the eye to 32 m far, vertex morphing between rings, a skirt to the far plane),
  displaced in the vertex shader with the same wave parameters (uniforms) as the CPU function.
  Fragment: deep → shallow colour from a **seabed depth texture** (R8, generated at level build
  from the island heightfields, ~0.6 m/texel over the play space), Fresnel blend toward the sky
  colour, sun glint, crest foam from steepness, a shoreline foam band (depth < 1.5 m) with
  animated noise, two octaves of scrolling detail normals, a turquoise sub-surface tint at
  crests. Opaque (no refraction); the camera never goes below the surface (+0.6 m clamp).
- **Wakes and spray:** foam trail decals behind ships and bow spray particles (draw list + FX).
- **Tools/tests:** `pav ocean` (sample height/normal at points; set sea state); unit tests
  (deterministic, mean ≈ 0, bounded by Σ amplitudes, blend continuity); a capture contact sheet
  of all sea states; a swimmer and a ship bob in visible sync with the rendered waves.
- **Done:** sea to the horizon with no seams or popping, turquoise shallows and white surf around
  every island, readable swell that grows convincingly in the storm.

### 9.2 Atmosphere: sky, time of day, weather
- **`Atmosphere` (pav_core data):** time of day, sun (azimuth, elevation, colour, intensity), sky
  (zenith, horizon, ground colours), ambient (sky, ground, intensity), clouds (cover, density,
  speed, colour, altitude), fog (colour, start, end, height falloff), haze, sea state (preset +
  blend), wind (direction, speed, gustiness), rain 0–1, lightning rate, wetness, grade
  (temperature, saturation, contrast, exposure). `blend(a, b, t)` in linear colour.
- **Presets (data, `games/blacktap/atmosphere.toml`):** `golden_hour` (title, R1–R3),
  `afternoon` (R4–R5), `sunset` (R6 start), `storm` (escape), `dawn_after_storm` (results, free
  roam), `memory_mist` (Last Words). Free roam may run a slow day cycle (one day ≈ 24 min).
- **Sky pass (renderer):** for depth-1 pixels: zenith↔horizon gradient, sun disc with glow,
  procedural FBM clouds on a curved sky plane scrolling with the wind and lit from the sun,
  storm darkening, lightning (sky and cloud flash + a brief directional-light boost).
- **Fog along the view ray** (today it uses horizontal distance from the target) with an
  aerial-perspective colour toward the horizon; far plane extended to ~1,800 m when the ocean is
  active (camera hint sets it).
- **New view settings:** `light.sun_color`, `light.ambient_sky`, `light.ambient_ground`,
  `sky_zenith`, `sky_horizon`, `fog_color`, `clouds.*`, `rain`, `wetness`, `far`.
- **Weather:** camera-following rain streaks slanted by the wind, rain rings on the ocean (detail
  normal intensity), wet surfaces (darker albedo, more specular), splashes on decks, lightning
  with thunder cues delayed by distance (timing from the game's rng). **One wind** drives
  sailing, smoke drift (add a wind term to particles), vegetation/flag/sail sway, cloud scroll and
  the wave direction bias.
- **Shadows:** sun shadow map centred on the camera, radius ~70 m (Medium) / 100 m (High).
- **Tests:** blend determinism; view settings reach the renderer (pattern of
  `pav_view/tests/parts.rs`); a capture contact sheet of every preset.

### 9.3 Islands: terrain and vegetation
- **Data (`[[island]]` in the level):** `outline` polygon, `peaks` [{at, height, radius}], `beach`
  width, `shelf` width (shallows), `cliffs` [{from, to, height}], `noise` {scale, amp, seed},
  `flatten` areas [{polygon, height}] for the town and fort, `roads` [{points, width}].
- **Generator (`pav_core::island`, deterministic):** signed distance to the outline → profile
  (seabed −12 m → shelf → beach → inland rise) + peaks + inland FBM + sea cliffs + flatten blend
  + road carving, on a 1 m grid over the island bounds plus shelf.
- **Mesh:** 32 m chunks with skirts and two LODs; per-vertex material weights (sand, wet sand,
  grass, rock, dirt/road) from height, slope and noise; a terrain shader blends procedural
  materials (start with vertex colours via `TerrainPatch` for the skeleton, then upgrade).
- **Collision:** a rapier `HeightField` collider (add `statics.set_heightfield`), much cheaper than
  a trimesh at this size. **Climb mask:** slope > 55° and > 1.5 m above the local base →
  climbable cliff.
- **Vegetation (data rules, deterministic scatter):** palms by height band and slope (dense on
  beaches and in the plaza), jungle trees inland, bushes, ferns, rocks, driftwood; authored
  **stalk-grass** polygons (gameplay concealment, using the grass-tuft renderer at 1.0–1.3 m);
  exclusion around roads, footprints and flatten areas. Plants are static decor (no entities)
  with sway; palm and tree trunks have cylinder colliders and are climbable poles.
- **New prop families:** `palm` (curved trunk of tilted ringed cylinders, 6–9 drooping fronds,
  coconuts; height 5–11 m, lean, seed), `jungle_tree`, `bush`, `fern`, `driftwood`.
- **Tests/tools:** deterministic generation (hash), collider height = mesh height at samples,
  flatten areas flat (±5 cm), foundations touch terrain; `pav levelmap scene=blacktap` draws the
  island (heights, coast, structures, nav, markers, bot hints) as a top-down PNG.

### 9.4 Structures: town, fort, docks
- **Generators (`pav_core::structures`, deterministic, game-agnostic)** emit blocks (with flags:
  climbable, walkable roof), ladders, stairs, decor and props, labels, zones (soft landing,
  hiding spot, perch), handhold decor (sills, beams, shutters) and nav links. Rotations are
  quarter turns (blocks stay axis-aligned, so collisions and climbing stay simple).
- **Kinds:** `house` (w×d, 1–3 floors of 3.2 m, flat roof with a 0.6 m parapet, balcony, awning,
  beam outriggers, shutters, door), `chapel` (nave + 24 m bell tower with a perch), `warehouse`,
  `hall`, `tavern`, `stall`, `stocks`, `hay_cart` (soft landing), `pier` (planks on posts,
  bollards, ladders to the water), `quay` (stone, stairs), `harbour_wall`, `fort_wall` (3 m thick,
  walkway, crenellations, inner stairs), `bastion`, `battery` (gun platform + 3–4 cannon
  targets), `mortar_tower`, `keep` (balcony, bell tower), `gate` (openable), `barracks`,
  `lighthouse`, `watchtower_ruin`, `hut`, `laundry_line` (rope + cloth; walkable), `beam`
  (plank between roofs), `palisade`, `crate_stack`, `wanted_poster`.
- **Placement:** `[[structure]] kind = "house" at = [x, z] yaw = 90 size = [8, 6] floors = 2
  style = "ochre" seed = 3 balcony = "south"`; foundations fill down to the terrain.
- **Styles (data):** plaster ochre `#d9a35b`, coral `#e07a5f`, sea-foam `#8fc1a9`, sky `#7fb7d6`,
  lime white `#efe6d2`; limestone `#cbbd9b`; timber `#6b4a2f`; terracotta `#b5543a` (visual
  pitched roofs only on a few non-traversable buildings).
- **New procedural materials (data, `assets/materials/`, `docs/MATERIALS.md` patterns):**
  `sand`, `wet_sand`, `plaster`, `limestone`, `terracotta`, `canvas`, `thatch`, `rope`.
- **Tests:** each generator deterministic; climb flags present; roofs walkable; gates open and
  close; `pav level_check scene=blacktap` validates islands, structures, markers, mission
  references and bot hints.

### 9.5 Camera director
- **The game owns the orbit** (yaw, pitch −20°…70°, distance per context) and updates it from
  `InputFrame.look` (right stick in deg/s with a response curve; mouse in deg/px; invert and
  sensitivity from settings). Auto-recentre behind the player after 2.5 s without look input
  while moving (setting).
- **Spring arm:** raycast from the shoulder pivot to the desired eye against statics, terrain and
  ship colliders (ignore foliage), pull in with a 0.2 m margin, ease back out; under 1.2 m fade
  the player's puppet. Eye ≥ ocean surface + 0.6 m.
- **Contexts (data, `camera.toml`):** foot (5.5 m, pitch 12°, fov 60°, shoulder 0.45 m), free-run
  (6.2 m, fov 66°), climb (7 m), hang, crouch/stealth (4.5 m, lower), combat (7.5 m, frames the
  nearest foes), aim (over-shoulder 2.2 m, fov 45°), swim (6 m, pitch 8°), helm (30 m, pitch 14°,
  fov 62°), broadside aim (yaw to ±90° of the hull, pitch 18°, 26 m, arcs in view), spyglass
  (first person at the helm, fov 25°→12° zoom), survey (an 8 s 360° orbit at the perch),
  cinematic shots (mission data), title (slow orbit of the ship), demo director (§12).
- **View side:** `CameraHint` → `CameraRig::drive` (explicit eye/target, negative tilt for
  looking up, per-hint far plane). While a hint is active, disable `front_cut` and `height_cut`;
  keep the dithered occluder `fade`.
- **Blends:** 0.35–0.8 s eased; hard cuts for cinematics.
- **Tests:** in a property test over random town positions the eye is never inside a block;
  per-frame camera motion is continuous except on cuts; yaw/pitch replay exactly from inputs.

### 9.6 Ships: sailing, helm and deck
- **`pav_kit::naval::Vessel` state:** pose (pos, yaw), forward speed, leeway, yaw rate, visual
  heave/pitch/roll (from ocean samples at bow, stern, port, starboard; smoothed), sail level
  (0 furled/anchored, 1 half, 2 full; plus a boost), rudder, hull HP, sail HP, crew count,
  cargo (timber, iron, reales), weapons state, fire, sinking, faction, controller.
- **Arcade sailing (tunables in `ships.toml`):** `target = max_speed[sail] × wind_factor(angle) ×
  sail_health`, with `wind_factor` ≈ in irons (0–30°) 0.15, close-hauled (45°) 0.6, beam reach
  (90°) 1.0, broad reach (135°) 0.9, running (180°) 0.75; acceleration and braking rates; turn
  rate by speed (≈4°/s at standstill → 22°/s at mid speed → 16°/s at full); small leeway; heel
  into the turn and away from the wind.
- **Classes (data):** brig *Black Tap* (32 m, HP 1200, 6/11/15 m/s, 8 guns a side, 2 chasers,
  1 swivel, 4 kegs); gunboat (12 m, HP 220, 1 bow gun); schooner *TS Restraint* (24 m, HP 700,
  6 a side); frigate *TS Abstemious* (40 m, HP 2400, 14 a side, pursuer); moored sloop; convoy
  merchant (free roam).
- **Collision:** hull as an OBB/capsule chain in XZ. Islands: sample seabed depth along the hull;
  depth < draft → grounding (hard brake, damage above 4 m/s, push out along the gradient). Ships:
  separate and apply ram damage by relative speed. Swimmers are pushed aside.
- **Physics body:** a kinematic compound body (deck, hull sides, rails, masts, helm) moved from
  yaw + heave only (velocity-based like the engine's movers, which already carry riders, or
  position-based — pick what keeps deck walkers stable in the moored/boarding tests); pitch and
  roll are visual, so the deck stays level for walkers. (Add `Spawn` support for compound
  colliders.)
- **Helm and deck:** take the helm at the wheel (interact) → seated rider with a visible helm
  pose (generalise vehicle seating: `Seat { offset, yaw, visible, pose_clip }`). Leave the helm
  only at ≤ 2.5 m/s (the prompt is hidden otherwise); then the ship drifts to a stop (anchored),
  the deck is walkable, crew NPCs idle on deck (`Rail_Loop`, cheering, hauling). Dive off,
  swim, climb back by the hull ladder, walk to the wheel.
- **Visuals (procedural, `pav_view`):** a `ship_hull` mesh generator lofted from cross-sections
  (length, beam, depth, sheer, tumblehome, transom) with planking, black wales, painted gunports,
  deck, rails, tapered masts, yards, bowsprit, rigging lines, a brass keg-tap figurehead; sails
  as small cloth grids (~16×16) billowed per frame by wind and sail level, furled sails as rolls
  on the yards; the *Black Tap*'s black sails carry a brass tap emblem (drawn in the sail shader);
  flags sway; lanterns light up in the storm. Damage: dark holes, smoke and fire, a broken
  topmast, listing and sinking with bubbles and floating debris.
- **Tests:** speed by wind angle matches the data; turning; grounding (a ship can't pass through
  an island); ship–ship separation; a character standing on a moored deck stays put; helm
  enter/exit; snapshot/rewind with ships is exact.

### 9.7 Naval combat, forts and boarding
- **Weapons (data):**
  - Broadside: N guns a side; muzzle speed ~70 m/s, spread ±0.8°, 30 damage a ball, reload 6 s;
    each volley staggers over 0.4 s with muzzle flashes and wind-drifted smoke.
  - Chain shot: 2 bow chasers, 80 m; hits sails → −50% speed for 20 s.
  - Swivel: 1 precision shot, 2 s cooldown; ×3 on glowing weak points (powder stores); also
    kills deck crew and gunboats.
  - Keg Drop: up to 4 floating powder kegs dropped astern; they explode on proximity or when
    hit; 15 s reload.
  - Boost/ram: R1 gives +25% speed for 3 s; ramming damage.
- **Aiming:** the weapon follows where you look. Within ±45° of the beam → that broadside; ahead →
  chain shot; astern → Keg Drop. L2 shows predicted arcs from each gun to the water or target,
  with a lead marker and adjustable aim assist (elevation ±3° toward a locked target). R2 fires.
- **`pav_kit::ballistics` (own deterministic list):** id, pos, vel, gravity, kind, owner, damage,
  radius. Each tick it tests the segment against ship hull volumes (and the sail volume for
  chain shot), blocks/structures (sim raycast), terrain height, the ocean surface (→ splash cue,
  removed) and characters (kills crew). Emits `SimEvent::Cue` for fire, splash, hull hit,
  splinters and explosion. (Engine projectiles stop only at fixed colliders and report no
  impacts, so don't use them for this.)
- **Damage:**
  - 75% hull: smoke.
  - 50%: fire (damage over time until repaired).
  - 25%: **Disabled** — enemies stop firing, drift and can be boarded.
  - 0: enemies sink. For the player, 0 is **Spilled**: reload the checkpoint.
  - Sail damage cuts speed; crew losses mean fewer boarders.
- **Repair:** D-pad ↑ at sea spends Timber (+200 HP, 20 s cooldown); a full repair is a plunder
  choice.
- **Brace:** △ halves incoming damage for 1.2 s (4 s cooldown). Enemy gunports glow orange 1.5 s
  before a volley, and Ballast squawks "BRACE!".
- **AI captains (`pav_kit::naval::Captain`):**
  - Gunboat: bow-on charges and circling; fires its bow gun at 60–120 m.
  - Schooner: holds a broadside angle at 60–110 m and fires when aligned and loaded; uses chain
    shot up close; flees toward the fort under 30%.
  - Frigate: pursues and fires its bow chasers; steers clumsily round waterspouts.
  - Convoy merchants: flee while their escorts fight.
  - All: wind-aware (tack, never sit in irons), steer away from shallows (seabed sampling along
    their heading), avoid collisions.
- **Forts (`pav_kit::naval::Battery`):** static emplacements with HP (battery 600, mortar tower
  900) and ranges (batteries 180 m, mortar 260 m).
  - Batteries: telegraphed volleys every 7 s.
  - Mortar: shells fly 2.2 s; a red ring decal marks the landing point from launch.
  - Destroyed states: rubble, fire, silence.
  - A health bar shows when targeted.
- **Boarding (`pav_kit::naval::Boarding`):** available when the enemy is disabled, within 25 m,
  under 4 m/s relative speed, and the player is at the helm → "✕ Board". Then:
  1. Both ships blend alongside over 2 s (decks parallel, 4 m gap) under a cinematic camera;
     grapple ropes are drawn; the player swings across.
  2. Four allied crew join; enemies spawn by remaining crew (six crew + officer + Captain
     Sobers).
  3. Both ships become static platforms. Falling in the water means climbing a hull ladder.
  4. Victory when the captain and officer fall and fewer than two crew remain (the rest
     surrender and kneel).
  5. Plunder overlay: *Repair hull* or *Take cargo*, plus the upgrade pick in R2.
  6. Fade back to the helm; the prize is scuttled.
- **Tests:**
  - Ballistics hit at 100 m within the spread.
  - Fort telegraphs precede impact by ≥ 1.2 s.
  - The AI schooner holds broadside within ±20° most of a duel.
  - The disabled threshold enables boarding.
  - Boarding victory → plunder → helm restored.
  - Mid-battle snapshot/rewind is exact.

### 9.8 Traversal: free-run, climb, leap, swim (engine, `character.rs`)
- **Free-run** (`freerun` held):
  - Run 6.8 → sprint 8.0 m/s.
  - Auto-vault obstacles ≤ 1.1 m; auto-step 0.5 m.
  - **Parkour jump assist:** at a roof edge with a landing within 3.5 m ahead (≤ 2 m below,
    ≤ 0.5 m above), launch onto it.
  - **Ledge safety:** at walking pace you stop at edges unless you push off with jump or
    Parkour Down.
- **Parkour Up** (free-run + ✕): mantle ≤ 2.1 m; start a wall climb on a climbable surface within
  0.8 m; jump to handholds. **Parkour Down** (free-run + ◯): drop to a hang, climb down,
  controlled drops to lower roofs. Walking into a wall without free-run never climbs.
- **`WallClimb` state** (new `climb: Option<WallClimb>` after the ledge block; gate it everywhere
  `climbing || hanging` is checked: `character.rs` 615, 691, 771, 964, 972, 1043, 1095, 1174):
  - Attaches to block faces flagged climbable, terrain cliffs (climb mask), and poles, trunks
    and masts (cylinders, 360°).
  - The stick maps camera-relative to up and sideways on the surface: up 2.2, down 2.6,
    sideways 1.8 m/s.
  - Tap ✕ for a climb leap (+1.4 m in 0.35 s).
  - Outer and inner corner wrap.
  - Auto-mantle onto walkable tops; vault parapets (0.6–1.2 m).
  - Roof-edge hang (reuse `Hang`).
  - Back-eject (✕ with the stick away from the wall: 3 m out, 1 m up; can catch the opposite
    wall or a pole).
  - ◯ lets go. No stamina.
- **Leap of Spirits:** on a `perch` zone, ✕ toward the leap direction → a swan dive (clip or
  procedural pose) on an assisted arc into the linked soft-landing zone (hay cart, deep water) →
  roll out, no damage. Any fall into a `soft` zone (hay, awnings that bounce, water ≥ 2.5 m deep)
  is safe.
- **Fall damage** (kit, from `SimEvent::Land{speed}`):
  - Safe under 11 m/s (~6 m).
  - A roll landing (free-run held) raises the safe limit to 14 m/s (~10 m).
  - Above that, damage rises linearly to lethal at 22 m/s.
- **Narrow beams and laundry lines:** balance assist snaps you to the centreline, slower, arms
  out. (Rope slides are a P2 extra.)
- **Swimming in waves:**
  - 3.2 m/s, or 4.5 m/s with free-run.
  - ◯ dives just under the surface for up to 6 s (hidden from sight; P1).
  - Climb out onto docks, rocks and hull ladders (`movement.ledge_grab = true` in this level).
- **Animation:** advance the puppet's climb phase with sideways motion too. Use clips where they
  read well: `Climb Wall`, `Ledge Hang`, `Swim_Fwd_Loop`, `Dive_Roll`, `Land_Three_Point` (the
  leap landing), `Sprint_Loop`, `Jog`.
- **Tests** (new `pav_core/tests/traversal.rs`):
  - Climb a 6 m wall and mantle onto the roof; outer and inner corners; back-eject to an
    opposite wall.
  - No accidental climb at walking pace.
  - Vault a 1 m crate; ledge safety stops at a roof edge; jump assist crosses a 2.5 m gap.
  - Leap into hay without damage; a 12 m fall onto stone hurts.
  - The swimmer follows the wave surface (±0.15 m); climb out onto a pier; pole climb.

### 9.9 Combat on foot (`pav_kit`)
- **Vitals:**
  - Health: player 100 (+upgrades), guard 60, brute 140, musketeer 50, officer 90, Drye 120,
    civilians 20.
  - Damage kinds: blade, shot, blast, fall.
  - Death stops the brain and plays a death clip or the puppet topple. The body stays as a
    discoverable corpse and drops reales.
  - The player regenerates after 4 s out of combat; Grog Ration (D-pad ↑, carry 3, refill at
    rum barrels) heals 50%.
- **Melee (data, `combat.toml`):**
  - Light chain of 3 (`slash`, `backslash`, `thrust`; 18/18/30); heavy on □ hold (`overhead`;
    breaks guards).
  - Finishers at 0 HP (a paired kill with 0.15 s slow-motion).
  - Enemy tells: weapon glint + 0.45–0.6 s wind-up.
  - **Attack tokens:** at most 2 enemies attack at once; the rest circle and taunt.
  - **Counter:** △ within [−0.35 s, +0.10 s] of the hit → counter-kill (guards) or
    disarm/stagger (brutes, officers). A missed counter costs a short stagger.
  - Dodge: ◯ in combat; 9 m/s, 0.25 s, 0.3 s i-frames.
- **Assassinations (Tap Blade, □ prompt when valid):**
  - Standing: behind or unaware, ≤ 1.6 m, target not alerted.
  - Air: 1.5–6 m above the target, ≤ 3.5 m away, falling or on a ledge (move `plunge`).
  - From hiding: in grass or hay, target ≤ 2 m.
  - Ledge: hanging, target near the edge above.
  - Double: two targets ≤ 1.2 m apart; unlocked by a Keg Tap token.
  - Noise: quiet (2 m), except air (6 m).
- **Pistols:** two flintlocks.
  - L2 locks the best target in a 25° cone within 30 m; R2 fires a hitscan with line of sight
    (60 damage).
  - 3 s reload each; noise 35 m.
  - Quick-shot: R2 without aim targets the nearest foe ahead after a 1 s wind-up.
- **Tools (L1 radial wheel, 20% slow-motion while open):**
  - Nightcap Dart: the target sleeps 60 s; silent.
  - Grog Dart: the target staggers and swings at his allies for 12 s, then dozes off.
  - Smoke Keg: 4 m cloud for 6 s that blinds and staggers; free assassinations inside.
  - Bung Line: pull a target off a ledge or to you (P1).
- **Feel:**
  - Hit-stop 60–90 ms on hits, 140 ms on kills.
  - Light shake, rumble pulses, steel sparks.
  - No blood (family-friendly): sparks and dust instead.
- **Tests:**
  - Counter window timing in exact ticks.
  - The assassination validity matrix.
  - Attack-token limits.
  - Pistol line of sight blocked by walls.
  - Darts change brain state.
  - Corpses persist.
  - The bot wins the boarding melee.

### 9.10 Perception, stealth and heat (`pav_kit`)
- **Sight per archetype (data):**
  - Ranges: near 8 m (fast notice), far 28 m (slow).
  - FOV: horizontal 110°; vertical −50°…+35° (a player high on a roof is unseen unless the
    guard is searching and looks up); peripheral 160° at ×0.3 rate.
  - Line of sight: rays to head, chest and feet against statics, terrain, structures and ships.
  - **Concealment:** inside stalk grass, a haystack, a hay cart or a crowd and not high-profile →
    invisible beyond 1.5 m; haystacks and carts fully hide.
  - Modifiers: crouch ×0.6 range; sprint ×1.4 rate; restricted areas ×2 (instant at near
    range); heat ×(1 + 0.4·heat) in town; witnessed crimes and seen bodies add awareness.
- **Hearing:**
  - `pav_kit::noise` events with radius: sprint steps 5 m, hard landing 8 m, pistol 35 m,
    cannon 150 m.
  - Also: whistle 14 m (a lure), body drop 6 m, clashes 20 m, explosion 60 m.
  - Bells alert the whole area.
- **Awareness and states** (meter 0–1 per NPC per target):
  - Unaware → **Suspicious** "?" at > 0.3 (stops, looks at the stimulus).
  - → **Alerted** "!" at 1.0 or strong evidence (chases, fights, shouts to allies within 18 m,
    a bell-assigned guard runs to the alarm).
  - Stimulus lost → **Investigating** (walks to the last known position or the noise) →
    **Searching** (random points round it for 20 s, pokes bushes: a poke within 1 m finds you) →
    **Returning** (patrol resumes, extra alertness for 60 s).
  - In combat, 6 s without sight → search mode; hidden 25 s → Lost.
- **Alarm bells** (fort, town): ringing raises area alert, spawns bounded reinforcements from the
  barracks, and adds +1 heat. Kill the ringer to stop it.
- **UI data:**
  - Awareness arcs over heads: white fill → yellow "?" → red "!".
  - Off-screen pips at the screen edge.
  - A last-known-position ghost silhouette while they search.
  - A detection stinger.
- **Heat 0–3 (town and fort):**
  - Witnessed crimes raise it. Heat 1: guards ×1.4 suspicious. Heat 2: instant recognition at
    near range. Heat 3: hunters and converging patrols.
  - Lower it by tearing down wanted posters (6 in town), bribing the crier (reales), or waiting
    60 s at heat 1.
  - HUD: a 3-segment poster icon.
- **Implementation:**
  - A `Perception` struct in game state (`BTreeMap`s per NPC), ticked in `post`.
  - Round-robin sight checks (every 3 ticks per NPC) to keep 100+ NPCs cheap.
  - Emits events that the brains consume.
  - **Don't use the engine's `AiDef::Guard`** (it resets the player on "SPOTTED"): spawn NPCs with
    `Idle` and drive them from `pre` (set `ai.wait = 0`).
  - The engine's cone fan can serve as a Drydock debug view and as an accessibility option
    ("show vision cones").
- **Tests:**
  - Seen in the open at 15 m; hidden behind a wall; hidden crouched in grass at 5 m but seen at
    1 m; a rooftop player 6 m up and 6 m away is unseen.
  - A whistle lures; states progress in order; alerts propagate within radius.
  - A bell spawns reinforcements; heat rises and falls; deterministic.

### 9.11 NPC brains, crowds and 2.5D navigation
- **Nav 2.5D (`pav_core::nav` extension):**
  - A layered 0.5 m grid over the island bounds; each cell holds up to K walkable surfaces
    (terrain under 40° slope, streets, floors, wall walks, stairs).
  - Edges where |Δh| ≤ 0.45 m, plus explicit links: stairs, ladders (for NPCs that climb),
    doorways, gates (toggled when opened or closed).
  - A* and reach-limited flow fields.
  - Built at level load as a `#[serde(skip)]` cache behind `Arc`. Roofs stay the player's space;
    musketeers shoot up at you, and ladder-climbing chasers follow slowly.
- **Brains (`pav_kit::brains`, archetypes in `npcs.toml`):**
  - Fighters: guard_sword, guard_brute, musketeer, officer, sentry_tower.
  - Townsfolk: civilian (vendor, stroller, gossip group), crier, courier, escort.
  - Story: target_drye, captain_sobers.
  - Ship crews: crew_ally, enemy_crew.
  - Each archetype has a senses profile, a combat profile, a route or post, and role idle clips
    (`Talking`, `Looking_Around`, `FoldArms`, `Torch`, `Lantern`, `Sitting_Idle`, `Drink_Water`).
  - Brains output `InputFrame`s from `pre` plus animation intents.
- **Crowds:** gossip groups of 3–5. Walking into one (not sprinting) blends you in: the group
  carries a moving concealment volume. Civilians flee drawn weapons and fights, cower, scream
  (a noise event that alerts guards), and at heat ≥ 2 point you out.
- **Scale and performance:**
  - ~120 NPCs on the island; brains tick at 20 Hz, combatants at 60 Hz.
  - NPCs beyond 120 m freeze unless mission-critical.
- **Tests:**
  - Dock-to-fort-gate path over terrain and stairs; patrol loops.
  - Civilians flee; the courier's route completes; crowd blending conceals.

### 9.12 Missions, checkpoints, collectibles and upgrades
- **`pav_kit::mission`** runs objective graphs from data (`games/blacktap/missions/*.toml`, one per
  Round):
  ```toml
  id = "r2_broadside"
  title = "Second Round — Broadside"
  setup = [{ atmosphere = "golden_hour" }, { place = "player_ship", at = "r2_start" }]
  [[step]]
  id = "gunboats"
  objective = "Sink the gunboats"
  markers = ["tag:gunboat"]
  on_start = [{ spawn = "group:gunboats" }, { music = "naval_battle" }, { line = "bung_broadside_1" }]
  done = { all = [{ dead = "tag:gunboat" }] }
  [[step]]
  id = "disable"
  objective = "Disable the TS Restraint"
  done = { disabled = "ship:restraint" }
  [[optional]]
  id = "chain_only"
  text = "Disable the schooner with chain shot"
  track = { disabled_by = { ship = "restraint", weapon = "chain" } }
  ```
- **Conditions and actions:**
  - Generic kinds in the kit: zone, dead, item, count, timer, flag, entity state, all/any/not;
    spawn group, place, line, music, atmosphere, give, open/close, checkpoint, cinematic.
  - Game-registered kinds: survey, boarded, disabled, destroyed, flag_raised, perch, heat,
    detected.
- **Runtime:** the current Round, step, optional trackers and timers live in the game struct.
  It feeds the `UiModel` (objective line, markers, Full Pour checklist) and the results screen.
- **Checkpoints:** add `Sim::checkpoint_save()` / `checkpoint_load()` that keep one snapshot
  **outside** `SimState` (not cloned by history). The game requests a save on step boundaries
  and a load on Spilled; app, tools and tests all honour it.
- **Round setup:** `blacktap/round/N` builds the level and applies Round N's `setup`
  deterministically. Used by Replay Round, attract segments, tests and the bot.
- **Collectibles:** pickup entities with a cue. Found IDs persist (§9.16). Surveys reveal icons
  within a radius.
- **Upgrades:** `upgrades.toml` gives cost and stat deltas (*Reinforced Hull*, *Heavy Shot*,
  *Swift Rigging*, *More Kegs*), chosen on big tiles.
- **Tests:**
  - Each Round is completable by the bot from its setup (5 seeds).
  - `pav level_check` validates every reference.
  - Optional objectives track.
  - A checkpoint load restores mission state exactly.

### 9.13 Audio, music and haptics (`pav_audio` + data)
- **Engine work:**
  - A data-driven bank: cue → patch layers (oscillator or noise, envelope, state-variable filter
    with resonance, pitch glide, LFOs, gain, pan spread, per-play randomisation). The old
    `sounds_for` match stays as the fallback.
  - Held and looping voices with handles.
  - Pink/brown noise; Karplus-Strong plucked strings; formant filters ("voices").
  - A small stereo reverb and delay.
  - Buses (master, music, sfx, ambience, voice, ui) with volumes and a master limiter.
  - Separate voice pools: sfx 48, music 24, ambience 8, so music is never stolen.
  - Positional pan and attenuation from the listener.
  - Offline rendering includes music (`audio_capture`).
- **Music sequencer** (sample-accurate, inside the audio callback):
  - Songs in an **ABC-notation subset** (`games/blacktap/shanties/*.abc`: melody, chord symbols,
    tempo, meter; agents read and write ABC well).
  - Arrangements in `music.toml`: instruments per part, sections, stems by intensity,
    crossfades and stingers.
  - Instruments:
    - Accordion: detuned saw/square with chorus and a bellows LFO.
    - Fiddle: saw + vibrato + band-pass.
    - Tin whistle: triangle + breath noise.
    - Plucked guitar: Karplus-Strong.
    - Bass: triangle.
    - Stomp and bodhrán: sine thump + noise. Clap: noise burst.
    - **Crew choir:** formant-filtered saws on vowels; unison, octave and a fifth on choruses;
      call (Old Bung solo) and response (crew).
- **Repertoire:** public-domain traditional shanties (Appendix C) plus an original title theme;
  comedic original verses as optional subtitles (e.g. *"What shall we do with a sober
  Commodore?"*).
- **Music states:**
  - Front end and voyage: title, sailing (shanty rotation; D-pad ←/→ at the helm), attract.
  - Combat: sea combat, fort siege.
  - On land: exploring (plucked calm), stealth (drone + pulses rising with the highest
    awareness), detected/combat.
  - Story beats: storm escape, Last Words (sparse), results fanfare.
- **Ambience layers (levels from `AudioState`):**
  - Sea: brown noise + swell LFO.
  - Weather: wind (band-passed, by wind speed), rain, thunder.
  - Ship and harbour: creaking timber, flapping sails, gulls (FM glides), bells.
  - Land: jungle insects and birds, town murmur (formant noise), fire.
- **Voices:** formant "blips" per speaker for dialog lines (pitch and vowel set per character);
  Ballast's squawk (FM).
- **UI sounds:** navigate (soft wooden tick), confirm, back, toggle, slider, error.
- **Haptics (rumble cues, data):**
  - Weapons: cannon fire (strong, 120 ms), hull hit, sword clash (short, high), assassination
    pulse.
  - Movement: hard landing, grapple.
  - Weather: thunder.
- **Browser autoplay:** the AudioContext unlocks on the first key, mouse or touch event. Gamepad
  presses don't count, so the title shows "Press any key or click to enable sound" while audio
  is locked; attract is silent until then (native is never locked).
- **Tests:**
  - Every bank, song and ambience file parses.
  - Every cue named in code or data exists.
  - 30 s offline renders of each song are non-silent, NaN-free and peak below 0 dBFS.
  - `audio_capture` in key scenarios shows the expected bus RMS.

### 9.14 Dialog, subtitles and cinematics (`pav_kit`)
- **Lines** (`dialog/*.toml`: id, speaker, text, duration auto from length, priority, bark
  category) are queued with interruption rules and show as subtitles in the `UiModel` (speaker
  name in their colour) with voice blips.
- **Barks** are contextual one-liners with cooldowns and seeded variety. Guards: "Who goes
  there?", "Must've been the rum…". Crew: cheers.
- **Cinematics** (`pav_kit::cinematic`) are shot lists: eye and target keys over time, fov,
  easing, letterbox, actor actions (place, clip, move). Skip by holding ◯ for 0.6 s ("Hold ◯ to
  skip"); a skip lands in the same end state. Used for the boarding grapple, Fort Captured,
  **Last Words** (the `memory_mist` atmosphere: white fog, only the actors visible) and Tab
  Complete.

### 9.15 Gull Sight, map, compass and markers
- **Gull Sight** (R3: hold or toggle in settings):
  - Desaturate and darken the environment; draw categorised silhouettes **through walls** via
    `DrawItem::Highlight` (a mask pass + coloured outline in `pav_render`).
  - Colours: enemies red, target gold, allies blue, collectibles and interactables white,
    hiding spots green, with colour-blind palettes.
  - Range: 60 m on land, 300 m at sea, where ships also show class, cargo and HP.
  - Tag an NPC (△ while centred in Gull Sight) for a persistent outline and compass marker.
- **Map** (touchpad / M): a full-screen parchment map drawn as vectors with egui.
  - Content: coastlines from heightfield contours (marching squares at 0 m and 20 m), roads,
    building footprints, icons (objectives, perches surveyed or not, revealed collectibles,
    ships, player arrow), fog of war (sepia wash) over unrevealed areas.
  - Controls: left stick pans, triggers zoom, ✕ sets a custom waypoint, △ toggles the legend,
    ◯ goes back.
- **Compass bar** (top centre): rotating ticks; objective, waypoint and revealed-collectible
  markers with distances; red threat pips.
- **World markers** (projected): objective diamond + distance, enemy ship and battery bars,
  interaction prompts on objects.
- **Tests:** map data deterministic; markers project correctly against a known camera.

### 9.16 Persistence and settings
- **`pav_core::persist::Store`** (get/set string by key) implemented by `pav_app` (native: JSON
  files next to the executable, `blacktap_*.json`; browser: `localStorage` `blacktap.*`),
  `pav_tools` (temp dir) and tests (memory).
- **Settings** (versioned): audio buses, display (quality, brightness, camera shake, motion,
  look preset), controls (invert X/Y, sensitivity, vibration, hold/toggle for crouch, aim and
  Gull Sight, prompt style auto/PlayStation/Xbox/keyboard), accessibility (subtitles, size,
  speaker names, colour-blind mode, high-contrast HUD, aim assist, vision cones), gameplay
  (difficulty Landlubber/Sailor/Captain, HUD element toggles).
- **Progress:** Rounds completed and best Pour each, collectibles found, upgrades, reales, free
  roam unlocked. *Continue* starts the next incomplete Round (or free roam); *New Game* asks
  before overwriting.
- **Tests:** round trip; a corrupt file → defaults + a toast; version migration.

### 9.17 Visual direction and the draw list
- **Look:** stylised golden-age Caribbean. Readable silhouettes, warm sunlight, saturated sea,
  painterly sky.
  - Default: style `lit`, outlines on characters and interactables only
    (`outlines_on = "objects"`), ACES, subtle bloom (~0.35), light haze, sun shafts in the
    jungle.
  - Selectable looks: *Painted Seas* (low-radius Kuwahara on the environment), Gull Sight,
    Memory Mist, Storm.
- **Palette tokens (data):**
  - Sea: deep `#0b3d5c`, mid `#12708a`, shallows `#3fc1c0`, foam `#f4f8f6`.
  - Land: sand `#e8d3a2`, wet sand `#b89e72`, grass `#6aa84f`, jungle `#2f6d3a`, palm trunk
    `#8b6b4a`, rock `#8c8577`, limestone `#cbbd9b`.
  - Materials: terracotta `#b5543a`, timber `#6b4a2f`, rope `#b99b6b`, brass `#c9a24a`, rum
    `#c9822b`.
  - Sails: white `#efe7d6`, black `#22201e`.
  - Temperance: navy `#2c3e66`, white `#e8e8ee`.
  - UI accent: red `#a8322d`.
- **Characters:** biped puppets with gear.
  - Add helm kinds: `tricorne`, `bicorne`, `bandana`, `straw_hat`, `beaked_hood` (a small
    `puppet.rs` gear change, drawn from primitives).
  - Captain Cask: beaked hood, coat tails (cape), brass-buckled belt, gloves, the Tap bracer
    (forearm accent part), cutlass (sword), flintlock props in hand when aiming.
  - Temperance guards: navy coats, white trousers, tricornes; muskets for musketeers.
  - Brutes: bigger proportions, axes. Officers: bicornes and epaulettes. Drye: white wig,
    bicorne, gold trim.
  - Civilians and crew: varied colours, straw hats, bandanas, striped shirts.
- **Draw list uses:** sails, rigging and grapple lines, telegraph decals, highlights, the last-known-
  position ghost, world markers, ballistic arc previews, wakes.
- **FX cues (`fx.toml` → bursts):**
  - Naval: muzzle flash (light + sparks) and wind-drifted smoke, cannonball streak, splashes
    (big/small), hull splinters, deck fire, sinking bubbles and debris, powder-keg explosion,
    mortar launch and impact, telegraph ring.
  - On foot: steel sparks, pistol flash and puff, dart trail, smoke-keg cloud, sand and dust
    footfalls.
  - Water and weather: bow spray and wake, rain streaks and splashes, lightning, a waterspout
    (rotating particle column + distortion).
  - Ambient life: gull flocks (visual boids), jungle leaf fall, dusk fireflies, torches.
  - Moments: the survey swirl (camera + gull fly-by), leap trail + splash/hay burst,
    collectible sparkle, Gull Sight pulse ring.

### 9.18 Performance budgets and quality
- **Engine work:**
  - CPU frustum culling per batch and per instance (bounding spheres) and per region.
  - Distance culling by category: small props 120 m, vegetation 180 m, structures 600 m+.
  - Two terrain LODs; ocean LOD.
  - Camera-following shadows; extended far plane.
  - `sim.history_seconds` 10 for this game.
- **Budgets (verify and record in BUILD_LOG):**
  - Sim ≥ 240 ticks/s headless in the dev build with the full level (ships, ~120 NPCs,
    perception).
  - ≤ 2,500 submitted instances after culling at High in town.
  - Ocean ≤ 120k vertices; typically ≤ 20k live particles.
  - Target 60 fps at 1080p on an RTX 3060 Ti natively and in Chrome.
- **Quality presets:**
  - Low: 0.5 MP, no GI/shafts/haze, half ocean grid, 50% vegetation, 1024 shadows.
  - Medium: the browser default.
  - High.
- **Checks:** `pav bench scene=blacktap ticks=1200`; `capture stats=only` at six viewpoints (helm at
  sea, town street, rooftop over town, fort courtyard, jungle, storm finale).

### 9.19 The Drydock (developer level) and agent tools
- **Scene `blacktap/drydock`**, also *Extras → Training* on the title menu: a compact island and
  lagoon with stations.
  - Each station has a sign (SDF text), an engine-style station guide (`[learn]`, H in developer
    mode), and pads that switch parameters.
  - Sea: Sailing lane (buoys, wind pads), Gunnery range (floating targets, a dummy hull), Duel
    buoy (AI schooner), Mortar yard, Boarding hulk, Pier (swim, ladders).
  - Traversal: Climbing wall (every climb feature), Haystack tower, Rope run.
  - Combat and stealth: Combat ring (a dummy of each archetype; slow-motion telegraph toggle),
    Tool range, Stealth yard (guards with debug cones, grass, whistle targets), Market (crowds),
    Poster wall (heat).
  - Presentation: Map table, Theatre (dialog and cinematics), Jukebox (every shanty, an
    ambience mixer, a sound-bank browser), Weather pads (each preset and sea state), UI Gallery
    (every screen and overlay with sample data), Camera bench (each context), Rumble pads.
- **New `pav` tools** (help ≤ 50 tokens, arguments ≤ 12 words, each tested; listed in `pav help`
  and MCP):
  - `ocean` (sample or set).
  - `ship` (status; `do=sail level=2 | steer deg= | fire side=port | board`).
  - `mission` (status; `goto=round/3`; `complete=step`).
  - `perception` (per-NPC state and meters; who sees the player).
  - `ui` (render a screen, HUD or overlay to PNG with sample or live data; `nav=down,down,confirm`
    scripts the focus).
  - `attract` (run the playlist N minutes headless: segments, durations, showcase events,
    stalls).
  - `tour` (the canonical vistas in one contact sheet).
  - `level_check` (all blacktap data references).
  - `levelmap` for the blacktap scene.
  - `capture` composites the game UI over the frame (`ui=true`), fixing the "HUD invisible to
    tools" gap.

### 9.20 The bot (in `blacktap`; it is the test player and the attract-mode player)
- **Layers:**
  - **Director:** current Round and step → goal.
  - **Planner:** a task stack with preconditions and fallbacks.
  - **Skills:** controllers that emit an `InputFrame`, including `look`, so the camera behaves
    like a player's.
- **Skills:**
  - **Sailing:** a coarse sea nav grid from seabed depth (avoid shallows), wind-aware headings
    with tacking, sail control, arrival tolerance.
  - **Broadside duel:** put the target on the beam at 60–100 m, look to that side, fire when the
    ballistic predictor says a hit is likely; brace on telegraphs; chain shot when close ahead;
    swivel on weak points; repair under 40% hull.
  - **Fort bombardment:** circle outside the mortar ring, take batteries in order, step out of
    telegraph rings.
  - **Boarding:** approach slowly, press Board.
  - **Melee:** pick targets; attack when no tell; counter on tells (read the enemy's attack state
    and window); pistol musketeers; dodge brutes; darts for groups.
  - **Ashore:** anchor, leave the helm, swim to a landing point. Follow **authored climb routes**
    (waypoints with actions: parkour up, climb to height, wrap, mantle). Survey and leap.
  - **Stealth:** paths on the 2.5D nav weighted by guard visibility (watched cells expensive,
    grass cheap); wait for patrol windows; whistle-lure into grass and assassinate from hiding;
    fall back to a fight.
  - **Interactions:** courier, Mother Molasses, flag, helm, flotsam.
  - **Recovery:** no progress for 6 s → jitter, jump, re-plan, or an alternate route.
- **Personalities:** `efficient` for tests (shortest path to a win); `showcase` for attract (air
  assassinations, leaps, full broadsides, chain shot, counters, Gull Sight pulses, dramatic
  looks).
- **Bot hints in data** (`[[bot_hint]]`: climb routes, perch approaches, landing spots, sea
  waypoints, stealth lanes, hiding spots) keep the bot simple and content-driven.
- **Tests:**
  - Each Round from its setup on 5 seeds (`efficient`).
  - The full level on 3 seeds (long test in `scripts/check.sh --full`).
  - `showcase` completes every attract segment without stalls on 5 seeds.

---

## 10. Controls

Controller-first: a PS5 DualSense (any standard-mapped pad works; glyphs follow the pad family).
Keyboard and mouse map onto the same model. Bindings live in `games/blacktap/bindings.toml`
(§8.4) and are remappable later without code. Prompts always show the glyph of the last-used
device.

### 10.1 On foot
| DualSense | Action | Keyboard / mouse |
|---|---|---|
| Left stick | Move (tilt: walk → run) | W A S D |
| Right stick | Camera | Mouse |
| R1 (hold) | **Free-run** (high profile) | Shift |
| ✕ | Jump · **Parkour Up** (with R1) · climb up · swim up · Leap of Spirits at a perch | Space |
| ◯ | Drop / let go · **Parkour Down** (with R1) · dodge (in combat) · dive (swimming) | C |
| □ | Attack · **Assassinate** (when the prompt shows) · hold: heavy / guard break | Left mouse |
| △ | **Counter** (on an enemy's tell) · interact (loot, helm, flag, free, poster) · hold behind a target: pickpocket · tag (in Gull Sight) | E |
| L2 (hold) | Aim pistol or tool | Right mouse |
| R2 | Fire / throw (aiming) · quick-shot (not aiming) | Left mouse (while aiming) · F |
| L1 (hold) | Tool wheel | Tab |
| R3 | **Gull Sight** | Q |
| L3 | Crouch / sneak (toggle) | Ctrl |
| D-pad ↑ | Grog Ration (heal) | R |
| D-pad ↓ | Whistle (lure) | G |
| D-pad ← / → | Previous / next tool | 1–4 |
| Touchpad (press) | Map (fallback: Create) | M |
| Options | Pause | Esc |

### 10.2 At the helm
| DualSense | Action | Keyboard / mouse |
|---|---|---|
| Left stick ↑ / ↓ | Raise / lower sails (Furled · Half · Full) | W / S |
| Left stick ← / → | Steer | A / D |
| R1 (hold) | Ramming speed | Shift |
| Right stick | Camera (look to a side to choose the weapon) | Mouse |
| L2 (hold) | Aim (show arcs) | Right mouse |
| R2 | Fire: broadside to the side you look at, chain shot ahead, Keg Drop astern | Left mouse |
| □ | Swivel gun (on a highlighted weak point) | F |
| △ | Brace · hold when slow: leave the helm | E |
| ✕ | Board (enemy disabled and close) | Space |
| L1 (hold) | Spyglass (R2 tags) | Z |
| R3 | Gull Sight | Q |
| D-pad ↑ | Repair (spends Timber) | R |
| D-pad ← / → | Previous / next shanty | ← / → |
| Touchpad / Options | Map / Pause | M / Esc |

### 10.3 Menus (every screen)
| DualSense | Action | Keyboard / mouse |
|---|---|---|
| D-pad / left stick | Move focus (repeat after 350 ms, then every 90 ms) | Arrows / W A S D · mouse hover |
| ✕ | Select | Enter / Space · left click |
| ◯ | Back | Esc / Backspace · right click |
| L1 / R1 | Previous / next tab | Q / E |
| △ | Context action (reset to default, legend) | Tab |
| Options | Close the pause menu | Esc |

---

## 11. UI and UX: the 10-foot front end

### 11.1 Rules (apply to every screen; check them in UI captures at 720p, 1080p and 4K)
- **Readable from a couch.** Design at 1080p and scale with screen height. Body text ≥ 28 px,
  labels ≥ 24 px, titles ≥ 56 px; high contrast (parchment on ink, or ink on parchment); no
  thin fonts below 32 px.
- **Safe area:** keep everything inside a 5% margin (TV overscan).
- **One focus, always visible.** Exactly one focused element, shown by scale, glow and an accent
  bar, with a sound on every move. Focus never lands on nothing; lists wrap; directional moves
  between columns go to the spatially nearest element.
- **≤ 7 items per list, one decision per screen.** No dense tables, no scrolling walls of text, no
  web-style forms. Settings use big rows with left/right value pickers and sliders.
- **Every action has a button hint** in a bottom bar ("✕ Select  ◯ Back  L1/R1 Tabs") using the
  last-used device's glyphs.
- **Mouse and keyboard map onto focus:** hover moves the focus, click confirms, Esc goes back. No
  pointer-only interactions. Drop the engine's virtual-cursor pad navigation for this game's
  screens.
- **The HUD is minimal and contextual.** Elements fade in when relevant and fade out after 4 s
  idle. Prompts appear only when an action is valid.
- **Every input gets feedback within a frame:** sound, motion, or both.

### 11.2 Visual language
- **Fonts (OFL, embedded with `include_bytes!`; licences in `assets/fonts/`):** a display face
  for titles, e.g. **Pirata One** or **IM Fell English SC**; a sturdy humanist sans for body and
  HUD, e.g. **Alegreya Sans** or **Fira Sans**. Register them in egui's `FontDefinitions` (and
  in the SDF world-text atlas if world signs should match). Download from the official
  google/fonts repository and commit the font files with `OFL.txt`.
- **Theme (data, `games/blacktap/ui/theme.toml`):** parchment `#f1e3c6`, ink `#2b1d12`, rum amber
  `#c9822b` (focus), sea teal `#1f6f78`, gold `#e2b04a`, danger red `#a8322d`; panels use torn-
  parchment edges and rope borders drawn with egui shapes (no images).
- **Glyphs are vector-drawn** (`pav_ui::glyph`): DualSense ✕ ◯ □ △, L1/R1/L2/R2, L3/R3, Options,
  Create, touchpad, sticks and D-pad; Xbox A B X Y, LB/RB/LT/RT, Menu/View; keyboard keycaps
  and mouse buttons. A setting can force a family.

### 11.3 Screens (front end in `pav_app`, drawn with `pav_ui`)
- **Title:** the live 3D scene behind (the *Black Tap* under sail at golden hour, slow orbit
  camera, crew singing), the logo (display font + a vector keg-tap emblem), "Press ✕ to start"
  pulsing (glyph by device), build ID small in a corner. Any input → main menu. Idle 10 s →
  attract.
- **Main menu:** Continue (when progress exists), New Game, Replay a Round (after first
  completion), Training (the Drydock), Settings, Credits & About, Quit (native only). Large
  vertical list, left-aligned over the live scene.
- **Settings:** tabs Controls, Camera, Audio, Display, Accessibility, Gameplay. Each row: label,
  value picker or slider, a one-line description at the bottom. △ resets the focused row; a
  "Defaults" row per tab. Changes apply live and persist.
- **Pause** (Options): Resume, Map, Objectives (current Round, steps, Full Pour checklist),
  Settings, Restart from Checkpoint, Quit to Title (confirm). The game is paused behind a
  darkened, blurred frame.
- **Map:** §9.15.
- **Overlays in play** (the game provides them as `UiModel.overlay`): the tool wheel (radial, 4
  slots), plunder and upgrade tiles (2–4 big tiles with icon, name, one-line effect, cost),
  survey reveal (icons pop onto a mini map), Last Words letterbox.
- **Results:** Tab title, Pour % ring, the Full Pour checklist with ticks, time, reales,
  collectibles x/y, assassinations, ships sunk, times detected; tiles Continue (Free Roam),
  Replay a Round, Title.
- **Credits & About:** the game's credits (engine: shardfall/Pavilion; fonts and licences;
  traditional shanties; procedural everything), plus the Source Information block (§1).
- **Confirm dialogs:** two big buttons, default focus on the safe choice.
- **Loading/transitions:** use the engine's screen transitions (iris with a keg-tap mask, or
  dissolve) instead of blank frames; the browser loader shows the title and progress.

### 11.4 HUD (from `Game::ui`, drawn by `pav_ui`)
- **Top left:** health as a segmented bar (and Heat poster icon when > 0); fades out at full
  health and out of combat.
- **Top centre:** the compass bar with markers (§9.15).
- **Top right:** the current objective (one line) + optional objective ticks when they change.
- **Bottom right:** contextual prompts (up to 3), e.g. "△ Take the helm", "□ Assassinate".
- **Bottom left at sea:** wind rose (wind vs heading, sail level pips), hull bar, weapon
  readiness (port/starboard/chain/kegs/swivel arcs around a small ship icon).
- **Bottom centre:** subtitles (speaker name in colour); boarding/fort progress bars when
  relevant.
- **In world:** awareness arcs over heads, enemy ship/battery bars, objective diamond with
  distance, telegraph rings, interaction glyphs on objects, ballistic arcs while aiming.
- **Full screen:** damage vignette, low-health heartbeat desaturation, Gull Sight grade,
  letterbox in cinematics, banners ("FORT CAPTURED", "TARGET TAPPED", "SPILLED") and toasts
  (collectibles, upgrades).

### 11.5 Onboarding
Teach by doing, one verb at a time, with Old Bung's lines and a big glyph prompt that stays
until the action is performed once (then never again in this save): sails, steer, spyglass,
broadside, brace, chain shot, board, counter, free-run, climb, Parkour Down, survey, leap,
Gull Sight, crouch in grass, whistle, assassinate. Prompts never cover the centre of the
screen.

### 11.6 Accessibility
Subtitles on by default (size S/M/L, speaker names, background box), colour-blind palettes for
Gull Sight and HUD, high-contrast HUD, camera shake and motion sliders, hold/toggle options,
invert axes and sensitivity, aim assist, show vision cones, three difficulties (Landlubber:
slower awareness, more health, wider counter window; Sailor; Captain: no regen in restricted
areas, tighter windows).

### 11.7 Verifying UI headlessly
`pav ui` and `capture ui=true` render screens and the live HUD through the same `pav_ui` code
the window uses, so every screen is checked with images: one contact sheet of every screen at
1080p, plus 720p and 4K spot checks for clipping and scale. Focus-navigation graphs are tested
in code (every focusable reachable; back always returns).

---

## 12. Attract mode, kiosk and mobile

### 12.1 Behaviour (must)
- **Trigger:** 10 s with no input on any front-end screen (title, main menu, settings, credits) →
  a short transition → attract. Not during play or the pause menu.
- **What it shows:** the AI playing the real game, on a loop of showcase segments, each a few
  seconds to a minute, chosen to show the core mechanics. Classic arcade framing: a
  **"DEMO PLAY"** badge, the logo in a corner, a pulsing **"PRESS ANY BUTTON"**, segment
  captions, letterboxed cinematic framing.
- **Exit:** any input → back to the title **immediately** (the same frame: cut, then a 0.2 s
  fade-in of the title), the triggering input consumed (it doesn't also press a menu button).
  "Any input" means any key, mouse button, wheel, mouse move over 8 px, gamepad button, stick
  over 0.35, trigger over 0.2, or (on desktop) touch.
- **Loop forever:** after the last segment, a 3 s title card ("Assassin's Keg — Black Tap ·
  PRESS ANY BUTTON"), then the playlist again with the next seed set, indefinitely, with no
  memory growth (every segment rebuilds its scene).

### 12.2 The playlist (`games/blacktap/demo.toml`)
| # | Segment | Start | Shows | Length |
|---|---|---|---|---|
| 1 | **Rule the Waves** | Round 2 setup | sailing, broadsides, chain shot, brace, boarding melee | ≤ 75 s |
| 2 | **Storm the Fort** | Round 3 setup | telegraphed mortars, batteries falling | ≤ 50 s |
| 3 | **Ashore** | Round 4 setup | leave helm, dive, swim, climb the chapel, survey, leap | ≤ 60 s |
| 4 | **Shadow of the Keg** | Round 5 setup | Gull Sight, grass, whistle, air and hiding assassinations, counters | ≤ 70 s |
| 5 | **Last Call** | Round 6 escape checkpoint | rooftop escape, dive, storm, waterspouts, frigate | ≤ 60 s |

Each segment: `scene = "blacktap/round/N"` (+ optional checkpoint id), seed list, time limit,
showcase personality, camera style (game play camera with director cuts: wider shots on idle
moments, close shots on kills), caption text, success condition (a showcase event count).

### 12.3 Implementation
- **App side (`pav_app`, engine, generic):** a `FrontEnd` state machine (Boot, Title, Menu,
  Attract, Play, Paused, Results); an idle timer reset by every raw input event; `TimeControl.
  autopilot = true` makes the sim host use `Sim::bot_input()` instead of the player's input each
  tick (native thread and the browser's frame-stepped host alike), keeping runs deterministic;
  a demo flag tells the game it is in attract (`Game` reads it from the scene name or a sim
  flag) so it picks the `showcase` personality and the demo camera; the UI shows the demo
  overlay; audio plays if unlocked.
- **Watchdog:** a segment ends early if no showcase progress for 25 s, the player dies, or the
  time limit passes; stalls are logged (a `demo_stall` line in the console and the `attract` tool
  report).
- **Headless check:** `pav attract minutes=30` runs the playlist faster than real time and
  reports each segment's duration, showcase events (e.g. broadsides fired, boardings,
  assassinations, leaps, counters) and stalls; tests assert zero stalls on 5 seed sets and every
  segment showing ≥ 3 distinct mechanics.

### 12.4 Mobile and kiosk
- **Phones and tablets** (`(pointer: coarse)` and no pad or keyboard input yet): boot straight
  into attract and stay there. Nothing on screen except a small **☰** button (top right) that
  opens a minimal overlay: Sound on/off (the tap unlocks audio), Quality, About, Close. Touching
  the game doesn't exit attract. No touch controls, tips, cards or fans: turn the engine's
  touch layer off for this game. Landscape lock and the "turn sideways" hint can stay.
- If a gamepad or keyboard is used on such a device, behave as desktop (go to the title).
- **Kiosk:** `?kiosk=1` / `--kiosk` starts in attract and returns to attract 60 s after the last
  input anywhere in the front end; for store displays.
- **Developer mode:** the shipped game hides every engine-demo affordance: boot diagnostics after
  boot, FPS stats, the hint bar, guide panel, room card, H station guide, bomb aim ring, the
  engine pause menu, F1–F10, F2 teleports, Backspace rewind, studios. `--dev` / `?dev=1`
  restores them (and lists engine scenes, the pavilion and Shardfall).

---

## 13. Build phases

Order matters: Phase 1 makes the whole product exist end to end (thin), so every later phase
deepens something already playable, testable and shippable. Each phase ends with: the exit
checks green, a BUILD_LOG entry, §14 ticks with evidence, a commit and a push.

**Parallel work:** after Phase 1 fixes the interfaces (hook v2 types, `UiModel`, `DrawList`,
`Atmosphere`, `AudioState`, `SimEvent::Cue`, the kit's module boundaries), these can run as
subagents in worktrees while the main thread drives the game, kit and bot: (a) `pav_audio` v2 +
sequencer + instruments (isolated; can start right after Phase 1), (b) the ocean and sky render
passes, (c) the structure generators, (d) `pav_ui` screens and glyphs, (e) the ship hull/sail
mesh generators. Integrate each with tests and captures before merging it.

### Phase 0 — Bootstrap
- Vendor the engine (§8.1), one commit. Run the engine's setup (the apt packages in
  `.claude/hooks/session-start.sh`, `cargo build --locked -p pav_tools --bin pav`,
  `rustup target add wasm32-unknown-unknown`), and extend that hook so future cloud sessions of
  this repo also get the wasm target and anything else you install.
- Smoke: `cargo test --locked -p pav_core --test games`, `pav play scene=starter bot=true
  seeds=3`, one capture.
- Create `docs/BUILD_LOG.md`, `ENGINE_UPSTREAM.md`, the README skeleton (Source Information
  first), and `scripts/check.sh` (fmt check, clippy `-D warnings` on the touched crates, tests
  of the touched crates and `blacktap`, `cargo clippy -p pav_app --target
  wasm32-unknown-unknown`; `--full` adds the long bot and attract runs).
- Record your model and effort from `get_session` in BUILD_LOG and the README.
- Push; open the draft PR; subscribe to its activity.
- **Exit:** the engine builds and tests here; the PR is open.

### Phase 1 — Walking skeleton (the whole product, thin)
- **Engine plumbing:** runtime game registration; crates `pav_ui`, `pav_kit`, `blacktap`
  (registered scene `blacktap`: a placeholder sea plane, one blocky island, one objective, a
  bot that completes it); hook v2 types and plumbing through `RenderFrame.play` to the view, app
  and tools (defaults everywhere); `InputFrame.look/triggers`; data-driven bindings with glyph
  lookup; `bombs.weapon = "none"`; zone events before `post`; `SimEvent::Cue` with stub banks;
  developer/kiosk gating; Assassin's Keg branding when it is the start scene.
- **`pav_ui` v0:** theme, embedded fonts, focus navigation, glyph painter, bottom hint bar;
  Title, Main menu, Settings (a few real options), Pause, Results stub; HUD v0 (objective line,
  prompts). `capture ui=true` and `pav ui` render it headlessly.
- **Front end + attract v0:** the state machine; title idle 10 s → attract (placeholder playlist
  with the bot via `TimeControl.autopilot`) → any input → title; mobile boots into attract with
  only ☰; persistence v0 (settings).
- **Ship the skeleton:** `scripts/build-web.sh --single-file --scene blacktap` with the title,
  description, theme colour and icon; `scripts/browser_check.mjs` (Playwright, SwiftShader
  flags) proves desktop (title → 11 s idle → DEMO → key → title) and phone emulation (attract +
  ☰ only).
- **Exit:** all of that shown by tests, captures and browser screenshots; `check.sh` green.

### Phase 2 — World tech and the greybox level
- Ocean (CPU function, buoyancy, swimming in waves, LOD renderer); atmosphere (sky pass, fog
  along the view ray, sun/ambient colours, presets and blends, rain, lightning, wind on
  particles); islands (generator, heightfield collider, terrain shading v1, vegetation scatter,
  palm family); structure generators at greybox fidelity (every kind in §9.4) and new
  materials; level format (`[[island]]`, `[[structure]]`, `[[bot_hint]]`, routes, named
  markers); culling, LOD, far plane, camera-following shadows; camera director v1 (orbit, spring
  arm, foot/swim/helm contexts); 2.5D nav v1; tools `ocean`, `levelmap` (blacktap),
  `level_check`, `tour`.
- **Greybox all of Tab I** (§5.1): islands, town, fort, docks, perches, cave, roads, collectible
  spots, patrol routes.
- **Exit:** walk, climb-less parkour and swim across the whole map; a tour sheet of every
  location; `level_check` clean; a first perf probe recorded.

### Phase 3 — Ships and naval combat
- Vessels and visuals (hull generator, sails, rigging), helm/seat/deck, ship camera contexts,
  weapons and ballistics, aim arcs and assist, damage, fire, sinking, repair, brace and
  telegraphs, AI captains, forts (batteries, mortar rings), boarding (transition, arena,
  plunder/upgrade overlay), flotsam, spyglass and tagging; `ship` tool.
- Mission framework v1 and checkpoints; wire Rounds 1–3; the bot's sea skills.
- **Exit:** the bot completes R1–R3 from their setups on 5 seeds; battle filmstrips reviewed.

### Phase 4 — On foot
- Traversal (§9.8 entire), new helm/gear kinds, clip choices; vitals, melee, assassinations,
  pistols, tools and the wheel, feel via `FrameEffects`; perception, heat and posters, brains,
  civilians and crowds; Gull Sight highlights and tagging.
- Wire Rounds 4–6; the bot's land skills (climb routes, stealth lanes, combat).
- **Exit:** the bot completes R4–R6 from their setups on 5 seeds; traversal, combat and
  perception tests green; filmstrips of climbing, assassinations and a leap reviewed.

### Phase 5 — Content complete
- Every Round fully scripted: dialog and subtitles, cinematics (boarding, Fort Captured, Last
  Words, Tab Complete), optional objectives, checkpoints; all collectibles placed; survey
  reveals and the map; upgrades; results; free roam (convoys, contract generator, the tavern);
  Replay a Round; Continue / New Game with saved progress; difficulties.
- **Exit:** `check.sh --full` green (full level on 3 seeds); per-Round timing table in BUILD_LOG;
  no placeholder content left.

### Phase 6 — Audio
- `pav_audio` v2 (bank, held voices, filters, reverb, buses, pools), sequencer + ABC parser +
  instruments + crew choir, ambience layers; game data (`sounds.toml`, `music.toml`,
  `shanties/*.abc`, `ambience.toml`); positional audio; voice blips; UI sounds; haptics (native
  and web); audio settings; the Jukebox station. Optional and recommended: `audio_capture
  spectrogram=true` writes a PNG so you can *look* at the music's structure.
- **Exit:** audio tests green; per-scene bus RMS and peak recorded; spectrograms reviewed.

### Phase 7 — UI and UX complete
- Every screen, overlay and HUD element final; onboarding; accessibility options working;
  glyph families; persistence; credits/about with Source Information.
- **Exit:** UI contact sheets at 720p/1080p/4K meet §11.1; focus-graph tests green.

### Phase 8 — Attract mode final
- The 5-segment playlist with the showcase personality, director camera and captions;
  watchdog; kiosk flag; mobile behaviour final.
- **Exit:** `pav attract minutes=30` has zero stalls on 5 seed sets with ≥ 3 mechanics per
  segment; browser checks (desktop and phone emulation) pass.

### Phase 9 — Art, VFX and performance polish
- Character looks, ship detail and damage states, building and fort detail, vegetation variety,
  the full VFX list, looks (Painted Seas, storm, mist), lighting per Round; culling/LOD tuning;
  quality presets; the Drydock with every station and guide.
- **Exit:** a 12+ vista tour sheet passes the art checklist (§15.4); perf budgets met and
  recorded.

### Phase 10 — Ship it
- `check.sh` and `check.sh --full` green; the single-file HTML built into `dist/` and
  browser-verified (desktop + phone emulation, no console errors); optional Windows `.exe`
  (`scripts/build-windows.sh`, `scripts/smoke-windows.sh`) if time allows.
- README complete; `docs/ENGINE_ANALYSIS.md`; `docs/CONTENT_GUIDE.md`; `docs/PROGRESS.md`
  rewritten for this repository (≤ 60 lines, `docs_limits.rs`); `ENGINE_UPSTREAM.md` complete;
  BUILD_LOG final; PR description final (summary, how to play, how to verify).
- Final chat message (§16.4).

---

## 14. Definition of Done

Tick each box only with evidence (test name, tool output, capture path) recorded in BUILD_LOG.
The goal is complete when every box is ticked.

**Status (2026-10-17): every box ticked.** The evidence for each, box by box, is the table
"§14 evidence map" in `docs/BUILD_LOG.md` (entry "Final verification").

**A. Identity and deliverables**
- [x] Title, ID and naming per §2 in the README, the HTML `<title>`, the About screen and the PR.
- [x] README complete: Source Information (§1, with the executor's confirmed model and effort),
      how to play, controls, build/run, features, credits, links to the docs.
- [x] `docs/ENGINE_ANALYSIS.md`: used, not used, what was added (files), source you had to read
      and why, wrong or missing docs, bugs found, **the one change**.
- [x] `docs/CONTENT_GUIDE.md`: how to add each content kind (§16.3).
- [x] `dist/<name>.html` (§2): one self-contained file (aim ≤ 25 MB) that opens from `file://` in
      Chrome/Edge; desktop and phone-emulation screenshots referenced in BUILD_LOG.
- [x] The final chat message per §16.4.

**B. Engine platform**
- [x] Engine vendored at `0098196`; `ENGINE_UPSTREAM.md` lists every engine change.
- [x] Runtime game registration; the game in `crates/blacktap`; kit in `crates/pav_kit`; UI kit in
      `crates/pav_ui`.
- [x] Hook v2 (camera, ui, draw, atmosphere, audio, effects, tunables, front_end,
      progress/restore) reaches view, app and tools; the starter still passes its tests.
- [x] `InputFrame.look/triggers`; data-driven bindings; DualSense, Xbox and keyboard glyphs.
- [x] `SimEvent::Cue` with data-driven sound and FX banks.
- [x] Zone events reach games; `bombs.weapon = "none"`.
- [x] Persistence API (native file + browser localStorage) for settings and progress.
- [x] Checkpoint save/load outside `SimState`.
- [x] Developer/kiosk gating; branding.
- [x] Culling, LOD, far plane, camera-following shadows, quality presets.

**C. World**
- [x] Ocean (CPU function + GPU renderer), buoyancy, swimming in waves.
- [x] Atmosphere presets and transitions: sky, clouds, fog, rain, lightning, wind.
- [x] Islands: heightfield, collider, terrain shading, vegetation incl. palms and stalk grass.
- [x] Structure generators (every kind in §9.4) and the new materials.
- [x] The full Tab I layout (every location in §5.1).
- [x] 2.5D navigation.

**D. Sea**
- [x] Sailing (wind, sails, turning, grounding, collisions); helm, seat, deck; leave and return.
- [x] Broadside, chain shot, swivel, Keg Drop, ram; aim arcs and assist; ballistics with impacts.
- [x] Damage, fire, disabled, sinking, repair; brace and telegraphs.
- [x] AI captains: gunboat, schooner, frigate, convoy.
- [x] Forts: batteries and the mortar tower with telegraph rings.
- [x] Boarding: transition, arena, victory, plunder and upgrade.
- [x] Spyglass and tagging; flotsam.

**E. Land**
- [x] Free-run, climb (walls, cliffs, poles), corners, mantle, back-eject, ledge safety, jump
      assist, beams.
- [x] Perches and survey (map reveal), Leap of Spirits, fall damage, diving.
- [x] Vitals; melee with tokens, tells, counters and finishers; the five assassinations; pistols;
      tools (wheel, both darts, Smoke Keg, Bung Line).
- [x] Perception (sight, hearing, concealment, states, alerts, bells, bodies); heat and posters.
- [x] Brains for every archetype; civilians and crowds.
- [x] Gull Sight with through-wall highlights and colour-blind palettes.

**F. Content**
- [x] Six Rounds playable start to finish: objectives, optional objectives, checkpoints, dialog
      and subtitles, cinematics.
- [x] Collectibles placed (4 sheets, 5 bottles, 8 chests, 3 Keg Taps, 6 flotsam); survey reveals;
      map; upgrades; results; free roam (convoys, contract, tavern); Replay a Round; Continue and
      New Game.
- [x] Three difficulties.
- [x] The Drydock with every station.

**G. Presentation**
- [x] Characters and gear (§9.17); ships, buildings, terrain, sky and ocean pass the art checklist
      in the tour sheet.
- [x] The VFX list (§9.17) complete.
- [x] Audio: sound bank, ambience, music (≥ 6 shanties + an original title theme + state music),
      voice blips, UI sounds; haptics.
- [x] 10-foot UI: every screen (§11.3), the HUD (§11.4), onboarding, accessibility, checked in UI
      captures at three resolutions.

**H. Attract mode and platforms**
- [x] Attract per §12.1: 10 s idle on the front end, any input exits the same frame, loops forever;
      the 5-segment playlist; the watchdog.
- [x] `pav attract minutes=30`: zero stalls on 5 seed sets.
- [x] Mobile: attract-only with the ☰ overlay, verified in phone emulation.
- [x] Kiosk flag.

**I. Quality gates**
- [x] `scripts/check.sh` green (fmt, clippy `-D warnings` on touched crates, tests, wasm clippy).
- [x] `scripts/check.sh --full` green (each Round 5 seeds, full level 3 seeds, attract run).
- [x] Determinism: snapshot/rewind/replay exact in a sea battle, a stealth section and a boarding.
- [x] Performance budgets (§9.18) met and recorded.
- [x] Release build: no placeholder text, no engine debug overlays, no browser console errors.

---

## 15. Verification and QA

### 15.1 The gate: `scripts/check.sh`
```sh
cargo fmt --all --check
cargo clippy --locked -p pav_core -p pav_kit -p blacktap -p pav_ui -p pav_view -p pav_audio \
  -p pav_tools -p pav_app --all-targets -- -D warnings      # trim to touched crates while iterating
cargo clippy --locked -p pav_app --target wasm32-unknown-unknown -- -D warnings
cargo test --locked -p pav_core -p pav_kit -p blacktap -p pav_ui -p pav_audio -p pav_tools
scripts/check-wgsl.sh                                      # after any WGSL change
# --full: cargo test --locked -p blacktap -- --ignored (each Round × 5 seeds, full level × 3,
#         attract 30 min headless, determinism suites)
```
Run the fast part before every push; run `--full` at the end of Phases 3, 4, 5, 8 and 10.

### 15.2 Test suites to write
- `crates/blacktap/tests/`: `rounds.rs` (each Round from setup, bot wins, 5 seeds; standing still
  loses), `full_level.rs` (`#[ignore]`: the whole Tab on 3 seeds with a per-Round timing table),
  `determinism.rs` (snapshot file round trip, lockstep stepping, `rewind_to`, replay hash — in a
  sea battle, a stealth section and a boarding), `attract.rs` (idle timer, any-input exit,
  playlist loop, watchdog, showcase coverage), `ui.rs` (focus graphs: all focusable reachable,
  back always returns; each screen renders), `data.rs` (`level_check` clean; every cue, line,
  song, mission reference resolves), `persist.rs`.
- `crates/pav_core/tests/`: `traversal.rs`, `ocean.rs`, `island.rs`, `structures.rs`, `nav25.rs`,
  `registry.rs` (runtime registration + `GameBox` deserialisation), plus extensions of
  `movement.rs` for swimming in waves.
- `crates/pav_kit/tests/`: one per module (vitals, melee, perception, brains, naval, ballistics,
  mission, dialog, cinematic, heat, inventory).
- `crates/pav_tools/tests/`: one per new tool; `capture ui=true` renders the HUD.
- `crates/pav_audio/tests/`: bank parsing, sequencer timing, ABC parsing, offline render metrics.
- `crates/pav_view/tests/`: atmosphere and ocean parameters reach the renderer.

### 15.3 Looking at the game (do it; don't skip)
- After each phase: `pav tour` (one contact sheet of the canonical vistas), `see` on busy scenes,
  filmstrips for motion (climb, corner wrap, mantle, leap, boarding swing, counter-kill, air
  assassination, broadside volley, sinking), `camera_bench` for camera contexts, `pav ui` sheets
  for every screen. Look at every image; note problems in BUILD_LOG; fix the obvious ones now.
- Watch the bot: in `pav repl`, alternate `play bot=true ticks=60` and `capture` through a Round
  to see what a player would see.

### 15.4 Art checklist (judge on the tour sheet)
Silhouettes read at the gameplay camera; the horizon is visible and clean (fog, no far-plane
cut); the sea has swell, foam and turquoise shallows; the sky has a sun and clouds; ships have
hull shape, masts, billowing sails and rigging; the town has colour variety, flat roofs with
parapets, balconies, awnings and laundry lines; the fort reads as stone with crenellations; the
jungle has palms and undergrowth; characters are distinguishable by faction at a glance (hood,
tricorne, coats); nothing floats or z-fights; no engine-demo text visible.

### 15.5 Browser checks (`scripts/browser_check.mjs`, Playwright)
Launch Chromium with `--enable-unsafe-webgpu --enable-features=Vulkan --use-vulkan=swiftshader
--use-angle=swiftshader --ignore-gpu-blocklist` (the only flag set that renders here; SwiftShader
draws ~0.5 fps, so allow generous waits; timers are wall-clock).
- Desktop (960×540): open `dist/…html` from `file://` → the title appears → wait 11 s untouched →
  screenshot shows the DEMO PLAY overlay over gameplay → press a key → the title is back within
  the next frames → screenshot. Scan the console for errors.
- Phone (844×390, `isMobile`, `hasTouch`): boots into attract → only ☰ visible → tap ☰ →
  overlay → screenshot.
Keep the screenshots under `out/` (git-ignored) and cite them in BUILD_LOG.

### 15.6 Performance and determinism
- `pav bench scene=blacktap ticks=1200` (≥ 240 ticks/s) and `capture stats=only` at the six
  viewpoints (§9.18) after Phases 2, 5 and 9; record the numbers.
- `record_save` / `replay` on a sea battle and a stealth section (the hash must match).

---

## 16. Deliverables and reporting

### 16.1 README.md (game first)
1. Title, tagline, `[AssassinsKegBlackTap-W9]`, one-paragraph pitch.
2. **Source Information** block (§1), with the executor's confirmed model and effort.
3. How to play: the HTML file (Chrome/Edge with WebGPU), the native build (`cargo run -p pav_app
   -- --scene blacktap`), controls tables (§10), what's in the slice.
4. Building and testing: setup, `scripts/check.sh`, the web build command, tools for agents.
5. Features and systems (short list linking to `PLAN.md` sections).
6. Credits: engine (shardfall/Pavilion, `ENGINE_UPSTREAM.md`), fonts (OFL), traditional shanties,
   "everything else procedural".
7. Links: `docs/ENGINE_ANALYSIS.md`, `docs/CONTENT_GUIDE.md`, `docs/BUILD_LOG.md`, `PLAN.md`.

### 16.2 `docs/ENGINE_ANALYSIS.md` (separate from the Source Information)
Written for an engine agent who has never seen this game (the format `docs/MAKE_A_GAME.md`
"Report back" asks for, extended by the user's questions):
- **What I used** (engine parts, with how much they carried).
- **What I didn't use** and why.
- **What I built that the engine should have had** (file pointers; ranked by how much output it
  saved).
- **Engine source I had to read and why.**
- **Wrong or missing docs; bugs found.**
- **If I could change one thing about the engine, it would be…** (one change, argued).
- Rows for the engine's `docs/FEEDBACK.md` tally.

### 16.3 `docs/CONTENT_GUIDE.md` (for the next agent: "the rest of the game is content")
How to add, with a worked example each: an island, a structure placement and a new structure
kind, a ship class, an NPC archetype and route, a mission (Round) with optional objectives and
checkpoints, dialog and barks, a cinematic, a shanty (ABC) and a music state, a sound cue and an
FX cue, an atmosphere preset, a collectible kind, an upgrade, a bot hint, an attract segment, a
Drydock station; plus how to verify each with the tools.

### 16.4 The final chat message (template)
1. **What was built** (5–8 lines) and **how to play** (the HTML path in `dist/`, the native
   command, the main controls).
2. **Source Information** — Initial Date, SOURCE, GAME ENGINE, Environment, Model and Effort
   Level (confirmed via `get_session`; never guessed).
3. **Game Engine Analysis** — used / not used / the one thing to change (a short version of
   `docs/ENGINE_ANALYSIS.md`), kept separate from the Source Information.
4. Known limitations and where the next content should go.
5. The PR link.

---

## 17. Risks, scope guards and decisions already made

### 17.1 Risks
| Risk | Mitigation |
|---|---|
| Scope is large | Walking skeleton first; every system thin before deep; data over code; subagents for isolated crates; the DoD is the scope — nothing beyond it until it is all ticked |
| Low camera exposes crude art | Fog and horizon early (Phase 2); silhouettes before detail; the art checklist each phase |
| No culling/LOD today | Culling and LOD land in Phase 2 with the greybox, measured with `capture stats=only` |
| Free-climb destabilises the controller | A separate `WallClimb` state gated everywhere `climbing \|\| hanging` is; `traversal.rs` + existing `movement.rs`/`gameplay.rs` must stay green |
| Characters on moving decks | Decks move by yaw and heave only; walking allowed only when moored or boarding (static) |
| The bot can't finish a Round | Bot skills grow with each system; authored bot hints; per-Round setups and tests from Phase 3 |
| Rebuild times | Game and kit outside `pav_core`; `cargo check` loops; balance in data |
| Memory from history clones | Big data behind `Arc` + `serde(skip)`; `history_seconds` 10 |
| Browser audio locked | Documented hint on the title; native unaffected |
| SwiftShader too slow for browser runs | Generous waits; logic is checked headlessly; the browser run only proves boot, attract and exits |
| Disk allowance | Clean stale `target/` artifacts; don't keep multiple dist builds |
| Context compaction mid-task | `docs/BUILD_LOG.md` + §14 ticks + commits are the memory; re-read them after any compaction |

### 17.2 Scope guards (don't build these)
Co-op or multiplayer; a full open world or more levels than Tab I + the Drydock; imported meshes,
images or audio files (fonts excepted); skinned meshes; a character creator; an economy beyond
the four upgrades; hunting, diving bells, fleet management; a modern-day frame story; voice
acting (subtitles + voice blips instead); localisation beyond English (but keep strings in
data); mobile touch controls (mobile is attract-only by design).

### 17.3 Decisions already made (change only with a logged reason)
1. Vendor the engine at the repository root (paths and tools work unchanged).
2. Game in `crates/blacktap`, reusable systems in `crates/pav_kit`, UI kit in `crates/pav_ui`;
   runtime registration in `pav_core::games`.
3. Hook v2 returns presentation as plain data in the frame; the HUD renders in headless captures.
4. A true third-person orbit camera owned by the game (look input in `InputFrame`), with a spring
   arm and context presets.
5. Ocean = a deterministic Gerstner function shared by gameplay and an LOD GPU renderer.
6. Islands = heightmap generator + heightfield colliders; buildings = parametric generators of
   axis-aligned blocks (flat Spanish-colonial roofs make parkour reliable).
7. Ships are kinematic kit vessels driven from the game's `pre`, not engine vehicles; decks are
   level physically (yaw + heave) and tilt visually.
8. Cannonballs use kit ballistics, not engine projectiles.
9. NPCs use kit perception and brains, never the engine's `Guard` (it resets the player on sight).
10. Checkpoints are in-memory snapshots outside `SimState`; disk saves hold progress and settings.
11. Music is sequenced synthesis from ABC notation; traditional shanties + an original theme.
12. Attract mode uses the real game and the real bot (showcase personality) via the sim host's
    autopilot input; mobile is attract-only.
13. Family-friendly tone: puns, no blood, alcohol as a running joke only.
14. Don't push to `michaelcrosato/ai-hub`; deliver the HTML in `dist/` named for it.

---

## Appendix A: the 50 titles and the pick

Generated in the planning session (2026-10-17). Each is a playful riff on *Assassin's Creed IV:
Black Flag*; none uses the trademark verbatim. The pick and the ID were drawn with Python's
`secrets` module (`secrets.randbelow(50)`; two `secrets.choice` draws from `A–Z0–9`).

1. Assassin's Greed: Black Flagon
2. Assassin's Screed: Black Flak
3. Assassin's Reed: Black Flute
4. Assassin's Steed: Black Foal
5. Assassin's Seed: Black Fig
6. Assassin's Creek: Black Frog
7. Assassin's Crud: Black Fog
8. Assassin's Crew: Black Flagon Brigade
9. Assassin's Keel: Black Fleet
10. Assassin's Squid: Black Ink
11. Molassassin's Creed: Black Treacle
12. Brassassin's Creed: Black Flask
13. Lassassin's Creed: Black Flagstaff
14. Cutlassin's Creed: Black Flagon
15. Corsair's Creed: Black Swag
16. Buccaneer's Creed: Black Flapjack
17. Kraken's Creed: Black Tentacle
18. Parrot's Creed: Black Feather
19. Barnacle's Creed: Black Hull
20. Scallywag's Creed: Black Swagger
21. Plunderer's Creed: Black Flagrant
22. Swashbuckler's Creed: Black Buckle
23. Hassle-in's Creed: Black Flap
24. A Saucy Sin's Creed: Black Flag
25. A Sassy Sin's Creed: Black Flagon
26. Assassin's Creaked: Black Plank
27. Assassin's Credo: Black Flamingo
28. Assassin's Brine: Black Flotsam
29. Assassin's Grog: Black Fogbank
30. Assassin's Shanty: Black Chorus
31. Assassin's Squall: Black Gale
32. Assassin's Seas: Bleak Flag
33. Assassin's Crate: Back Flag
34. Assassin's Bread: Black Flatbread
35. Assassin's Cheese: Black Flan
36. Assassin's Knead: Black Bagel
37. Assassin's Feed: Black Fishsticks
38. Assassin's Plead: Black Flagstaff Trial
39. Assassin's Freed: Black Flagship
40. Assassin's Weave: Black Flannel
41. Assassin's Treed: Black Palm
42. Assassin's Speed: Black Flash
43. Assassin's Credit: Black Doubloon
44. Assassin's Crease: Black Fold
45. **Assassin's Keg: Black Tap** ← picked
46. Assassin's Cleat: Black Knot
47. Assassin's Gleam: Black Pearl-ish
48. Rapscallion's Creed: Black Flag
49. Assassin's Wreck: Black Flotilla
50. Assassin's Gull: Black Flock

Output of the draw: `PICK #45: Assassin's Keg: Black Tap` · `ID: [AssassinsKegBlackTap-W9]`.

---

## Appendix B: engine pointers (where to hook)

From the planning survey of shardfall `0098196`. Line numbers drift as you edit; search for the
named item when they do. Paths are relative to `crates/`.

**`pav_core/src`**
- `games/mod.rs`: `Game` trait 43–91 · `Entry`/`entry` · `level_text` · `GameBox` 125–190 ·
  `DynGame` 193–241 · `play_pre`/`play_post` 259–270 · `run_bot`/`bot_input` 254–288.
- `games/starter.rs` + `starter.toml`: the template (rules from `[game]`, markers, nav flow
  fields, bot).
- `sim.rs`: `SimState` 100–147 · `spawn` 259–344 · `spawn_npc` 381 · `step` 488 · `step_inner`
  521–727 (order: hit-stop dt 524 → behaviours 531 → `vehicle_interact` 534 (raw input) →
  Shardfall pre + `play_pre` 538–539 → characters 549–592 → `update_guards` 596 → actions
  597–622 → `drive_vehicles` 623 (raw input) → physics 626–628 → health 669 → projectiles
  672–708 → `play_post` 710–712 → focus = player 714–716 → `update_zones` 717) · `explode`
  753–828 · `save_state`/`load_state` 874–889 · `frame()` 941–1060 (HUD lines 986, hidden rider
  998, Shardfall tint 1003).
- `input.rs`: button bits 7–46 (JUMP 0, CROUCH 1, INTERACT 2, USE 3, FOCUS 4, SPRINT 5 unused,
  PRIMARY 6, SECONDARY 7, CRAWL 8, SKILL3–6 9–12, DODGE 13, POTION 14) · `InputFrame` 48–67.
- `character.rs`: capsule 57, 65–71 · `MovementParams` 78–200 (jump 139–145, `ledge_grab` 150) ·
  `Character` 299–383 (`riding` 361–363, levers 364–382) · `find_ledge` 429–467 · `tick` 471–1281
  (riding 494–512, hazards 546–590, zones 592–604, swim 597–606, posture 612–648, ladder 652–688,
  ledge 690–768, horizontal 770–879, wading 777, swim speed 785–787, dodge 832–837, swim vertical
  881–898, jump/gravity 899–924, controller 959–1026, landing event 1063–1066, bombs/blaster on
  PRIMARY/USE 1109–1149).
- `vehicle.rs`: kinds 18–25 · def/preset/tunables 28–131 · state 135–153 · view 157–186 · spawn
  195–264 · interact 304–327 (3.2 m) · enter/exit 340–374 · drive 385–493.
- `stealth.rs`: guard sight + "SPOTTED" respawn 25–98 · cone fan 101–123.
- `ai.rs`: brain kinds 12–53 · `Ai::new` 192 · `wait` 201 · think 221–324 (no pathfinding).
- `nav.rs`: `for_room` 41–46 · floor/obstacle rules 89–131 · A* 216–267 · flow fields 270–324.
- `course.rs`: `say` 139–141 · `respawn_player` 152–182 · `hit_character` 186–218 · cues/view
  overrides cleared on room change 221–231 · zone reactions 299–461 · camera cues 434–453.
- `zones.rs`: kinds 20–42 · labels 141–164. `statics.rs`: chunk size 20 · unused LADDER flag 26 ·
  regions 38–46 · `BlockRef::from_tag` 400–406 · block colliders 409–453 · `add_zone_to` 473 ·
  labels 481–497 · `add_decor` 519 · `add_prop_to` 532–545 · `set_terrain` 566–575.
- `terrain.rs`: noise/terraces 134–154 · mesh 164–230 · vegetation 233–339 · `TerrainPatch` +
  collider 343–387. `level.rs`: `Piece` 63–92 · layers 312–320 · merging 380–520 · ladders
  409–423 · water merge 466–485, 521–550. `room.rs`: fields 291–360 · asset/vehicle limits
  478–482.
- `projectile.rs`: struct 13–29 · hits 92–143 · `only_fixed` 154. `health.rs`: hit sphere 38 ·
  boss bar 70–78. `destruct.rs`: `zone_props` 158–201 (add buoyancy here).
- `entity.rs`: `Behavior` 37–67 · `Entity` 306–347 · `Spawn` 407–504 · despawn/`set_position`
  432–485. `frame.rs`: `SimEvent` 62–211 · `HudFrame` 233–256 · `RenderFrame` 272–302 (`game`
  294 is Shardfall-only).
- `puppet.rs`: gear 79–113 · body plans 139–148 · look 152–238 · `preset` 342 · topple 521–528 ·
  climb phase 604–606 · `play_clip`/`set_action` 695–752. `clips.rs`: flags 27–40 · `death_clip`
  574. `history.rs`: cadence 22–26. `params.rs`: `Tunable` 64–76.
- `world.rs`: vehicle spawn 422–435 · health attach 459 · standalone skips streaming 772–774 ·
  rotation locks 1079–1090 · `raycast` 1176. `scenes.rs`: `build_standalone_room` 42–77 · scene
  resolution 109–164. `prop_instance.rs`: `spawn_prop` 242–254 · `restyle_prop` 271–293.
- `arpg/mod.rs` (patterns only): input masking 839–842 · driving moves and deaths 896–968 · feel
  (hit-stop, shake, gibs) 1077–1237.
- `build.rs` (crate root): embeds `rooms/`, `game/`, `anim/`, materials — copy the pattern for
  `games/blacktap/`.

**`pav_view/src`**
- `camera.rs`: params/presets 10–73 (incl. "close 3rd person") · rig 75–216 · eye 159–161 · far
  plane 174.
- `build.rs`: light settings 37–65 · shadow radius 52, 63 · cutaway 67–104, 1103–1127 (`front_cut`
  under 30° tilt, `height_cut`) · `ViewSettings` 346–541 · fog 442–444, 530–531, 1345–1351 ·
  interpolation jump limit 671–672 · events → effects 721–845 · water mesh 866–898 · hard-coded
  sun colour 1078 · sky = clear colour 1084 · puppet lights 1370 · vehicles 1415–1423 · terrain
  2143–2154.
- `water.rs` (ripple pools; size cap 53–56) · `assets.rs` (`emit_asset` 110–136; scope 82–95) ·
  `fx.rs` (presets 14–229; event bursts 291–480) · `vehicles.rs` 52–201 · `looks.toml`.

**`pav_render/src`**: `renderer.rs` (shadow map 16, 64 lights 18, light pick 1053–1063, instance
submit 1154–1205 — add culling here, clear colour 1344–1354) · `scene.rs` (ambient constants 299,
`outline_color` 338) · `shaders/scene.wgsl` (wind sway 52–86, shadows 141–161, fog 274–279) ·
`shaders/post.wgsl` (haze/shafts 178–255, Kuwahara 443–516) · `shadows.rs` 9–10 · `fx.rs` 13 ·
`text.rs` (font atlas 96–104, `Text3d` 202–218).

**`pav_audio/src/lib.rs`**: oscillators 10–17 · patch 20–38 · voice render 74–133 · mixer 143–155 ·
`sounds_for` 210–481 · pan/attenuation 517–529 · offline render 532–578 · cpal output 586–669.

**`pav_app/src`**
- `app.rs`: native/wasm loops 33–53 · `AppSettings` 56–73 · egui context 218 · touch 294–312 ·
  window title 355 · `finish_init` 395–534 · pad script 507–519 · pad virtual cursor 536–634 ·
  `load_scene` 642–661 · `set_menu` pauses 764–775 · `handle_key` 778–945 · `frame()` 947–1786
  (pad camera 974–987, mode switching 1024–1130, Shardfall camera 1084–1113, input assembly
  1146–1214, aim 1183–1195, audio events 1227–1236, bomb aim ring 1277–1302, guide panel
  1338–1339/1420, UI closure 1409–1547, Shardfall HUD 1423–1429, hint bar 1535–1546) · audio
  resume 2097–2106.
- `simhost.rs`: `TimeControl` 21–33 · `Shared` 65–79 · sim thread 121–161 · wasm host 163–189 ·
  publish 216–231 · clock 256–396 · `tick_once` 268–287 (**autopilot hook point**).
- `hud.rs`: `CameraDirector` 57–115 · `game_lines` 134–145 · `course_hud` 148–225.
- `input.rs`: default keys 29–42 · Shardfall bindings 111–129 · room overrides 132–161 ·
  `PadScript` 299–380 · gilrs 389 · first pad only 470 · trigger threshold 488 · pad layout
  515–549 · system pad buttons 617–619.
- `ui.rs`: pause menu 167–270 · scene list 248–257 · touch style 282–306 · touch tips 327–362 ·
  touch menu 378–476. `touch.rs`: covered view 148–149, draw 401–478. `platform.rs`: touch
  detection 44–53. `quality.rs`: presets 31–37, trim 41–61, dynamic resolution 64–125.
  `save.rs` (save/prefs pattern) · `settings.rs` (URL/CLI 62–91, file keys 181–201) · `rooms.rs`
  (room camera 48–77, room card 93–167).

**`pav_tools/src`**: `tools.rs` (`TOOLS` registry; capture stats 1225–1232) · `session.rs` (view sync
284–306, `sync_camera`) · `play_tools.rs` 14–70 · `bin/pav.rs` 145–148 (one-shot `ticks=` is used
twice) · `game_tools.rs` 171–186 (Shardfall autoplay pattern).

**Web:** `web/index.html` (WebGPU check; `#msg` removed when the canvas appears) ·
`scripts/pack_single_html.py` (loader; add `--icon`) · `scripts/build-web.sh`.

---

## Appendix C: shanty repertoire and fonts

**Traditional shanties (public domain melodies; write the ABC from the traditional tune, not
from any modern recording or arrangement):** *Drunken Sailor* (the obvious anthem: "What shall
we do with a sober Commodore?"), *Leave Her, Johnny*, *Spanish Ladies*, *Haul Away Joe*, *Blow
the Man Down*, *South Australia*, *Bully in the Alley*, *Randy Dandy-O*, *Whiskey Johnny*, *Roll
the Old Chariot*, *Sally Brown*, *Bound for the Rio Grande*, *Fire Down Below*, *John Kanaka*.
Ship at least six, plus an **original** title theme ("The Black Tap") and original state cues.
Lyrics, if shown, use traditional public-domain verses or original comic ones.

**Fonts (SIL Open Font License; from the official `google/fonts` repository; commit the font
files with their `OFL.txt` under `assets/fonts/<family>/`):** display — **Pirata One** or **IM
Fell English SC**; body/HUD — **Alegreya Sans** or **Fira Sans**. Check each licence file before
committing; fonts are the only binary assets the doctrine allows.

---

## Appendix D: preliminary engine analysis (planning view)

To be replaced by the executor's own `docs/ENGINE_ANALYSIS.md` after building; recorded here so
the hand-off carries the planning session's view.

- **Strongest parts:**
  - Everything runs, renders and checks headless. Tools are one registry exposed as CLI, REPL
    and MCP.
  - The deterministic simulation has snapshots, rewind and replays.
  - The hook-game pattern makes "a bot that wins" the first-class test.
  - The renderer's post stack (bloom, haze/shafts, outlines, per-part styles, painterly) is rich.
  - Puppets carry 437 clips; the prop format is readable.
  - The single-file web build worked first time here and booted in headless Chromium in under
    1 s.
- **Biggest gaps for this kind of game:**
  - A hook game can't present anything beyond HUD text lines. It has no camera control, no
    drawing from game state, no runtime atmosphere, no sound or music cues, no HUD widgets or
    menus, and the HUD is invisible to captures.
  - Beyond presentation: no title or attract flow, no ocean or sky, a single-floor nav grid, no
    character health, a guard that resets the player on sight, and no culling or LOD for large
    levels.
- **Planned uses:** sim/rewind/replay, the hook and `play` tool, the character controller
  (extended), puppets, moves and clips, rapier, room files (extended), the renderer and post
  stack (extended), particles, materials, props, `pav` tools, the web and Windows builds.
- **Planned non-uses:** Shardfall's ARPG systems, the pavilion's streaming wilderness, engine
  projectiles, engine guards, the ripple-water pools (except small pools), the drift car and
  helicopter, the asset/animation/creature studios (optional).
- **The one change (planning view):** give hook games a **presentation channel**: one hook that
  returns camera, UI, drawing, atmosphere, audio and effects as plain data in the frame, with the
  UI rendered identically in headless captures. Then a new game never has to edit `pav_app` or
  `pav_view` just to be seen and heard. This plan builds exactly that (§8.3).
