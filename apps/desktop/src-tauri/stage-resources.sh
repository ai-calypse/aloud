#!/bin/sh
# Stages what the bundle needs into `resources/`, which tauri.conf.json points
# at. Run before `cargo tauri build`.
#
# Two things do not survive bundling on their own:
#
#   - The browser engine and its axd helper (built here).
#   - The Now Playing helper (`crates/media/mediaremote/build.sh`).
#   - `.env`. A `.app` launched from Finder has a working directory of `/`, so
#     the walk that finds it in dev finds nothing and every API key is empty.
#
# ponytail: a shell script rather than a build-time hook. It runs once per
# release build and the alternative is teaching build.rs about bundle layout.
set -eu

here=$(cd "$(dirname "$0")" && pwd)
repo=$(cd "$here/../../.." && pwd)
out="$here/resources"

mkdir -p "$out"

# The task engine (browser tasks for the agent): its TypeScript, production dependencies
# only (dev tools and the 240 MB Claude SDK stay out), and the axd accessibility helper.
rm -rf "$out/engine"
mkdir -p "$out/engine"
rsync -a --exclude node_modules --exclude data --exclude axd --exclude package-lock.json "$repo/engine/" "$out/engine/"
(cd "$out/engine" && npm install --omit=dev --no-audit --no-fund --silent)
swiftc -O "$repo/engine/axd/main.swift" -o "$out/axd"
echo "staged engine ($(du -sh "$out/engine" | cut -f1)) and axd"

sh "$repo/crates/media/mediaremote/build.sh" "$out/mediaremote"

if [ -f "$repo/.env" ]; then
    cp "$repo/.env" "$out/.env"
    echo "staged .env (keys ship inside the app — do not distribute this build)"
else
    echo "warning: no .env at repo root; the bundled app will have no API keys" >&2
fi
