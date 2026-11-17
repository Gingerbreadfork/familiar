#!/usr/bin/env bash
# Fails unless the workspace, the app crate, tauri.conf.json and package.json
# carry the same version. Pass a version, or a tag like v0.2.0, to require
# that one.
set -euo pipefail
cd "$(dirname "$0")/.."

workspace=$(grep -m1 '^version' Cargo.toml | cut -d'"' -f2)
app=$(grep -m1 '^version' gui/src-tauri/Cargo.toml | cut -d'"' -f2)
conf=$(node -p "require('./gui/src-tauri/tauri.conf.json').version")
page=$(node -p "require('./gui/package.json').version")
want="${1:-$workspace}"
want="${want#v}"

status=0
for entry in "Cargo.toml:$workspace" "gui/src-tauri/Cargo.toml:$app" "gui/src-tauri/tauri.conf.json:$conf" "gui/package.json:$page"; do
  if [ "${entry#*:}" != "$want" ]; then
    echo "${entry%%:*} is at ${entry#*:}, expected $want" >&2
    status=1
  fi
done
[ "$status" = 0 ] && echo "All four files are at $want"
exit "$status"
