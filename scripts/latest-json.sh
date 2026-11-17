#!/usr/bin/env bash
# Writes latest.json, the feed installed copies read to find a newer release,
# from the signature files of the packages.
# Usage: latest-json.sh TAG DIR...   (the directories holding the *.sig files)
set -euo pipefail

tag=$1
shift
repo=${GITHUB_REPOSITORY:-Gingerbreadfork/familiar}
base="https://github.com/$repo/releases/download/$tag"

# One entry, from the signature of the first package matching the pattern.
entry() {
  local key=$1 pattern=$2 sig
  sig=$(find "${dirs[@]}" -maxdepth 1 -name "$pattern.sig" | sort | head -1)
  [ -n "$sig" ] || return 0
  jq -n --arg key "$key" --arg url "$base/$(basename "${sig%.sig}")" --rawfile sig "$sig" \
    '{($key): {url: $url, signature: ($sig | rtrimstr("\n"))}}'
}

dirs=("$@")
{
  entry linux-x86_64-deb '*.deb'
  entry linux-x86_64-rpm '*.rpm'
} | jq -s --arg version "${tag#v}" \
  --arg notes "https://github.com/$repo/releases/tag/$tag" \
  --arg date "$(date -u +%Y-%m-%dT%H:%M:%SZ)" \
  '{version: $version, notes: $notes, pub_date: $date, platforms: add}'
