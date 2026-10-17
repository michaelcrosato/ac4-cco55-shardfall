#!/usr/bin/env bash
# Everything Assassin's Keg needs beyond the engine's own setup (logged in docs/BUILD_LOG.md):
# the wasm32 target, wasm-bindgen-cli at Cargo.lock's version (browser build) and Playwright
# for scripts/browser_check.mjs (uses the container's Chromium at /opt/pw-browsers when present).
set -euo pipefail
cd "$(dirname "$0")/.."
rustup target add wasm32-unknown-unknown >/dev/null 2>&1 || true
want=$(awk '/^name = "wasm-bindgen"$/{getline; gsub(/version = |"/,""); print; exit}' Cargo.lock)
have=$(wasm-bindgen --version 2>/dev/null | awk '{print $2}' || true)
if [ "$have" != "$want" ]; then
  cargo install --locked -q wasm-bindgen-cli --version "$want"
fi
if ! node -e "require.resolve('playwright')" >/dev/null 2>&1 && ! npm ls -g playwright >/dev/null 2>&1; then
  npm install -g --silent playwright@1.56.1 || true
fi
