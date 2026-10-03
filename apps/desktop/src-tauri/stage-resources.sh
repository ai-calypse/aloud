#!/bin/sh
# Stages what the bundle needs into `resources/`, which tauri.conf.json points
# at. Run before `cargo tauri build`.
#
# Two things do not survive bundling on their own:
#
#   - The MacosUseSDK helpers. `build.rs` compiles them into an OUT_DIR whose
#     name carries a build hash, so there is no stable path for the bundler to
#     reference. Copied here under fixed names; `helper::locate` already looks
#     in `../Resources`.
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

# Newest first: several OUT_DIRs accumulate across debug/release builds, and a
# stale helper would ship silently.
for tool in TraversalTool InputControllerTool; do
    found=$(find "$repo/target" -name "$tool" -type f -perm -111 \
        -not -path '*dSYM*' -not -path '*.build*' 2>/dev/null \
        | xargs -I{} stat -f '%m %N' {} 2>/dev/null | sort -rn | head -1 | cut -d' ' -f2-)
    if [ -z "$found" ]; then
        echo "error: $tool not built — run 'cargo build --release -p aloud-computer' first" >&2
        exit 1
    fi
    cp "$found" "$out/$tool"
    echo "staged $tool from $found"
done

# The task engine (browser + Mac-app tasks for the agent): its TypeScript, production dependencies
# only (dev tools and the 240 MB Claude SDK stay out), and the axd accessibility helper.
rm -rf "$out/engine"
mkdir -p "$out/engine"
rsync -a --exclude node_modules --exclude data --exclude axd --exclude package-lock.json "$repo/engine/" "$out/engine/"
(cd "$out/engine" && npm install --omit=dev --no-audit --no-fund --silent)
swiftc -O "$repo/engine/axd/main.swift" -o "$out/axd"
echo "staged engine ($(du -sh "$out/engine" | cut -f1)) and axd"

if [ -f "$repo/.env" ]; then
    cp "$repo/.env" "$out/.env"
    echo "staged .env (keys ship inside the app — do not distribute this build)"
else
    echo "warning: no .env at repo root; the bundled app will have no API keys" >&2
fi
