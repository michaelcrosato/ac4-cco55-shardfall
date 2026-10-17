#!/usr/bin/env bash
# Builds Assassin's Keg: Black Tap as ONE self-contained HTML file in dist/, named per the
# ai-hub convention: <Source>-<UniqueID>-<build date>-<model>.html (PLAN.md §2).
#   scripts/build-game.sh [MODEL_LABEL]     (default label: Claude-Opus-5.5)
set -euo pipefail
cd "$(dirname "$0")/.."
model=${1:-Claude-Opus-5.5}
date=$(date -u +%Y-%m-%d)
name="Assassins-Creed-IV-Black-Flag-AssassinsKegBlackTap-W9-$date-$model.html"
mkdir -p dist
# One dist build at a time (disk): drop older ones.
find dist -maxdepth 1 -name 'Assassins-Creed-IV-Black-Flag-AssassinsKegBlackTap-W9-*.html' ! -name "$name" -delete
scripts/build-web.sh --single-file --scene blacktap \
	--title "Assassin's Keg — Black Tap" \
	--description "Assassin's Keg: Black Tap — sail the brig Black Tap, trade broadsides, swim ashore, climb a chapel tower and take a fort from its commodore. A pirate parody played straight, all procedural." \
	--theme-color "#0f3b4c" \
	--icon games/blacktap/web/icon.svg \
	--out "dist/$name"
ls -la "dist/$name"
