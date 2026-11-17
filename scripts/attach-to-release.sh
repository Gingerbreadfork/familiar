#!/usr/bin/env bash
# Attaches files to the release for the pushed tag, creating the release if
# there is none yet. Files already on the release are kept. The checksum file
# goes up only when this run added something.
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
uploaded=0
for f in "$@"; do
  name=$(basename "$f")
  if grep -qxF -- "$name" <<<"$existing"; then
    echo "$name is already on the release, keeping the published copy"
  else
    gh release upload "$tag" --repo "$repo" "$f"
    uploaded=1
  fi
done

if [ "$uploaded" = 1 ]; then
  gh release upload "$tag" --repo "$repo" "$sums"
fi
