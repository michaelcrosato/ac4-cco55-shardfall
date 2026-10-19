# Progress

The current state of this repository, kept to one page because every session reads it (Claude
Code loads it through `CLAUDE.md`). This repository is the game **Assassin's Keg: Black Tap**
(`README.md`) on the Pavilion engine, vendored from shardfall @ `0098196` and extended. The
game's journal is `docs/BUILD_LOG.md`, newest first; `docs/HISTORY.md` is shardfall's journal up
to the vendoring (search it for engine background; don't add to it).

## Where things stand (2026-10-18)
- **Shipped** (PR #1, merged 2026-10-17): one polished level, Tab I (a 1.2 km sea, three islands,
  six Rounds, free roam, 26 collectibles, three difficulties, the Drydock), a 10-foot front end
  with attract mode, phones attract-only, a kiosk flag. Every `PLAN.md` §14 box is ticked; the
  evidence, box by box, is BUILD_LOG's "§14 evidence map".
- **Verified on `a0c43f3`**: `scripts/check.sh --full` all green (612 tests; each Round x 5
  seeds, the whole Tab x 3, 30 min of attract); the single-file HTML in `dist/` was built from
  that commit (only docs and `scripts/browser_check.mjs` changed after it); browser check 10/10.
- **Re-checked 2026-10-18** in a fresh container: the toolchain and `scripts/check.sh` (BUILD_LOG).
- **Windows `.exe`** (2026-10-19, from `f71af5e`): `scripts/build-windows.sh`, then
  `scripts/smoke-windows.sh 40` under Wine + lavapipe: boots to the title, idles into attract,
  saves next to the `.exe`, 0 errors. Not committed; the build script remakes it.
- Code: the game is `crates/blacktap` with its data in `games/blacktap/`; the reusable kit is
  `crates/pav_kit` and `crates/pav_ui`; every engine change is listed in `ENGINE_UPSTREAM.md`.
  Adding content: `docs/CONTENT_GUIDE.md`. Engine evaluation: `docs/ENGINE_ANALYSIS.md`.

## Next
- **The plan is done**, Phase 10's optional Windows `.exe` included. Next for it: the user plays
  it on real Windows (frame rate, DX12, gamepads, audio) and reports what they see.
- **The user's call:** sending the engine changes upstream to shardfall (`ENGINE_UPSTREAM.md`,
  the `docs/ENGINE_ANALYSIS.md` row for shardfall's `docs/FEEDBACK.md`), and anything past the
  scope guards (`PLAN.md` §17.2: more Tabs, co-op, imported assets, touch controls).
- More content inside Tab I goes through `docs/CONTENT_GUIDE.md`; run `scripts/check.sh` (and
  `--full` after Round or level changes), then `scripts/build-game.sh` to refresh `dist/`.

## Known limits
- No GPU here: 60 fps on the user's cards is unmeasured; instance, triangle and particle counts
  and the quality presets are the proxy (BUILD_LOG "Performance budgets"). The slowest scene,
  `blacktap/round/6`, simulates at 389 ticks/s in the dev build. The `.exe` is smoked only under
  Wine, and keeps the engine's file names (`shardfall.exe`, `.log`, `.toml`).
- The browser check proves boot, attract, exits and the phone menu only: SwiftShader draws
  under 0.1 fps. Browser audio starts after the first input (the title says so).
- Some data files ignore unknown fields and some references aren't checked: CONTENT_GUIDE,
  "What no check catches". The art checklist's one note: the barracks' back wall is plain.
- PR #1 had no code review (Codex hit its usage limit).
- Engine (shardfall's open issues at the vendoring): its egui HUD and menus are invisible to the
  tools (the game's own UI isn't: `pav ui`); the `campaign` bot stalls on Shardfall's level 4;
  copies of one asset patched to different materials under one key draw one material.

## Keeping this page current
When you finish something, add a dated entry at the top of `docs/BUILD_LOG.md` (what changed,
how to verify it, the decisions), list any engine change in `ENGINE_UPSTREAM.md`, then update
the sections above. Keep this page under 60 lines: `pav_tools/tests/docs_limits.rs` fails past
that.
