# Object storage (MinIO/S3) and malware scanning (ClamAV)

## MinIO / S3

Pale stores file blobs on local disk by default. For multi-user production scale,
point pale-server at S3-compatible storage:

```bash
PALE_S3_BUCKET=pale
PALE_S3_REGION=us-east-1
PALE_S3_ENDPOINT=http://minio:9000   # or https://s3.amazonaws.com
PALE_S3_ACCESS_KEY=...
PALE_S3_SECRET_KEY=...
```

### Docker production profile

```bash
# .env
PALE_S3_BUCKET=pale
PALE_S3_ENDPOINT=http://minio:9000
PALE_S3_ACCESS_KEY=paleminio
PALE_S3_SECRET_KEY=change-me-min-8-chars

docker compose -f docker-compose.prod.yml --profile storage up -d
```

`minio-init` creates the bucket once. Restart pale-server after MinIO is healthy
so it picks up the S3 client at boot.

## ClamAV / ATP

Upload scanning always runs local EICAR/test patterns. When `PALE_CLAMAV_HOST`
is set (for example `clamav:3310`), pale-server uses the clamd INSTREAM protocol.

```bash
PALE_CLAMAV_HOST=clamav:3310
# Fail closed if ClamAV is down or missing:
PALE_ATP_REQUIRED=true
```

### Docker production profile

```bash
PALE_CLAMAV_HOST=clamav:3310
PALE_ATP_REQUIRED=true

docker compose -f docker-compose.prod.yml --profile atp up -d
```

ClamAV signature download on first start can take several minutes; healthchecks
use a long `start_period`.

| Setting | Behavior |
|---------|----------|
| ClamAV host set, ATP not required | ClamAV scan; on error, allow clean local-pattern files |
| `PALE_ATP_REQUIRED=true` | Block upload if ClamAV is missing or unreachable |
| No ClamAV, ATP not required | Local patterns only (lab) |

Native health check (clamd `zPING`):

```bash
curl -sS "$BASE/v1/admin/atp/clamav/probe" \
  -H "Authorization: Bearer $TOKEN" -H "User-Agent: Pale/admin"
```

Enterprise validation includes the same probe under `workflow.atp_scanner`.

Admin cloud storage status reports S3 when `PALE_S3_*` is active.
