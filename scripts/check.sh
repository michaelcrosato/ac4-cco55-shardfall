#!/usr/bin/env bash
# The quality gate for Assassin's Keg: Black Tap (PLAN.md §15.1). Run from anywhere.
#   scripts/check.sh          fmt, clippy -D warnings (native + wasm32), tests of the game's crates
#   scripts/check.sh --full   also the long runs: each Round x 5 seeds, the full Tab x 3 seeds,
#                             attract mode (30 min of demo play, headless), determinism suites
set -euo pipefail
cd "$(dirname "$0")/.."
export CARGO_BUILD_JOBS=${CARGO_BUILD_JOBS:-4}
full=false
[[ ${1:-} == --full ]] && full=true

step() { printf '\n== %s\n' "$*"; }

# The crates this repository changed or added (the engine's other crates are untouched).
crates=(pav_core pav_kit pav_ui blacktap pav_view pav_render pav_audio pav_tools pav_app)
present=()
for c in "${crates[@]}"; do [[ -d crates/$c ]] && present+=(-p "$c"); done
tested=()
for c in pav_core pav_kit pav_ui blacktap pav_audio pav_tools pav_view; do [[ -d crates/$c ]] && tested+=(-p "$c"); done

step "cargo fmt --check"
cargo fmt --all --check

step "clippy (native, -D warnings)"
cargo clippy --locked -q "${present[@]}" --all-targets -- -D warnings

step "clippy (wasm32, -D warnings)"
cargo clippy --locked -q -p pav_app --target wasm32-unknown-unknown -- -D warnings

# WGSL: whenever the shaders differ from the vendored engine (commit 79061e4).
if ! git diff --quiet 79061e4 -- crates/pav_render/src/shaders 2>/dev/null || [[ ${CHECK_WGSL:-0} == 1 ]]; then
	step "WGSL (Tint)"
	scripts/check-wgsl.sh
fi

step "tests"
cargo test --locked -q "${tested[@]}"

if $full; then
	step "long runs (ignored tests: Rounds x5, full Tab x3, attract, determinism)"
	cargo test --locked -q -p blacktap -- --ignored --test-threads 2
fi
printf '\ncheck.sh: all green%s\n' "$($full && echo ' (full)')"
