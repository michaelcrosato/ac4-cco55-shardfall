# Build log — Assassin's Keg: Black Tap

Newest first. Each entry: what was built, how to verify it (exact commands), decisions and why,
open problems. This file and the ticks in `PLAN.md` §14 are the memory across context
compactions: after any restart read §14, then the top of this file, then `git log -20`.

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
