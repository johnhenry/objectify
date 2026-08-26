#!/usr/bin/env bash
# Rewind restores old state as a NEW version — history is never destroyed.
#
# `rewind N` reads the state at version N and appends it as the next version.
# Nothing is deleted: the "bad" versions stay in the log, and the rewind itself
# shows up as one more auditable event.
set -euo pipefail
source "$(cd "$(dirname "$0")" && pwd)/_lib.sh"

STORE="$(mktemp -d)"
trap 'rm -rf "$STORE"' EXIT
cd "$STORE"

obj init >/dev/null
ID="$(create_id "deploy settings")"
obj use "$ID" set '{"replicas": 3, "region": "us-east"}' >/dev/null  # v2: known-good
obj use "$ID" set '{"replicas": 300, "region": "us-east"}' >/dev/null # v3: oops

echo "== Current (bad) state =="
obj use "$ID" get

echo
echo "== Rewind to the known-good version 2 =="
obj rewind "$ID" 2

echo
echo "== State is restored... =="
obj use "$ID" get
obj use "$ID" get | grep -q '"replicas": 3$'

echo
echo "== ...and the log kept everything, including the mistake and the rewind =="
obj log "$ID"
VERSIONS="$(obj log "$ID" | grep -c '"version"')"
test "$VERSIONS" -eq 4   # create, good set, bad set, rewind

echo
echo "OK: rewound to v2 as a new v4; all 4 versions still in history"
