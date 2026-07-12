# High availability topology for Pale

Pale Server today keeps **SIP registrations, sessions, and hot caches in
process memory**, with write-through to PostgreSQL for durable records. That
shapes how you scale.

## Supported production shapes

### A. Single node (recommended until edge SIP exists)

```
Clients → TLS edge (Caddy/nginx) → pale-server (1 replica)
                                  → Postgres (managed HA)
                                  → coturn (public IP)
                                  → LiveKit / ClamAV / MinIO (optional)
```

- **RPO**: last successful `pg_dump` + data volume backup  
- **RTO**: replace VM + restore dump + restart compose  
- SIP registrar = one process. Vertical scale (CPU/RAM) first.

### B. Split HTTP API scale-out (advanced)

Run **one** `udp-parser` registrar instance for SIP. Run additional pale-server
instances **only if** they do not own SIP transports (or put OpenSIPS/Kamailio
in front as the only SIP edge).

```
SIP clients ──SIP TLS──► OpenSIPS/Kamailio ──► media/SBC
HTTP clients ──HTTPS───► LB ──► pale-server-api × N  (shared Postgres)
```

Constraints:

| State | Shared? | Implication |
|-------|---------|-------------|
| Postgres | Yes | Users, rooms, messages, CDR, files metadata |
| Admin/user sessions | Process-local | Sticky sessions or shared session store (not built-in) |
| SIP registrations | Process-local | **One** registrar; clients re-REGISTER on fail |
| SSE connections | Process-local | Sticky LB recommended for `/v1/events` |
| File blobs | Local disk or S3 | Use MinIO/S3 for multi-node |

Environment knobs:

```bash
# Instance role label for ops/metrics (documentation only today)
PALE_INSTANCE_ROLE=registrar   # or api
PALE_HTTP_ADDR=0.0.0.0:8080
# Only on registrar:
PALE_SIP_BACKEND=udp-parser
PALE_SIP_TLS_CERT=...
# On API-only nodes you may set:
# PALE_SIP_BACKEND=pjsip   # no-native build: SIP disabled — HTTP/PBX records only
```

### C. Managed dependencies

Prefer managed PostgreSQL with automated failover. Point all pale-server
instances at the same `PALE_DATABASE_URL`. Never expose Postgres publicly.

## Failover checklist

1. Health: `GET /health` and `GET /ready`  
2. Promote standby Postgres if needed; update DNS for API  
3. Start pale-server with same secrets (`PALE_STORAGE_KEY` must match ciphertext)  
4. Clients re-register SIP (expect brief calling outage)  
5. Re-attach coturn `external-ip` and LiveKit if used  

## What is not implemented

- Multi-active SIP registrar with shared registration table  
- Shared session store for bearer tokens across API nodes  
- Automatic leader election for PBX runtime  

Track those as product work before claiming multi-region active-active.

See [PRODUCTION.md](PRODUCTION.md) for single-tenant go-live.
