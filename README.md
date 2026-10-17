# Assassin's Keg: Black Tap

**`[AssassinsKegBlackTap-W9]`** · *Nothing is watered down; everything is on tap.*

An original spiritual successor to *Assassin's Creed IV: Black Flag*: one polished level, a
sea with three islands, where you sail the brig *Black Tap*, trade broadsides, board a
schooner, swim ashore, climb a chapel tower, slip through a colonial port and take a fort from
its commodore. It is built on the Pavilion engine from
[shardfall](https://github.com/michaelcrosato/shardfall), extended here into the foundation for
the full game.

**Status:** in development (Phase 0 done: the engine is vendored and builds here). The build
plan is [`PLAN.md`](PLAN.md); progress is logged in [`docs/BUILD_LOG.md`](docs/BUILD_LOG.md).

## Source Information

| | |
|---|---|
| **Initial Date** | 2026-10-17 |
| **SOURCE** | *Assassin's Creed IV: Black Flag* (Ubisoft Montreal, 2013; PS3, PS4, Xbox 360, Xbox One, Wii U, PC) |
| **GAME ENGINE** | Pavilion (shardfall), https://github.com/michaelcrosato/shardfall at commit `0098196`, workspace 0.1.0: Rust 1.98.1, wgpu 30 (WebGPU), rapier3d 0.36, egui 0.36. Vendored and extended in this repository. |
| **Environment** | Claude Code 2.1.302 in an Anthropic-managed cloud container (claude.ai/code remote session launched from the Claude desktop app); Ubuntu 24.04.5 LTS, Linux 6.18 x86_64, 4 vCPUs, 15 GiB RAM, no GPU (Mesa lavapipe for headless rendering); headless Chromium 1194 via Playwright 1.56.1 for browser checks. Targets: the browser (Chrome/Edge with WebGPU, one self-contained HTML file), native Linux and Windows. |
| **Model and Effort Level** | Planning: Anthropic Claude Opus 5.5 (`claude-opus-5-5`), effort max (confirmed from session metadata). Implementation: Anthropic **Claude Opus 5.5** (`claude-opus-5-5`), effort **xhigh** — confirmed with `get_session` (`session_context.model` = `claude-opus-5-5`, `external_metadata.last_served_model` = `claude-opus-5-5`, `effort_level` = `xhigh`). |

The engine evaluation is kept separate from this block: see [`PLAN.md`, Appendix D](PLAN.md#appendix-d-preliminary-engine-analysis-planning-view)
(planning view) and, once the game is built, `docs/ENGINE_ANALYSIS.md`.

## Naming

Fifty playful titles were generated from the source's name and one was drawn at random with a
cryptographic RNG, plus a random 2-character ID: *Assassin's Keg: Black Tap* →
`AssassinsKegBlackTap-W9`. The full list is in [`PLAN.md`, Appendix A](PLAN.md#appendix-a-the-50-titles-and-the-pick).
