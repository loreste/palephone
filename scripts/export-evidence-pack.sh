#!/usr/bin/env bash
# Export an auditor-facing evidence pack from a running Pale Server.
#
# Usage:
#   export PALE_BASE_URL=http://127.0.0.1:8090
#   export PALE_ADMIN_TOKEN='...'   # server token or admin session bearer
#   ./scripts/export-evidence-pack.sh
#
# Optional:
#   PALE_EVIDENCE_DIR=./evidence/run-1
#   PALE_SKIP_RESTORE_NOTE=1
set -euo pipefail

BASE_URL="${PALE_BASE_URL:-http://127.0.0.1:8090}"
BASE_URL="${BASE_URL%/}"
TOKEN="${PALE_ADMIN_TOKEN:-}"
OUT_DIR="${PALE_EVIDENCE_DIR:-./evidence/$(date -u +%Y%m%dT%H%M%SZ)}"
UA="Pale/evidence-pack"

if [ -z "$TOKEN" ]; then
  echo "error: set PALE_ADMIN_TOKEN (break-glass server token or admin session)" >&2
  exit 1
fi

mkdir -p "$OUT_DIR"
auth=(-H "User-Agent: $UA" -H "X-Pale-Client: Pale/evidence" -H "Authorization: Bearer $TOKEN")

fetch() {
  local path="$1" dest="$2"
  local code
  code="$(curl -sS -o "$dest" -w "%{http_code}" "${auth[@]}" "${BASE_URL}${path}" || echo "000")"
  if [ "$code" != "200" ]; then
    echo "WARN $path -> HTTP $code (saved response body if any)" >&2
    echo "http_status=$code" > "${dest}.status"
    return 1
  fi
  echo "OK   $path -> $dest"
  return 0
}

echo "=== Pale evidence pack → $OUT_DIR ==="
echo "base_url=$BASE_URL" > "$OUT_DIR/MANIFEST.txt"
echo "exported_at_utc=$(date -u +%Y-%m-%dT%H:%M:%SZ)" >> "$OUT_DIR/MANIFEST.txt"
echo "hostname=$(hostname 2>/dev/null || echo unknown)" >> "$OUT_DIR/MANIFEST.txt"

# Public probes (still send Pale UA for consistency)
curl -sS -H "User-Agent: $UA" "${BASE_URL}/health" -o "$OUT_DIR/health.json" || true
curl -sS -H "User-Agent: $UA" "${BASE_URL}/ready" -o "$OUT_DIR/ready.json" || true
echo "OK   /health /ready"

fail=0
fetch "/v1/admin/enterprise-integrations/validation.csv" "$OUT_DIR/enterprise-validation.csv" || fail=$((fail + 1))
fetch "/v1/admin/enterprise-integrations/validation" "$OUT_DIR/enterprise-validation.json" || fail=$((fail + 1))
fetch "/v1/admin/enterprise-integrations/readiness" "$OUT_DIR/enterprise-readiness.json" || fail=$((fail + 1))
fetch "/v1/admin/enterprise-integrations/health" "$OUT_DIR/enterprise-health.json" || fail=$((fail + 1))
fetch "/v1/admin/enterprise-integrations/provider-probes" "$OUT_DIR/enterprise-provider-probes.json" || fail=$((fail + 1))
fetch "/v1/admin/enterprise-integrations/deployment-plan" "$OUT_DIR/enterprise-deployment-plan.json" || fail=$((fail + 1))
fetch "/v1/admin/security-score" "$OUT_DIR/security-score.json" || fail=$((fail + 1))
fetch "/v1/admin/audit/export.csv" "$OUT_DIR/audit-export.csv" || fail=$((fail + 1))
# Limit audit list noise — export.csv is the primary artifact
fetch "/v1/admin/audit" "$OUT_DIR/audit-events.json" || fail=$((fail + 1))
fetch "/v1/admin/dlp/policies" "$OUT_DIR/dlp-policies.json" || fail=$((fail + 1))
fetch "/v1/admin/dlp/violations/export.csv" "$OUT_DIR/dlp-violations.csv" || true
fetch "/v1/admin/conditional-access" "$OUT_DIR/conditional-access.json" || fail=$((fail + 1))
fetch "/v1/admin/sso-providers" "$OUT_DIR/sso-providers.json" || fail=$((fail + 1))

# Metrics (may be long)
if curl -sS -o "$OUT_DIR/metrics.txt" -w "%{http_code}" -H "User-Agent: $UA" "${BASE_URL}/metrics" | grep -q '^200$'; then
  echo "OK   /metrics"
else
  echo "WARN /metrics not 200" >&2
fi

if [ "${PALE_SKIP_RESTORE_NOTE:-0}" != "1" ]; then
  cat > "$OUT_DIR/RESTORE_NOTE.txt" <<'EOF'
Operator restore drill
---------------------
1. Confirm backup script exists: scripts/backup.sh
2. Run restore drill per docs/deploy/secrets-rotation.md and scripts/restore-drill.sh
3. Attach the latest successful restore-drill log next to this evidence pack.
4. Record RPO/RTO from docs/deploy/ha.md for this topology (single-node default).

This pack proves API-side configuration and validation state at export time.
It does not by itself prove backup restore or carrier certification.
EOF
  echo "OK   RESTORE_NOTE.txt"
fi

# Checksums for integrity of the pack itself
(
  cd "$OUT_DIR"
  if command -v shasum >/dev/null 2>&1; then
    find . -type f ! -name 'SHA256SUMS.txt' -print0 | sort -z | xargs -0 shasum -a 256 > SHA256SUMS.txt
  elif command -v sha256sum >/dev/null 2>&1; then
    find . -type f ! -name 'SHA256SUMS.txt' -print0 | sort -z | xargs -0 sha256sum > SHA256SUMS.txt
  fi
)

echo "fail_count=$fail" >> "$OUT_DIR/MANIFEST.txt"
echo ""
echo "Pack written to $OUT_DIR"
if [ "$fail" -gt 0 ]; then
  echo "Completed with $fail fetch warning(s) — review *.status files" >&2
  exit 1
fi
exit 0
