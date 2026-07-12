# Load and capacity scripts

These are **repeatable lab tools**, not a certified 10k-viewer report. Run them
against a staging Pale Server with known credentials.

| Script | Purpose |
|--------|---------|
| `../smoke-test.sh` | Functional smoke: health, login, chat, meeting, metrics |
| `chat-burst.sh` | Sequential message burst to one room |
| `sse-fanout.sh` | Concurrent SSE `/v1/events` holders |
| `meeting-join-storm.sh` | Concurrent conference join (signaling) |

## Example session

```bash
export PALE_BASE_URL=https://pale.staging.example.com
export PALE_SIP_URI=sip:admin@example.com
export PALE_PASSWORD='...'

# 1. Smoke
./scripts/smoke-test.sh
export PALE_ADMIN_TOKEN=...   # from login or admin session

# 2. Create a room id from smoke or admin UI
export PALE_ROOM_ID=...

# 3. Chat + SSE
./scripts/load/chat-burst.sh 100
./scripts/load/sse-fanout.sh 50 20

# 4. Meetings (signaling; set LiveKit for real media)
./scripts/load/meeting-join-storm.sh 40 10
```

## Interpreting results

- Prefer **p95 latency** and **error rate** from a reverse-proxy access log.
- Pale Server keeps SIP registrar state in process memory — do not scale
  registrar replicas without an edge SIP proxy (see [ha.md](../../docs/deploy/ha.md)).
- Town hall 10k viewers require LiveKit (or another SFU/broadcast path) sized
  for fanout; these scripts only exercise pale-server control plane.
