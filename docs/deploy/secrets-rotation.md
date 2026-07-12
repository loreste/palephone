# Secret rotation

## Generate candidates (API)

As an admin bearer:

```bash
# Suggest new admin password, server token, storage key (does not apply them)
curl -sS -X POST "$BASE/v1/admin/rotate-admin-password" \
  -H "Authorization: Bearer $TOKEN" -H "User-Agent: Pale/admin"

# Suggest new server token only
curl -sS -X POST "$BASE/v1/admin/rotate-server-token" \
  -H "Authorization: Bearer $TOKEN" -H "User-Agent: Pale/admin"
```

Copy values into `/etc/pale-server/pale-server.env` or Docker `.env`, then
**restart** pale-server. Secrets are not written to the database by these
endpoints.

## Impact matrix

| Secret | Rotation impact |
|--------|-----------------|
| `PALE_ADMIN_PASSWORD` | Admin login password changes after restart |
| `PALE_SERVER_TOKEN` | Break-glass static bearer invalidated |
| `PALE_STORAGE_KEY` | May make prior field ciphertext unreadable — treat as **destructive** unless re-encrypt is planned |
| `POSTGRES_PASSWORD` | Update DB + `PALE_DATABASE_URL` together |
| `TURN_SECRET` | Align coturn and pale-server simultaneously |
| `NATS_TOKEN` | Align NATS and `NATS_URL` |
| Session bearer tokens | Expire naturally (12h) or revoke via session APIs |

## Procedure

1. Schedule a maintenance window for storage-key changes  
2. Generate candidates via API or `openssl rand -base64 32`  
3. Update env on all instances  
4. Restart pale-server (and coturn/NATS if their secrets changed)  
5. Sign in as admin; verify `/ready` and a test call/message  
6. Record audit events (`secrets.generated`, `server.token_generated`)  

## Break-glass

Keep `PALE_SERVER_TOKEN` offline. Prefer user sessions for day-to-day admin.
After a leak, rotate token and review audit log for the token’s prior use.
