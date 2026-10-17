# Engine upstream

This repository vendors the **Pavilion** engine from **shardfall**:

- Source: https://github.com/michaelcrosato/shardfall
- Commit: `0098196` ("Co-op plan on hold (#74)"), workspace version 0.1.0
- What was copied: `git ls-files -- . ':!:pavilion-lite'` (the file set of
  `scripts/package-agent.sh`), imported unchanged as commit `79061e4` ("Vendor shardfall engine @
  0098196"). The engine README moved to `docs/ENGINE_README.md`.

Every local change to the engine is listed below, newest first, so it can be sent upstream or
re-applied on a newer engine. "Engine" means every crate except `crates/blacktap` (the game) and
everything outside `games/blacktap/`. New engine crates (`pav_ui`, `pav_kit`) are game-agnostic.

## Changes

| Date | Area | Change | Files |
|---|---|---|---|
| 2026-10-17 | render, view, app, ui, tools | Performance and quality (PLAN §9.18): hook-game quality presets Low/Medium/High (`view.quality`; shadow map size `Sun::shadow_map`, shadow reach, ocean grid, plant share, terrain LOD, effects), half-resolution terrain LOD with skirts, camera-fitted sun shadows, distance classes on statics and puppet details, plants thinned with distance plus a sparse far forest, screen-size culling (`Scene::cull_pixels`), static batching of plain pieces per region, per-class render stats; looks Painted Seas / Memory Mist / Storm and the Settings "Visual Style" row; `tour stats=true`. | `pav_render/src/{renderer,scene,cull}.rs`, `pav_view/src/{quality,batch,build,atmos,looks.toml}`, `pav_view/tests/quality.rs`, `pav_app/src/app/play.rs`, `pav_ui/src/settings.rs`, `pav_tools/src/{tools,kit_tools}.rs` |
| 2026-10-17 | setup | Session hook also runs `scripts/setup-game.sh` (wasm32 target, wasm-bindgen-cli, Playwright). | `.claude/hooks/session-start.sh`, `scripts/setup-game.sh` |
| 2026-10-17 | scripts | `scripts/check.sh`: the repository's quality gate. | `scripts/check.sh` |
