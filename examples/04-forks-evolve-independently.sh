#!/usr/bin/env bash
# A fork copies an object's state into a new object; the two then diverge freely.
#
# `fork` snapshots the source (optionally at a past version with --at) into a
# fresh object with its own ID and its own history. Mutating one never touches
# the other — the same model as forking a repo.
set -euo pipefail
source "$(cd "$(dirname "$0")" && pwd)/_lib.sh"

STORE="$(mktemp -d)"
trap 'rm -rf "$STORE"' EXIT
cd "$STORE"

obj init >/dev/null
ORIG="$(create_id "prod config")"
obj use "$ORIG" set '{"env": "prod", "debug": false}' >/dev/null

echo "== Fork prod config into a staging copy =="
FORK="$(obj fork "$ORIG" | tr -d '"')"
echo "original: $ORIG   fork: $FORK"

echo
echo "== Mutate only the fork =="
obj use "$FORK" set '{"env": "staging", "debug": true}' >/dev/null

echo "original is untouched:"
obj use "$ORIG" get
obj use "$ORIG" get | grep -q '"env": "prod"'

echo "fork went its own way:"
obj use "$FORK" get
obj use "$FORK" get | grep -q '"env": "staging"'

echo
echo "== Each has an independent history =="
echo "-- original --"; obj log "$ORIG"
echo "-- fork --";     obj log "$FORK"

echo
echo "OK: fork diverged without affecting the original"
