# DLP golden path lab

Goal: create a block policy, send a matching chat message, observe the send
failure and a recorded violation, then export violations CSV.

Requires a running Pale Server and an admin bearer token
(`PALE_SERVER_TOKEN` or admin session).

## Prerequisites

```bash
export PALE_BASE_URL=http://127.0.0.1:8090
export TOKEN='your-admin-or-server-token'
UA=(-H "User-Agent: Pale/dlp-lab" -H "X-Pale-Client: Pale/dlp-lab" \
    -H "Authorization: Bearer $TOKEN" -H "Content-Type: application/json")
```

## 1. Create a block policy

Pattern is a **regex**. Example: flag SSN-like sequences and credit-card-like
digit groups used only in labs (not a full compliance catalog).

```bash
curl -sS "${UA[@]}" -X POST "$PALE_BASE_URL/v1/admin/dlp/policies" -d '{
  "name": "Lab SSN block",
  "description": "Blocks lab SSN pattern in chat and file text scan",
  "pattern": "\\b\\d{3}-\\d{2}-\\d{4}\\b",
  "action": "block",
  "enabled": true
}'
```

List policies:

```bash
curl -sS "${UA[@]}" "$PALE_BASE_URL/v1/admin/dlp/policies" | python3 -m json.tool
```

Preview without recording (admin scan API):

```bash
curl -sS "${UA[@]}" -X POST "$PALE_BASE_URL/v1/admin/dlp/scan" -d '{
  "content": "patient ssn 123-45-6789"
}'
```

## 2. Send a clean message (must succeed)

```bash
ROOM=$(curl -sS "${UA[@]}" -X POST "$PALE_BASE_URL/v1/rooms" -d '{
  "name": "dlp-lab",
  "members": [],
  "is_direct": false
}')
ROOM_ID=$(python3 -c 'import json,sys; print(json.load(sys.stdin)["id"])' <<<"$ROOM")
echo "room=$ROOM_ID"

curl -sS -w "\nHTTP %{http_code}\n" "${UA[@]}" \
  -X POST "$PALE_BASE_URL/v1/rooms/${ROOM_ID}/messages" \
  -d '{"body":"hello from dlp lab"}'
# Expect HTTP 200/201
```

## 3. Send a blocked message (must fail)

```bash
curl -sS -w "\nHTTP %{http_code}\n" "${UA[@]}" \
  -X POST "$PALE_BASE_URL/v1/rooms/${ROOM_ID}/messages" \
  -d '{"body":"ssn 123-45-6789 must not send"}'
# Expect non-2xx with message blocked by DLP policy
```

## 4. Review violations and export CSV

```bash
curl -sS "${UA[@]}" "$PALE_BASE_URL/v1/admin/dlp/violations" | python3 -m json.tool

curl -sS "${UA[@]}" \
  "$PALE_BASE_URL/v1/admin/dlp/violations/export.csv" \
  -o /tmp/dlp-violations.csv
head /tmp/dlp-violations.csv
```

## 5. File upload path (optional)

Uploads are scanned with the same DLP engine (and ClamAV when configured).
With `PALE_ATP_REQUIRED=true` and no ClamAV, uploads fail closed for malware —
see [storage-atp.md](storage-atp.md).

To exercise DLP on a text-like upload body, use the admin file upload path in
the client or API with a file whose content matches the policy pattern; expect
`dlp_status` blocked / upload rejected.

## 6. Smoke integration

`scripts/smoke-test.sh` can run a DLP block check when
`PALE_SMOKE_DLP=1` and an admin token is available (creates a temporary
policy, asserts send fails, deletes policy when possible).

```bash
export PALE_ADMIN_TOKEN="$TOKEN"
export PALE_SMOKE_DLP=1
./scripts/smoke-test.sh
```

## 7. Evidence

```bash
export PALE_ADMIN_TOKEN="$TOKEN"
./scripts/export-evidence-pack.sh
# includes dlp-policies.json and dlp-violations.csv when available
```

## Failure modes

| Symptom | Check |
|---------|--------|
| Pattern rejected | Invalid regex — fix pattern |
| Message not blocked | Policy `enabled: false` or action is `warn`/`audit` |
| Message blocked but no violation | Policy action or scan recording path |
| File upload always blocked | `PALE_ATP_REQUIRED` without ClamAV |

## Out of scope

- Full eDiscovery case workflow proof (separate lab)
- CASB product replacement
- Natural-language policy authoring

See [regulated-midmarket.env.example](regulated-midmarket.env.example) and
[MILESTONES.md](../MILESTONES.md).
