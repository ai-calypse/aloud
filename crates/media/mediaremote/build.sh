#!/bin/sh
# Builds the now-playing helper into `apps/desktop/src-tauri/resources/mediaremote/`
# (or $1), which tauri.conf.json bundles and `aloud_media::mediaremote` finds.
#
# What it is: ungive/mediaremote-adapter (BSD-3, see LICENSE), vendored at
# commit 2971825 minus its test client. From macOS 15.4 the private MediaRemote
# framework only answers Apple-signed processes, so the adapter is a small
# dylib that `/usr/bin/perl` (Apple-signed) loads; the perl script streams
# now-playing JSON and takes control commands.
#
# Reproducible: clang from the Command Line Tools and nothing else (no cmake),
# universal arm64 + x86_64, ad-hoc signed like the other bundled helpers.
#
# ponytail: a flat "framework" (just the dylib). The adapter script only needs
# `X.framework/X`, and a Versions/ symlink farm does not survive Tauri's
# resource copy. Run again after bumping the vendored sources.
set -eu

here=$(cd "$(dirname "$0")" && pwd)
out=${1:-"$here/../../../apps/desktop/src-tauri/resources/mediaremote"}
fw="$out/MediaRemoteAdapter.framework"

rm -rf "$out"
mkdir -p "$fw"

# shellcheck disable=SC2046
clang -dynamiclib -fobjc-arc -fvisibility=default -O2 -w \
    -arch arm64 -arch x86_64 -mmacosx-version-min=13.0 \
    -I "$here/include" -I "$here/src" \
    -framework Foundation -framework AppKit -framework UniformTypeIdentifiers \
    -install_name @rpath/MediaRemoteAdapter \
    $(find "$here/src" -name '*.m' | sort) \
    -o "$fw/MediaRemoteAdapter"
codesign --force --sign - "$fw/MediaRemoteAdapter"

cp "$here/bin/mediaremote-adapter.pl" "$out/"
cp "$here/LICENSE" "$out/LICENSE"
echo "built mediaremote helper in $out"
