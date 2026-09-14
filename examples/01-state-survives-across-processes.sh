#!/usr/bin/env bash
# State written by one CLI invocation is still there for the next one.
#
# Each `objectify` call is a separate short-lived process. Because state lives
# in a SQLite database inside .objectify/, an object written by one process is
# fully readable by a later, unrelated process — no server, no daemon.
set -euo pipefail
source "$(cd "$(dirname "$0")" && pwd)/_lib.sh"

STORE="$(mktemp -d)"
trap 'rm -rf "$STORE"' EXIT
cd "$STORE"

echo "== Process 1: init the store and write some state =="
obj init
ID="$(create_id "app config")"
echo "created object: $ID"
obj use "$ID" set '{"theme": "dark", "fontSize": 14}'

echo
echo "== Process 2 (a completely separate invocation): read it back =="
STATE="$(obj use "$ID" get)"
echo "$STATE"

# Prove the round-trip actually happened.
echo "$STATE" | grep -q '"theme": "dark"'
echo
echo "OK: state persisted across independent processes"
