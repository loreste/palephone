#!/usr/bin/env bash
# SSE reconnect/fanout smoke-load against Pale Server.
#
# Spawns N concurrent EventSource-style clients that hold /v1/events open,
# then posts a chat message (or pings) to generate events.
#
# Usage:
#   export PALE_BASE_URL=http://127.0.0.1:8090
#   export PALE_ADMIN_TOKEN=...
#   ./scripts/load/sse-fanout.sh [clients] [hold_seconds]
set -euo pipefail

BASE_URL="${PALE_BASE_URL:?Set PALE_BASE_URL}"
BASE_URL="${BASE_URL%/}"
TOKEN="${PALE_ADMIN_TOKEN:?Set PALE_ADMIN_TOKEN}"
CLIENTS="${1:-25}"
HOLD_SECS="${2:-15}"
UA="Pale/sse-load"
TMPDIR="${TMPDIR:-/tmp}/pale-sse-$$"
mkdir -p "$TMPDIR"
trap 'rm -rf "$TMPDIR"; kill $(jobs -p) 2>/dev/null || true' EXIT

echo "sse-fanout: clients=$CLIENTS hold=${HOLD_SECS}s url=$BASE_URL"

for i in $(seq 1 "$CLIENTS"); do
  (
    # Browser EventSource cannot set Authorization; Pale also accepts token query.
    # Prefer header for native Pale clients.
    curl -sS -N \
      -H "User-Agent: $UA" \
      -H "X-Pale-Client: Pale/load" \
      -H "Authorization: Bearer $TOKEN" \
      -H "Accept: text/event-stream" \
      --max-time "$HOLD_SECS" \
      "${BASE_URL}/v1/events" \
      >"$TMPDIR/client-$i.log" 2>"$TMPDIR/client-$i.err" || true
  ) &
done

# Give connections a moment to establish
sleep 2

# Generate activity if a room is provided
if [ -n "${PALE_ROOM_ID:-}" ]; then
  curl -sS -o /dev/null -w "activity_post:%{http_code}\n" -X POST \
    -H "User-Agent: $UA" \
    -H "X-Pale-Client: Pale/load" \
    -H "Authorization: Bearer $TOKEN" \
    -H "Content-Type: application/json" \
    -d "{\"body\":\"sse fanout pulse $(date -u +%Y-%m-%dT%H:%M:%SZ)\"}" \
    "${BASE_URL}/v1/rooms/${PALE_ROOM_ID}/messages" || true
fi

wait || true

ok=0
fail=0
bytes=0
for f in "$TMPDIR"/client-*.log; do
  [ -f "$f" ] || continue
  sz=$(wc -c <"$f" | tr -d ' ')
  bytes=$((bytes + sz))
  # Any HTTP error lands in .err; empty body is ok for quiet periods
  base=$(basename "$f" .log)
  if [ -s "$TMPDIR/${base}.err" ] && grep -qiE 'error|refused|401|403|500' "$TMPDIR/${base}.err" 2>/dev/null; then
    fail=$((fail + 1))
  else
    ok=$((ok + 1))
  fi
done

echo "sse-fanout: ok_clients=$ok fail_clients=$fail total_bytes=$bytes"
# Soft pass: majority of clients must not hard-fail
if [ "$fail" -gt $((CLIENTS / 2)) ]; then
  echo "sse-fanout: FAILED (too many client errors)"
  exit 1
fi
echo "sse-fanout: PASS"
