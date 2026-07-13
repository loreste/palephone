# Capacity profile RM-500 (Phase 0.9 / M4)

**Target:** 500 registered users, 100 concurrent SSE, 50 chat msg/s burst,
40 concurrent meeting joins (signaling).

This template is filled from lab runs of `scripts/load/*`. It is **not** a
10k town hall claim.

## Lab environment

| Field | Value |
|-------|--------|
| Date (UTC) | |
| Pale version / commit | |
| Hardware | |
| OS | |
| Postgres | |
| LiveKit | none / version |
| Replicas (API / SIP) | 1 API + 1 registrar (typical) |

## Procedure

```bash
export PALE_BASE_URL=...
export PALE_ADMIN_TOKEN=...
export PALE_ROOM_ID=...

./scripts/smoke-test.sh
./scripts/compliance-smoke.sh
./scripts/load/sse-fanout.sh 100 30
./scripts/load/chat-burst.sh 200
./scripts/load/meeting-join-storm.sh 40 10
```

Capture reverse-proxy p95 and error rate if available.

## Results

| Metric | Target | Measured | Pass? |
|--------|--------|----------|-------|
| Smoke | 100% pass | | |
| Compliance smoke | 100% pass | | |
| SSE holders concurrent | 100 | | |
| Chat burst errors | 0 | | |
| Meeting join errors | 0 | | |
| p95 login latency | document | | |
| p95 message send | document | | |
| CPU peak (pale-server) | document | | |
| RAM peak | document | | |
| Postgres size after run | document | | |

## Conclusion

- [ ] RM-500 control plane: **pass** / **fail**  
- Media capacity (LiveKit): separate appendix only if SFU was under test  

Signer: _______________  Date: _______________
