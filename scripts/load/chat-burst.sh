#!/usr/bin/env bash
# Lightweight chat burst against Pale Server (not a full load suite).
#
# Usage:
#   export PALE_BASE_URL=http://127.0.0.1:8090
#   export PALE_ADMIN_TOKEN=...
#   export PALE_ROOM_ID=...
#   ./scripts/load/chat-burst.sh [count]
set -euo pipefail

BASE_URL="${PALE_BASE_URL:?Set PALE_BASE_URL}"
BASE_URL="${BASE_URL%/}"
TOKEN="${PALE_ADMIN_TOKEN:?Set PALE_ADMIN_TOKEN}"
ROOM_ID="${PALE_ROOM_ID:?Set PALE_ROOM_ID}"
COUNT="${1:-50}"
UA="Pale/load-test"

ok=0
fail=0
start=$(date +%s)

for i in $(seq 1 "$COUNT"); do
  code=$(curl -sS -o /dev/null -w "%{http_code}" -X POST \
    -H "User-Agent: $UA" \
    -H "X-Pale-Client: Pale/load" \
    -H "Authorization: Bearer $TOKEN" \
    -H "Content-Type: application/json" \
    -d "{\"body\":\"burst message $i at $(date -u +%Y-%m-%dT%H:%M:%SZ)\"}" \
    "${BASE_URL}/v1/rooms/${ROOM_ID}/messages" || echo "000")
  if [ "$code" = "200" ] || [ "$code" = "201" ]; then
    ok=$((ok + 1))
  else
    fail=$((fail + 1))
  fi
done

elapsed=$(( $(date +%s) - start ))
echo "chat-burst: sent=$COUNT ok=$ok fail=$fail elapsed_sec=$elapsed"
[ "$fail" -eq 0 ]
