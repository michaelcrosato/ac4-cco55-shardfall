# CLAUDE.md

<!-- Claude Code expands these imports at launch, so every session (and every subagent that
doesn't set omitClaudeMd) starts with them. Other agents read AGENTS.md, which names the other
two files. -->
@docs/DOCTRINE.md
@AGENTS.md
@docs/PROGRESS.md

## Claude Code
- This repository is the game Assassin's Keg: Black Tap on the vendored engine: `AGENTS.md`'s
  first section says how the engine's rules apply here; `docs/PROGRESS.md` is the game's state.
- Subagents load this file and its imports. A subagent that must not see the repository's docs
  (a benchmark author or judge, a blind brief writer) sets `omitClaudeMd: true` in its
  `.claude/agents/` definition, as the two asset-bench agents do.

## Distribution: the self-contained HTML (no Vercel)
We no longer deploy to Vercel (2026-11-02). Distribution and testing use one self-contained
HTML file, committed in `dist/` (`README.md` links it), that runs from `file://` in Chrome/Edge.

Rules:
- Never deploy to Vercel: no connector deployments, deploy hooks or Vercel CLI.
- Keep `dist/` current: a PR that changes what the browser build runs (code, game data, `web/`,
  the pack script) rebuilds it with `scripts/build-game.sh`, checks it with
  `node scripts/browser_check.mjs dist/<file>.html out/browser`, and commits it in the same PR.
  Docs-only and test-only PRs leave it alone. The script keeps one file and drops older ones;
  update `README.md`'s link when the dated name changes.
- `vercel.json` stays only as a guard (`"git": { "deploymentEnabled": false }`), so a push never
  deploys while the Vercel project is still linked; don't change it.
