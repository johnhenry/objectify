#!/usr/bin/env bash
# Expiry is soft: expired objects keep answering until `gc` actually reaps them.
#
# `create --expire=1s` marks an object with a TTL. After it expires it still
# responds (with a warning on stderr) and disappears from the default `list`.
# Only `objectify gc` permanently deletes expired objects — and it leaves
# everything else alone.
set -euo pipefail
source "$(cd "$(dirname "$0")" && pwd)/_lib.sh"

STORE="$(mktemp -d)"
trap 'rm -rf "$STORE"' EXIT
cd "$STORE"

obj init >/dev/null
TEMP="$(create_id "scratch notes" --expire=1s)"
KEEP="$(create_id "important notes")"
obj use "$TEMP" set '{"note": "throwaway"}' >/dev/null
obj use "$KEEP" set '{"note": "keep me"}'   >/dev/null

echo "waiting for the 1s expiry to pass..."
sleep 2

echo
echo "== Expired object still answers (soft expiry) =="
obj use "$TEMP" get 2>/dev/null
obj use "$TEMP" get 2>/dev/null | grep -q '"note": "throwaway"'

echo
echo "== gc reaps exactly the expired one =="
GC="$(obj gc)"
echo "$GC"
echo "$GC" | grep -q '"deleted": 1'

echo
echo "== It is gone now; the unexpired object survives =="
if obj use "$TEMP" get >/dev/null 2>&1; then
  echo "ERROR: expired object should have been deleted" >&2
  exit 1
fi
obj use "$KEEP" get
obj use "$KEEP" get | grep -q '"note": "keep me"'

echo
echo "OK: gc deleted only the expired object"
