#!/usr/bin/env bash
# Concurrent conference join storm (signaling path; LiveKit optional).
#
# Usage:
#   export PALE_BASE_URL=http://127.0.0.1:8090
#   export PALE_ADMIN_TOKEN=...
#   ./scripts/load/meeting-join-storm.sh [joins] [concurrency]
set -euo pipefail

BASE_URL="${PALE_BASE_URL:?Set PALE_BASE_URL}"
BASE_URL="${BASE_URL%/}"
TOKEN="${PALE_ADMIN_TOKEN:?Set PALE_ADMIN_TOKEN}"
JOINS="${1:-30}"
CONCURRENCY="${2:-10}"
UA="Pale/meeting-load"

create_body=$(curl -sS -w "\n%{http_code}" -X POST \
  -H "User-Agent: $UA" \
  -H "X-Pale-Client: Pale/load" \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d "{\"name\":\"load-meet-$(date +%s)\",\"description\":\"join storm\"}" \
  "${BASE_URL}/v1/conferences")
code="${create_body##*$'\n'}"
body="${create_body%$'\n'*}"
conf_id=$(python3 -c 'import json,sys; print(json.loads(sys.argv[1]).get("id",""))' "$body" 2>/dev/null || true)

if [ -z "$conf_id" ]; then
  echo "meeting-join-storm: failed to create conference ($code) $body"
  exit 1
fi
echo "meeting-join-storm: conference=$conf_id joins=$JOINS concurrency=$CONCURRENCY"

ok=0
fail=0
run_join() {
  local i="$1"
  local uid
  uid=$(python3 -c 'import uuid; print(uuid.uuid4())')
  local code
  code=$(curl -sS -o /dev/null -w "%{http_code}" -X POST \
    -H "User-Agent: $UA" \
    -H "X-Pale-Client: Pale/load" \
    -H "Authorization: Bearer $TOKEN" \
    -H "Content-Type: application/json" \
    -d "{\"user_id\":\"$uid\",\"sip_uri\":\"sip:load${i}@load.local\"}" \
    "${BASE_URL}/v1/conferences/${conf_id}/participants" || echo "000")
  if [ "$code" = "200" ] || [ "$code" = "201" ]; then
    echo OK
  else
    echo FAIL:$code
  fi
}

export -f run_join
export BASE_URL TOKEN UA conf_id

# GNU parallel optional; fall back to xargs -P
if command -v parallel >/dev/null 2>&1; then
  results=$(seq 1 "$JOINS" | parallel -j "$CONCURRENCY" run_join {})
else
  results=$(seq 1 "$JOINS" | xargs -P "$CONCURRENCY" -I{} bash -c 'run_join "$@"' _ {})
fi

ok=$(printf '%s\n' "$results" | grep -c '^OK$' || true)
fail=$(printf '%s\n' "$results" | grep -c '^FAIL' || true)
echo "meeting-join-storm: ok=$ok fail=$fail"
if [ "$fail" -gt $((JOINS / 5)) ]; then
  echo "meeting-join-storm: FAILED"
  exit 1
fi
echo "meeting-join-storm: PASS"
