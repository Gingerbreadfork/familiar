#!/usr/bin/env bash
# Attaches files to the release for the pushed tag, creating the release if
# there is none yet. The checksum file goes up last, so a release that has it
# is complete and is left as published. Without it, whatever is there came from
# a run that stopped part way and is replaced: signatures and checksums only
# match the packages built in the same run.
# Usage: attach-to-release.sh SUMS_FILE FILE...
set -euo pipefail

tag=${GITHUB_REF_NAME:?}
repo=${GITHUB_REPOSITORY:?}
sums=$1
shift

if ! gh release view "$tag" --repo "$repo" >/dev/null 2>&1; then
  gh release create "$tag" --repo "$repo" --title "Familiar ${tag#v}" --generate-notes
fi

existing=$(gh release view "$tag" --repo "$repo" --json assets --jq '.assets[].name')
if grep -qxF -- "$(basename "$sums")" <<<"$existing"; then
  echo "The release already has its files, keeping the published copies."
  echo "To publish this build instead, delete the files on the release and run again."
  exit 0
fi

# One at a time, so the feed is never up before the packages it points to.
for f in "$@" "$sums"; do
  gh release upload "$tag" --repo "$repo" --clobber "$f"
done
