# SSO OIDC golden path (Keycloak or Entra ID)

Goal: a user can sign in via OIDC, and when a conditional-access policy requires
MFA, Pale returns an MFA challenge before issuing a full session.

Requires a running Pale Server with admin credentials and a real OIDC IdP
(Keycloak lab or Microsoft Entra app registration).

## Prerequisites

- Pale HTTP API reachable (example: `https://pale.example.com` or lab `http://127.0.0.1:8090`)
- Admin bearer: break-glass `PALE_SERVER_TOKEN` or `POST /v1/admin/login`
- IdP client: authorization code flow, confidential client
- Redirect URI registered on the IdP **exactly** as stored in Pale

Recommended redirect URI:

```text
https://pale.example.com/auth/sso/callback
```

For local lab:

```text
http://127.0.0.1:1420/auth/sso/callback
```

(Adjust to the URL your Pale client actually uses for the callback page.)

Optional: set `PALE_OIDC_CA_BUNDLE` to a PEM file if the IdP uses a private CA.

## 1. Create the SSO provider in Pale

```bash
export PALE_BASE_URL=http://127.0.0.1:8090
export TOKEN='your-admin-or-server-token'
UA=(-H "User-Agent: Pale/sso-lab" -H "X-Pale-Client: Pale/sso-lab" \
    -H "Authorization: Bearer $TOKEN" -H "Content-Type: application/json")

curl -sS "${UA[@]}" -X POST "$PALE_BASE_URL/v1/admin/sso-providers" -d '{
  "name": "Keycloak lab",
  "provider_type": "oidc",
  "client_id": "pale-client",
  "client_secret": "replace-me",
  "issuer_url": "https://keycloak.example.com/realms/pale",
  "redirect_uri": "http://127.0.0.1:1420/auth/sso/callback",
  "enabled": true,
  "groups_claim": "groups",
  "default_role": "user",
  "role_mappings": { "pale-admins": "admin", "staff": "user" }
}'
```

Notes:

- `issuer_url` must match the OIDC issuer in the discovery document
  (`{issuer}/.well-known/openid-configuration`).
- Pale discovers authorization/token/JWKS endpoints from that document.
- List providers (admin): `GET /v1/admin/sso-providers`
- **Public list for login wizard** (no secrets): `GET /v1/auth/sso/providers`
- `role_mappings` maps IdP group claim values → Pale roles on every SSO login
  (auto-provision and existing users).

### Entra ID sketch

| Field | Value |
|-------|--------|
| issuer_url | `https://login.microsoftonline.com/{tenant-id}/v2.0` |
| client_id | Application (client) ID |
| client_secret | Client secret value |
| redirect_uri | Same URI registered under Authentication → Redirect URIs |

## 2. Start login (client wizard or API)

The Pale **Setup Wizard** loads `GET /v1/auth/sso/providers` and shows
**Sign in with …** buttons. Prefer that path for end users.

API equivalent:

```text
GET /v1/auth/sso/{provider_id}/login
```

Pale returns a redirect URL (authorization endpoint + state/nonce). Complete
login at the IdP, then post the callback payload the client receives to:

```text
POST /v1/auth/sso/callback
```

The desktop/web client handles `/auth/sso/callback?code=&state=` automatically
when the IdP redirect URI points at the app origin.

Successful callback issues a user session token (or `mfa_required` / MFA-pending
token when conditional access demands MFA).

## 3. Require MFA via conditional access

```bash
curl -sS "${UA[@]}" -X POST "$PALE_BASE_URL/v1/admin/conditional-access" -d '{
  "name": "Require MFA for all",
  "conditions": {},
  "actions": { "require_mfa": true, "block": false },
  "enabled": true
}'
```

Re-test SSO (or password login). Expected:

- Response indicates MFA is required, **or**
- Session role is MFA-pending until TOTP is completed

Users complete enrollment in the Pale client (setup wizard / server settings)
when MFA is forced and not yet enrolled.

## 4. Validation

```bash
# Provider inventory
curl -sS "${UA[@]}" "$PALE_BASE_URL/v1/admin/sso-providers" | python3 -m json.tool

# Enterprise validation (SSO readiness appears in report)
curl -sS "${UA[@]}" \
  "$PALE_BASE_URL/v1/admin/enterprise-integrations/validation.csv" | head

# Evidence pack
export PALE_ADMIN_TOKEN="$TOKEN"
./scripts/export-evidence-pack.sh
```

## Failure modes (expected)

| Symptom | Check |
|---------|--------|
| Discovery TLS error | `PALE_OIDC_CA_BUNDLE`, IdP cert chain |
| `redirect_uri_mismatch` | Exact match on IdP and Pale provider |
| Invalid client | Client secret / app type (confidential) |
| User not provisioned | First login JIT / SCIM / admin-created user for that SIP/email identity |
| Always MFA pending | CA policy + user has no TOTP enrollment |

## Out of scope for this lab

- SAML (OIDC path only here)
- Full SCIM lifecycle automation proof
- Group → custom role claim mapping beyond current product support

See [regulated-midmarket.env.example](regulated-midmarket.env.example) and
[PRODUCTION.md](PRODUCTION.md).
