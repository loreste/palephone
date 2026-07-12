# PSTN lab with an SBC (SIP gateway)

Pale routes outbound numbers that look like phone numbers through configured
**SIP gateways** (longest prefix match). Media stays on the client/SBC path;
pale-server issues a SIP **302** toward `sip:{number}@{gateway}:{port}`.

This is enough for a **lab** against FreeSWITCH, Asterisk, Kamailio, OpenSIPS,
or a carrier sandbox SBC. It is **not** Operator Connect certification.

## Prerequisites

- Pale Server with `PALE_SIP_BACKEND=udp-parser` and SIP TLS if clients use TLS
- An SBC that accepts SIP from Pale and can dial the PSTN (or a second lab PBX)
- Admin token for the Pale API

## Configure a gateway

```bash
export BASE=https://pale.example.com
export TOKEN=...   # admin bearer

curl -sS -X POST "$BASE/v1/admin/sip-gateways" \
  -H "Authorization: Bearer $TOKEN" \
  -H "User-Agent: Pale/admin" \
  -H "Content-Type: application/json" \
  -d '{
    "name": "lab-sbc",
    "host": "sbc.lab.example.com",
    "port": 5061,
    "transport": "tls",
    "username": "pale-trunk",
    "password": "trunk-secret",
    "prefix": "+",
    "enabled": true
  }'
```

| Field | Notes |
|-------|--------|
| `prefix` | Longest match wins. `+` matches E.164; `""` is a catch-all |
| `transport` | Prefer `tls` in production |
| `username` / `password` | Stored for inventory/status; outbound is 302 redirect today |

Optional catch-all for national formats:

```json
{ "name": "lab-sbc-nat", "host": "sbc.lab.example.com", "port": 5060,
  "transport": "tcp", "prefix": "", "enabled": true }
```

## Probe connectivity

```bash
curl -sS -X POST "$BASE/v1/admin/sip-gateways/{id}/probe" \
  -H "Authorization: Bearer $TOKEN" \
  -H "User-Agent: Pale/admin"
```

Returns TCP reachability of `host:port` (not a full SIP OPTIONS handshake).
Use this to confirm firewall and SBC listen address before dial tests.

## Operator Connect readiness

```bash
curl -sS "$BASE/v1/admin/pstn/status" \
  -H "Authorization: Bearer $TOKEN" \
  -H "User-Agent: Pale/admin"
```

`routable` becomes true when:

- enterprise integration `pstn_sbc_operator_connect` is marked available (admin UI),
- at least one enabled gateway has an E.164-style prefix (`+...`),
- TLS gateway count and emergency route heuristics are satisfied as reported.

## Dial test

1. Register two Pale clients (or one client + lab phone).
2. From Pale, dial `+15551234567` (or a number matching your prefix).
3. Expect SIP 302 Contact toward the gateway; the client/SBC completes the call.
4. Confirm CDR on Pale and leg on the SBC.

## FreeSWITCH example (sketch)

On FreeSWITCH, accept INVITEs for `+.` and bridge to a carrier gateway profile.
Point Pale gateway `host`/`port` at the FreeSWITCH SIP profile that trusts Pale
as a peer (IP ACL or digest). Prefer TLS profiles for lab realism.

## Limitations (honest)

- Pale does **not** REGISTER as a trunk client to the SBC in this path.
- Gateway password is for admin status / future B2BUA work, not live digest out.
- E911 and Operator Connect need certified providers and location policy.
- Media is peer/SBC-side; ensure TURN and codec policy match the SBC.

See also [PRODUCTION.md](PRODUCTION.md) and Admin → Voice / SIP gateways.
