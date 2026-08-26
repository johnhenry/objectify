#!/usr/bin/env bash
# Every write appends a version; any past version stays readable.
#
# `set` never overwrites in place — it appends an event. `log` lists the whole
# history, `get --at=N` reads any historical version, and `diff` emits an
# RFC 6902 JSON Patch describing exactly what changed between two versions.
set -euo pipefail
source "$(cd "$(dirname "$0")" && pwd)/_lib.sh"

STORE="$(mktemp -d)"
trap 'rm -rf "$STORE"' EXIT
cd "$STORE"

obj init >/dev/null
ID="$(create_id "feature flags")"                       # version 1: create (null)
obj use "$ID" set '{"darkMode": false}'      >/dev/null # version 2
obj use "$ID" set '{"darkMode": true}'       >/dev/null # version 3
obj use "$ID" set '{"darkMode": true, "beta": true}' >/dev/null # version 4

echo "== Full history (one row per write) =="
obj log "$ID"

echo
echo "== Time travel: state as of version 2 =="
obj use "$ID" get --at=2
obj use "$ID" get --at=2 | grep -q '"darkMode": false'

echo
echo "== What changed between v2 and v4 (JSON Patch) =="
DIFF="$(obj diff "$ID" 2 4)"
echo "$DIFF"
echo "$DIFF" | grep -q '"op": "replace"'   # darkMode flipped
echo "$DIFF" | grep -q '"op": "add"'       # beta appeared

echo
echo "OK: 4 versions recorded, old state readable, diff explains the change"
