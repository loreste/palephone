#!/usr/bin/env bash
# Compliance smoke (Phase 0.5 / M2.4–2.7): MFA policy surface, DLP block,
# retention config, dual-admin knob awareness, ClamAV/ATP readiness.
#
# Usage:
#   export PALE_BASE_URL=http://127.0.0.1:8090
#   export PALE_ADMIN_TOKEN='...'   # server token or admin session
#   ./scripts/compliance-smoke.sh
#
# Optional:
#   PALE_SMOKE_DLP=1   (default 1) create temp DLP and assert block
#   PALE_EXPECT_ATP=1  require ClamAV configured in /ready
set -euo pipefail

BASE_URL="${PALE_BASE_URL:-http://127.0.0.1:8090}"
BASE_URL="${BASE_URL%/}"
TOKEN="${PALE_ADMIN_TOKEN:-}"
SMOKE_DLP="${PALE_SMOKE_DLP:-1}"
EXPECT_ATP="${PALE_EXPECT_ATP:-0}"
UA="Pale/compliance-smoke"

pass=0
fail=0
log() { printf '%s\n' "$*"; }
ok() { log "PASS  $*"; pass=$((pass + 1)); }
bad() { log "FAIL  $*"; fail=$((fail + 1)); }

if [ -z "$TOKEN" ]; then
  log "error: set PALE_ADMIN_TOKEN" >&2
  exit 1
fi

auth=(-H "User-Agent: $UA" -H "X-Pale-Client: $UA" -H "Authorization: Bearer $TOKEN" -H "Content-Type: application/json")

# ── Health / ready ──────────────────────────────────────────────────────────
code=$(curl -sS -o /tmp/pale-c-health.json -w '%{http_code}' "${auth[@]}" "$BASE_URL/health" || true)
if [ "$code" = "200" ]; then ok "GET /health"; else bad "GET /health ($code)"; fi

code=$(curl -sS -o /tmp/pale-c-ready.json -w '%{http_code}' "${auth[@]}" "$BASE_URL/ready" || true)
if [ "$code" = "200" ]; then
  ok "GET /ready"
  if [ "$EXPECT_ATP" = "1" ]; then
    if grep -q '"clamav_configured":true' /tmp/pale-c-ready.json 2>/dev/null; then
      ok "ClamAV configured (ready)"
    else
      bad "ClamAV not configured but PALE_EXPECT_ATP=1"
    fi
  else
    ok "ATP check skipped (set PALE_EXPECT_ATP=1 to require ClamAV)"
  fi
else
  bad "GET /ready ($code)"
fi

# ── Conditional access surface ──────────────────────────────────────────────
code=$(curl -sS -o /tmp/pale-c-ca.json -w '%{http_code}' "${auth[@]}" \
  "$BASE_URL/v1/admin/conditional-access" || true)
if [ "$code" = "200" ]; then ok "GET conditional-access policies"; else bad "conditional-access ($code)"; fi

# ── DLP ─────────────────────────────────────────────────────────────────────
if [ "$SMOKE_DLP" = "1" ]; then
  MARKER="COMPLIANCE-DLP-$(date +%s)-ZZZZ"
  code=$(curl -sS -o /tmp/pale-c-dlp.json -w '%{http_code}' "${auth[@]}" \
    -X POST "$BASE_URL/v1/admin/dlp/policies" -d "{
      \"name\": \"compliance-smoke-$MARKER\",
      \"enabled\": true,
      \"action\": \"block\",
      \"patterns\": [\"$MARKER\"]
    }" || true)
  if [ "$code" = "200" ] || [ "$code" = "201" ]; then
    ok "create DLP policy"
    POLICY_ID=$(python3 -c "import json;print(json.load(open('/tmp/pale-c-dlp.json')).get('id',''))" 2>/dev/null || true)
    # Room + blocked message
    code=$(curl -sS -o /tmp/pale-c-room.json -w '%{http_code}' "${auth[@]}" \
      -X POST "$BASE_URL/v1/rooms" -d '{"name":"compliance-smoke","is_direct":false}' || true)
    ROOM_ID=$(python3 -c "import json;print(json.load(open('/tmp/pale-c-room.json')).get('id',''))" 2>/dev/null || true)
    if [ -n "$ROOM_ID" ]; then
      code=$(curl -sS -o /tmp/pale-c-msg.json -w '%{http_code}' "${auth[@]}" \
        -X POST "$BASE_URL/v1/rooms/$ROOM_ID/messages" \
        -d "{\"body\":\"blocked content $MARKER\"}" || true)
      if [ "$code" = "403" ] || [ "$code" = "409" ] || [ "$code" = "422" ]; then
        ok "DLP blocked message send ($code)"
      elif [ "$code" = "200" ] || [ "$code" = "201" ]; then
        bad "DLP did not block message (HTTP $code)"
      else
        bad "DLP message unexpected status $code"
      fi
    else
      bad "create room for DLP ($code)"
    fi
    if [ -n "${POLICY_ID:-}" ]; then
      curl -sS -o /dev/null -w '' "${auth[@]}" -X DELETE "$BASE_URL/v1/admin/dlp/policies/$POLICY_ID" || true
    fi
  else
    bad "create DLP policy ($code)"
  fi
else
  ok "DLP smoke skipped"
fi

# ── Retention policies list ─────────────────────────────────────────────────
code=$(curl -sS -o /tmp/pale-c-ret.json -w '%{http_code}' "${auth[@]}" \
  "$BASE_URL/v1/admin/retention-policies" || true)
if [ "$code" = "200" ]; then ok "GET retention policies"; else bad "retention policies ($code)"; fi

# ── eDiscovery cases list ───────────────────────────────────────────────────
code=$(curl -sS -o /tmp/pale-c-ed.json -w '%{http_code}' "${auth[@]}" \
  "$BASE_URL/v1/admin/ediscovery/cases" || true)
if [ "$code" = "200" ]; then ok "GET ediscovery cases"; else bad "ediscovery ($code)"; fi

# ── Enterprise validation CSV ───────────────────────────────────────────────
code=$(curl -sS -o /tmp/pale-c-val.csv -w '%{http_code}' "${auth[@]}" \
  "$BASE_URL/v1/admin/enterprise-integrations/validation.csv" || true)
if [ "$code" = "200" ] && [ -s /tmp/pale-c-val.csv ]; then
  ok "validation CSV non-empty"
else
  bad "validation CSV ($code)"
fi

# ── Dual-admin awareness ────────────────────────────────────────────────────
if [ "${PALE_REQUIRE_DUAL_ADMIN:-}" = "true" ] || [ "${PALE_REQUIRE_DUAL_ADMIN:-}" = "1" ]; then
  ok "PALE_REQUIRE_DUAL_ADMIN enabled in environment"
else
  ok "dual-admin not forced in this env (document for production)"
fi

log ""
log "compliance-smoke: $pass passed, $fail failed"
if [ "$fail" -gt 0 ]; then exit 1; fi
