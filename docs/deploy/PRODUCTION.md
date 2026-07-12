# Pale Production Runbook

Operator guide for a first production tenant. Read this before exposing Pale to
users outside a lab network.

**Scope of this guide (Phase 0 — P0 Production):**

| Included | Not included yet |
|----------|------------------|
| Secure deploy on Docker / Linux / Windows / Kubernetes | Multi-party SFU at scale (LiveKit profile optional) |
| Native chat, presence, admin | Certified PSTN / E911 carriers |
| 1:1 SIP over TLS + SRTP | 10k town hall load proof |
| Postgres backups | Full HA multi-registrar |
| TLS edge, private DB/NATS | Malware scanner adapters (ClamAV) without extra setup |

Product honesty: many enterprise admin screens are **readiness records**. Do not
market features that still need external providers. See [NEXT_STEPS.md](../NEXT_STEPS.md).

---

## Architecture (target)

```
Pale clients ──HTTPS──► TLS edge (Caddy / nginx / Ingress)
                   └──SIP TLS:5061──► pale-server (udp-parser registrar)
                   └──TURN:3478────► coturn (public IP + external-ip)

pale-server ──► PostgreSQL (private)
            ──► NATS (private, auth token)
            ──► LiveKit (optional, multi-party)
```

**Single active SIP registrar.** pale-server keeps registration state in process
memory. Run one registrar instance (or put OpenSIPS/Kamailio in front).

---

## Port matrix

| Port | Service | Public? |
|------|---------|---------|
| 443 | HTTPS API (proxy) | Yes |
| 8090 | pale-server HTTP (direct) | Private / admin only |
| 5060/tcp | SIP TCP | Optional (prefer TLS) |
| 5061/tcp | SIP TLS | Yes (or SBC only) |
| 3478 / 5349 | TURN | Yes |
| 49152–65535/udp | TURN relay | Yes |
| 5432 | Postgres | **No** |
| 4222 / 8222 | NATS | **No** |
| 8080 | pale-server in-cluster | Cluster only |
| `/metrics` | Prometheus | **Private scrape only** |

---

## Secrets

Generate once per tenant:

```bash
./scripts/generate-secrets.sh
```

Creates `.env` with:

- `PALE_SERVER_TOKEN` — **break-glass** superadmin bearer. Prefer admin user sessions day-to-day.
- `PALE_ADMIN_PASSWORD` — admin UI login (min 24 chars)
- `PALE_STORAGE_KEY` — local encryption material (rotating loses ciphertext)
- `POSTGRES_PASSWORD`, `TURN_SECRET`, `NATS_TOKEN`

**Rotation:** rotating `PALE_SERVER_TOKEN` invalidates static bearer clients.
Rotating `PALE_STORAGE_KEY` can make previously encrypted local files unreadable.
Admin password rotation is supported via admin secret-rotation APIs.

---

## TLS certificates

Place PEM files (or point `CERTS_DIR`):

```
certs/fullchain.pem
certs/privkey.pem
```

Used for **SIP TLS** (required in production compose). HTTP TLS is preferably
terminated at Caddy (`--profile proxy`) or Kubernetes Ingress so pale-server
health checks stay plain HTTP on the private network.

Minimum SAN/CN: the hostname clients use for API and SIP.

---

## Deploy paths

### A. Docker Compose (production)

```bash
./scripts/generate-secrets.sh
# Edit .env — set all of:
#   PALE_PUBLIC_HOSTNAME
#   PALE_SIP_EXTERNAL_ADDR      e.g. pale.example.com:5060
#   PALE_SIP_TLS_EXTERNAL_ADDR  e.g. pale.example.com:5061
#   PALE_TURN_SERVER            e.g. turn:pale.example.com:3478
#   TURN_EXTERNAL_IP            public IPv4 of this host
# Place certs in ./certs/

mkdir -p certs
# copy fullchain.pem privkey.pem into certs/

docker compose -f docker-compose.prod.yml up -d --build

# Optional public HTTPS:
docker compose -f docker-compose.prod.yml --profile proxy up -d

# Optional multi-party meetings:
# set PALE_LIVEKIT_URL / API_KEY / API_SECRET in .env
docker compose -f docker-compose.prod.yml --profile meetings up -d

curl -sf http://localhost:8090/health
```

Firewall: allow 443 (if proxy), 5061/tcp, 3478, 5349, and UDP relay range
49152–65535. Do **not** publish Postgres or NATS.

### B. Bare-metal Linux

See [linux.md](linux.md).

```bash
curl -fsSL https://drcpbx.com/install-pale-server.sh | sudo bash
# Prefer SIP backend: udp-parser
# Bind HTTP to 127.0.0.1:8080 and put Caddy/nginx in front with TLS.
```

### C. Windows Server

See [windows.md](windows.md). Installer binds `127.0.0.1:8080` by default.
Enable `udp-parser` for SIP; terminate TLS at IIS/Caddy; use external Postgres
and coturn for multi-user production.

### D. Kubernetes

See [deploy/k8s/README.md](../../deploy/k8s/README.md).

```bash
kubectl apply -f deploy/k8s/namespace.yaml
# secrets, configmap, postgres, pale-server, ingress, networkpolicy
```

Replicas must stay **1** for the registrar until you add a SIP edge.

---

## First admin steps

1. Open the Pale desktop client (or API).
2. Server URL: `https://pale.example.com` (proxy) or `http://host:8090` (lab only).
3. Log in as admin with `PALE_ADMIN_PASSWORD`.
4. Create users + SIP accounts; assign extensions if using PBX features.
5. On each desktop: sign in, confirm SIP registration (TLS), place a test 1:1 call.
6. Create a chat room; send messages between two users.
7. In Admin → Enterprise integrations / Security score, note missing providers.

---

## Backup and restore

### Backup (Postgres)

```bash
# Compose prod
docker compose -f docker-compose.prod.yml exec -T postgres \
  pg_dump -U pale pale | gzip > "pale-$(date +%Y%m%d).sql.gz"

# Or scripts/backup.sh with PGHOST/PGPASSWORD
PGHOST=127.0.0.1 PGPORT=5433 PGUSER=pale PGPASSWORD=... ./scripts/backup.sh
```

Also back up the pale-server data volume (`/data` / `pale-data`) for files and
local artifacts.

### Restore drill (quarterly)

```bash
gunzip -c pale-YYYYMMDD.sql.gz | \
  docker compose -f docker-compose.prod.yml exec -T postgres \
  psql -U pale pale
```

Document RPO/RTO for your org. Single-node default RPO = last successful dump.

### Systemd timer example (Linux)

```ini
# /etc/systemd/system/pale-backup.timer
[Unit]
Description=Daily Pale Postgres backup

[Timer]
OnCalendar=daily
Persistent=true

[Install]
WantedBy=timers.target
```

Pair with a service that runs `scripts/backup.sh` and copies artifacts off-box.

---

## Health and monitoring

| Endpoint | Auth | Use |
|----------|------|-----|
| `GET /health` | none | LB liveness; `status` may be `healthy` or `degraded` |
| `GET /metrics` | none | Prometheus; **do not** expose to the internet |
| Admin enterprise readiness | bearer | Provider inventory / validation UI |

Alert on: process down, `/health` degraded, disk full, cert expiry, coturn down,
Postgres connection failures.

---

## Security checklist

- [ ] TLS for SIP (5061) with real certs
- [ ] SRTP enabled (`PALE_SIP_SRTP=true`)
- [ ] `PALE_SIP_BACKEND=udp-parser` for Docker/no-native builds
- [ ] `PALE_SIP_EXTERNAL_ADDR` / TLS external addr set (not loopback)
- [ ] TURN has `external-ip` and client-facing `PALE_TURN_SERVER`
- [ ] Postgres and NATS not on public interfaces; NATS has auth
- [ ] HTTP API behind TLS proxy or `PALE_HTTP_TLS_*`
- [ ] `PALE_SERVER_TOKEN` treated as break-glass only
- [ ] Admin password ≥ 24 chars
- [ ] Conditional access policy with `require_mfa` if org policy needs MFA (enforced on password + SSO login)
- [ ] DLP policies for chat and files (chat send is blocked when action is Block)
- [ ] LiveKit configured for multi-party meetings; set `PALE_LIVEKIT_REQUIRED=true` to refuse joins without SFU
- [ ] ClamAV (`PALE_CLAMAV_HOST`) + optional `PALE_ATP_REQUIRED=true` (see [storage-atp.md](storage-atp.md))
- [ ] S3/MinIO for file scale (`PALE_S3_*`, `--profile storage`)
- [ ] PSTN lab gateway when dialing the public phone network ([pstn-lab.md](pstn-lab.md))
- [ ] Smoke tests: `./scripts/smoke-test.sh` after deploy
- [ ] `/metrics` private
- [ ] Automated backups + restore drill
- [ ] Firewall allows only required ports

---

## What is not production-ready yet

Do not sell these as complete without extra systems and evidence:

- Multi-party gallery video without LiveKit configured
- PSTN / Operator Connect / E911 without certified carriers and tests
- Town hall 10k scale without load reports
- ATP/malware without ClamAV/YARA (or equivalent) wired and tested
- Matrix E2E chat without a homeserver (native chat works without Matrix)
- Multi-instance HA registrar

---

## Incident notes

| Issue | First checks |
|-------|----------------|
| Clients cannot register | SIP backend, TLS certs, external addr, firewall 5061 |
| One-way audio / no media | TURN external-ip, relay UDP range, `PALE_TURN_SERVER` |
| Login fails | Admin lockout (5 fails / 15 min), IdP if SSO, clock skew |
| Secret leak | Rotate `PALE_SERVER_TOKEN` and admin password; review audit log |
| PG degraded | Disk, connections, `docker compose logs postgres` |
| SIP down after restart | Single replica Recreate; registrations re-REGISTER from clients |

---

## Related docs

- [linux.md](linux.md) — bare-metal
- [windows.md](windows.md) — Windows Server
- [ha.md](ha.md) — high availability topology
- [storage-atp.md](storage-atp.md) — MinIO/S3 and ClamAV
- [pstn-lab.md](pstn-lab.md) / [e911-lab.md](e911-lab.md) — carrier lab
- [secrets-rotation.md](secrets-rotation.md) — secret rotation
- [scripts/restore-drill.sh](../../scripts/restore-drill.sh) — backup restore drill
- [scripts/load/README.md](../../scripts/load/README.md) — load scripts
- [deploy/k8s/README.md](../../deploy/k8s/README.md) — Kubernetes
- [IOS_SETUP.md](../../IOS_SETUP.md) — iOS packaging path
- [NEXT_STEPS.md](../NEXT_STEPS.md) — product roadmap beyond P0
- [README.md](../../README.md) — product overview
