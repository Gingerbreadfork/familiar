#!/bin/sh
# Builds the .deb and .rpm packages: the daemon, the command line tool and the
# window, with the udev rule and the systemd units. Needs pnpm, a Rust
# toolchain and the WebKitGTK and udev development packages.
set -eu

root=$(cd "$(dirname "$0")/.." && pwd)

# The packages pick these two up from the workspace's release directory.
cargo build --release --manifest-path "$root/Cargo.toml" -p familiard -p familiar-cli
pnpm --dir "$root/gui" install --frozen-lockfile

# Installed copies only take an update that is signed with the release key.
if [ -n "${TAURI_SIGNING_PRIVATE_KEY:-}" ]; then
    pnpm --dir "$root/gui" tauri build --config '{"bundle":{"createUpdaterArtifacts":true}}'
else
    echo "TAURI_SIGNING_PRIVATE_KEY is not set: these packages cannot be offered as updates" >&2
    pnpm --dir "$root/gui" tauri build
fi

echo
echo "Packages:"
ls -1 "$root"/gui/src-tauri/target/release/bundle/deb/*.deb* "$root"/gui/src-tauri/target/release/bundle/rpm/*.rpm*
