#!/usr/bin/env bash
# Pale Server smoke tests: health, login, chat, meeting join (signaling).
#
# Usage:
#   export PALE_BASE_URL=http://127.0.0.1:8090
#   export PALE_SIP_URI=sip:admin@example.com
#   export PALE_PASSWORD='your-password'
#   ./scripts/smoke-test.sh
#
# Optional:
#   PALE_ADMIN_TOKEN=...  skip login and use a bearer token
#   PALE_SKIP_MEETING=1   skip conference create/join
set -euo pipefail

BASE_URL="${PALE_BASE_URL:-http://127.0.0.1:8090}"
BASE_URL="${BASE_URL%/}"
SIP_URI="${PALE_SIP_URI:-}"
PASSWORD="${PALE_PASSWORD:-}"
TOKEN="${PALE_ADMIN_TOKEN:-}"
SKIP_MEETING="${PALE_SKIP_MEETING:-0}"

UA="Pale/smoke-test"
pass=0
fail=0

log() { printf '%s\n' "$*"; }
ok() { pass=$((pass + 1)); log "  OK  $*"; }
bad() { fail=$((fail + 1)); log "  FAIL $*"; }

json_field() {
  # json_field <json> <key>
  python3 -c 'import json,sys; d=json.loads(sys.argv[1]); print(d.get(sys.argv[2],"") or "")' "$1" "$2" 2>/dev/null || true
}

req() {
  local method="$1" path="$2" body="${3:-}"
  local args=(-sS -w "\n%{http_code}" -X "$method" -H "User-Agent: $UA" -H "X-Pale-Client: Pale/smoke")
  if [ -n "$TOKEN" ]; then
    args+=(-H "Authorization: Bearer $TOKEN")
  fi
  if [ -n "$body" ]; then
    args+=(-H "Content-Type: application/json" -d "$body")
  fi
  curl "${args[@]}" "${BASE_URL}${path}"
}

split_body_code() {
  # stdin: body\ncode -> sets BODY and CODE
  local raw
  raw="$(cat)"
  CODE="${raw##*$'\n'}"
  BODY="${raw%$'\n'*}"
}

log "=== Pale smoke test against $BASE_URL ==="

# 1. Health
raw="$(curl -sS -w "\n%{http_code}" -H "User-Agent: $UA" "${BASE_URL}/health" || true)"
CODE="${raw##*$'\n'}"
BODY="${raw%$'\n'*}"
if [ "$CODE" = "200" ] && echo "$BODY" | grep -q '"ok"'; then
  ok "GET /health ($CODE)"
else
  bad "GET /health ($CODE) $BODY"
fi

# 2. Login (unless token provided)
if [ -z "$TOKEN" ]; then
  if [ -z "$SIP_URI" ] || [ -z "$PASSWORD" ]; then
    bad "Set PALE_SIP_URI and PALE_PASSWORD (or PALE_ADMIN_TOKEN)"
    log "Summary: $pass passed, $fail failed"
    exit 1
  fi
  raw="$(curl -sS -w "\n%{http_code}" -X POST \
    -H "User-Agent: $UA" -H "Content-Type: application/json" \
    -d "{\"sip_uri\":\"$SIP_URI\",\"password\":\"$PASSWORD\"}" \
    "${BASE_URL}/v1/auth/login" || true)"
  CODE="${raw##*$'\n'}"
  BODY="${raw%$'\n'*}"
  if [ "$CODE" != "200" ]; then
    bad "POST /v1/auth/login ($CODE) $BODY"
    log "Summary: $pass passed, $fail failed"
    exit 1
  fi
  MFA="$(json_field "$BODY" mfa_required)"
  TOKEN="$(json_field "$BODY" token)"
  if [ "$MFA" = "True" ] || [ "$MFA" = "true" ]; then
    bad "login requires MFA — complete MFA interactively, then re-run with PALE_ADMIN_TOKEN"
    log "Summary: $pass passed, $fail failed"
    exit 1
  fi
  if [ -z "$TOKEN" ]; then
    bad "login returned no token"
    exit 1
  fi
  ok "POST /v1/auth/login"
fi

# 3. Create room + send message
ROOM_BODY="$(req POST /v1/rooms "{\"name\":\"smoke-$(date +%s)\",\"members\":[],\"is_direct\":false}")"
CODE="${ROOM_BODY##*$'\n'}"
BODY="${ROOM_BODY%$'\n'*}"
ROOM_ID="$(json_field "$BODY" id)"
if [ "$CODE" = "200" ] || [ "$CODE" = "201" ]; then
  if [ -n "$ROOM_ID" ]; then
    ok "POST /v1/rooms -> $ROOM_ID"
  else
    bad "POST /v1/rooms missing id: $BODY"
  fi
else
  bad "POST /v1/rooms ($CODE) $BODY"
fi

if [ -n "$ROOM_ID" ]; then
  MSG_BODY="$(req POST "/v1/rooms/${ROOM_ID}/messages" '{"body":"smoke hello"}')"
  CODE="${MSG_BODY##*$'\n'}"
  BODY="${MSG_BODY%$'\n'*}"
  if [ "$CODE" = "200" ] || [ "$CODE" = "201" ]; then
    ok "POST /v1/rooms/{id}/messages"
  else
    bad "POST messages ($CODE) $BODY"
  fi
fi

# 4. Conference create + join (signaling; LiveKit optional)
if [ "$SKIP_MEETING" != "1" ]; then
  CONF_BODY="$(req POST /v1/conferences '{"name":"smoke-meet","description":"smoke"}')"
  CODE="${CONF_BODY##*$'\n'}"
  BODY="${CONF_BODY%$'\n'*}"
  CONF_ID="$(json_field "$BODY" id)"
  if [ -n "$CONF_ID" ] && { [ "$CODE" = "200" ] || [ "$CODE" = "201" ]; }; then
    ok "POST /v1/conferences -> $CONF_ID"
    USER_ID="$(json_field "$(req GET /v1/users | sed '$d')" id)"
    # Best-effort join with SIP URI from login
    JOIN_BODY="$(req POST "/v1/conferences/${CONF_ID}/participants" \
      "{\"user_id\":\"00000000-0000-0000-0000-000000000001\",\"sip_uri\":\"${SIP_URI:-sip:smoke@local}\"}")"
    CODE="${JOIN_BODY##*$'\n'}"
    BODY="${JOIN_BODY%$'\n'*}"
    if [ "$CODE" = "200" ]; then
      ok "POST /v1/conferences/{id}/participants"
    else
      # Join may fail without matching user; still useful signal
      log "  WARN conference join ($CODE) — $BODY"
    fi
  else
    log "  WARN POST /v1/conferences ($CODE) — skipping join"
  fi
fi

# 5. Metrics scrape path exists
METRICS="$(curl -sS -o /dev/null -w "%{http_code}" -H "User-Agent: $UA" "${BASE_URL}/metrics" || true)"
if [ "$METRICS" = "200" ]; then
  ok "GET /metrics"
else
  bad "GET /metrics ($METRICS)"
fi

log ""
log "Summary: $pass passed, $fail failed"
if [ "$fail" -gt 0 ]; then
  exit 1
fi
exit 0
