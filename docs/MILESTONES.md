# Milestone Plan: Regulated Mid-Market Vertical

> Goal: close the **deal-winning** gap with Microsoft Teams Enterprise for a
> specific buyer, not achieve global Teams parity.

**Buyer profile (ICP):** mid-market org (50–2,000 seats) in regulated or
sovereignty-sensitive industries (finance, healthcare ops, legal, public sector
subsidiaries, critical infrastructure). Needs **SIP/PBX ownership**, **chat +
meetings**, **SSO/MFA**, **DLP/retention/eDiscovery**, and **self-hosted data**.

**Win condition:** a sales engineer can deploy a tenant from this repo’s
runbooks, pass an enterprise validation report with live checks green, and
defend the security posture in a procurement questionnaire without hand-waving
readiness records as finished product.

**Non-goals (explicitly later):**

- 10,000-viewer town halls / broadcast media at hyperscale
- Global certified Operator Connect / multi-carrier PSTN marketplace
- Full iOS App Store fleet replacement (document path only until certified)
- Active-active multi-region SIP registrar
- Loop/whiteboard/app-ecosystem depth matching M365 Graph

Related docs: [NEXT_STEPS.md](NEXT_STEPS.md), [deploy/PRODUCTION.md](deploy/PRODUCTION.md),
[deploy/ha.md](deploy/ha.md), [ARCHITECTURE.md](../ARCHITECTURE.md).

---

## How to read this plan

| Status language | Meaning |
|-----------------|---------|
| **Done in tree** | Code paths exist and enforce (not only UI/records) |
| **Milestone work** | Must ship before claiming this vertical “ready” |
| **Proof** | Scripts, CI, or lab evidence — not just features |

Each milestone has:

1. **Outcome** — what the buyer can trust after it lands  
2. **Deliverables** — concrete code, CI, or docs  
3. **Acceptance** — how we know it is finished  
4. **Out of scope** — what we refuse to claim yet  

Suggested sequencing is strict: M0 → M1 → M2 → M3 → M4. Do not start M3
(mobile) or M4 (scale theater) until M1–M2 make a single-site deploy certifiable.

---

## Baseline (already landed — do not re-scope)

Use this as the floor, not the finish line.

| Capability | Evidence in tree |
|------------|------------------|
| SIP registrar / PBX routing | `pale-server` udp-parser, production compose, `docs/deploy/PRODUCTION.md` |
| Chat + meetings surface | rooms, DLP on message send, LiveKit fail-closed when required |
| SSO + MFA + conditional access | SSO providers, TOTP, `require_mfa` on login |
| Compliance surface | DLP, eDiscovery, retention, labels, barriers, keyed audit integrity hashes |
| ATP path | ClamAV probe + `PALE_ATP_REQUIRED` fail-closed |
| Storage | MinIO/S3 via `PALE_S3_*` |
| E911 / PSTN honesty | fail-closed emergency INVITE; lab guides for PSTN/E911 |
| Enterprise validation | live checks + `/v1/admin/enterprise-integrations/validation.csv` |
| Operator packaging | secrets, restore drill, HA topology doc, load scripts |

Remaining work is **depth, proof, and packaging for procurement**, not greenfield
product surface.

---

## Milestone 0 — Vertical freeze and go-to-market honesty (1 week)

### Outcome

Everyone (engineering, SE, sales) sells the same slice: **single-site self-hosted
Teams+PBX for regulated mid-market**. No silent expansion into town-hall-scale
or multi-active SIP.

### Deliverables

- [x] This document (`docs/MILESTONES.md`)
- [x] **One-pager** in README “Who Pale is for” section linking here and to
      PRODUCTION / NEXT_STEPS
- [x] **Procurement matrix** (`docs/procurement/TEAMS_PARITY.md`): feature →
      status (`enforced` / `configured-provider` / `readiness-only` / `out of scope` / `lab`)
- [ ] Tag open admin screens that are readiness-only in the enterprise
      validation UI with the same four statuses (no new backend required if
      status already exists in validation report)

### Acceptance

- A non-engineer can answer “are we a Teams replacement?” with:  
  **“For single-site chat, meetings, calling/PBX, and compliance — yes, with
  providers. For global Microsoft cloud scale and M365 app ecosystem — no.”**
- Procurement matrix covers at least: SSO, MFA, DLP, eDiscovery, retention,
  ATP, SIP TLS, E911, PSTN, LiveKit, S3, HA, iOS, town hall.

### Out of scope

Rewriting the entire product marketing site.

---

## Milestone 1 — Certifiable single-site deploy (2–3 weeks)

**Theme:** a regulated buyer’s security team can install Pale and get a green
validation pack without a Pale engineer on a call.

### Outcome

“Day-1 production” for ≤500 concurrent users, one registrar, Postgres + coturn,
optional LiveKit/ClamAV/MinIO, SSO, MFA, DLP, ATP fail-closed.

**Status: complete in tree (2026-07-12).** CI compose job must stay green on
`main`; re-run smoke locally with `PALE_ADMIN_TOKEN` after upgrades.

### Deliverables

| # | Work item | Touch points | Status |
|---|-----------|--------------|--------|
| 1.1 | **Compose CI smoke** — run `scripts/smoke-test.sh` against compose (+ binary path in pale-server CI) | `.github/workflows/compose-smoke.yml`, `docker-compose.ci.yml`, `pale-server.yml` | landed |
| 1.2 | **Regulated profile env template** | `docs/deploy/regulated-midmarket.env.example` | landed |
| 1.3 | **Validation pack export** | `scripts/export-evidence-pack.sh` | landed |
| 1.4 | **SSO golden path runbook** | `docs/deploy/sso-oidc.md` | landed |
| 1.5 | **DLP golden path runbook** | `docs/deploy/dlp-lab.md` | landed |
| 1.6 | **Fix gaps found by smoke** — admin principal room membership; smoke DLP JSON; conference create body | `pale-server` `create_room`, `scripts/smoke-test.sh` | landed |

### Acceptance

- CI on `main` runs compose (or binary + docker deps) smoke: health, ready,
  login, room message, meeting join (or skip with documented flag), validation
  CSV non-empty.
- On a clean machine, following PRODUCTION + regulated env + SSO/DLP labs yields
  enterprise validation with critical items green or explicitly “provider not
  configured” (never silent green).
- Evidence pack script produces artifacts an auditor can attach without SE help.

### Out of scope

Multi-node HA session store (M2), iOS signing (M3), 10k town hall (deferred).

---

## Milestone 2 — Identity, session HA, and compliance depth (3–4 weeks)

**Theme:** the vertical’s security questionnaire items that currently fail on
“how do you scale API?” and “how do you prove policy enforcement?”

### Outcome

Bearer sessions survive multi-API-node (sticky or shared store); MFA/SSO/DLP/ATP
are demos you can record; admin dual-control and retention are operator-proven.

### Deliverables

| # | Work item | Touch points |
|---|-----------|--------------|
| 2.1 | **Shared session store** — Postgres-backed sessions so API replicas share auth state | `admin_sessions` + `put_auth_session` / PG miss path | **landed** (when `PALE_DATABASE_URL` set) |
| 2.2 | **Session revocation propagation** — revoke-all and per-session delete visible on all API nodes | delete by token / token_hash / principal in PG | **landed** |
| 2.3 | **OIDC production polish** — custom CA already (`PALE_OIDC_CA_BUNDLE`); add group/role claim mapping docs + one automated integration test with mock OIDC if feasible | SSO routes in `http.rs`, `docs/deploy/sso-oidc.md` |
| 2.4 | **DLP enforcement matrix** — document and test: chat send, file upload, and any meeting chat path; export violations; ensure policy packages can attach DLP | DLP APIs + smoke extension |
| 2.5 | **Retention enforcement proof** — enable interval worker in lab; document RPO for deleted content; smoke asserts job runs | `PALE_RETENTION_ENFORCEMENT_*` already in `main.rs` |
| 2.6 | **ATP depth** — keep ClamAV fail-closed; optional YARA ruleset path *or* clear “ClamAV-only certified” statement in procurement matrix | ATP probe + storage-atp lab |
| 2.7 | **Privileged access / dual-admin** — `PALE_REQUIRE_DUAL_ADMIN` exercised in smoke when enabled; break-glass token procedure in secrets-rotation | existing knobs + docs |

### Acceptance

- `docs/deploy/ha.md` documents shared auth sessions via Postgres (landed).
- Load balancer can run ≥2 API nodes with shared store; sticky sessions
  optional for SSE only.
- Smoke (or new `scripts/compliance-smoke.sh`) proves: SSO login path (mock or
  lab), MFA required, DLP block, ClamAV block or fail-closed, retention tick.
- Procurement matrix already marks MFA/DLP as enforced and SSO/ATP as
  configured-provider; M2 must not claim those as new — only add **proof**
  (labs, CI, HA session store) and update the sessions HA row when shared
  store lands.

### Out of scope

Multi-active SIP registrar; CASB product replacement; full SPKI pinning (nice
follow-on, not vertical blocker if TLS edge + CA bundle exist).

---

## Milestone 3 — Client fleet for the vertical (3–5 weeks)

**Theme:** regulated buyers still care about desktop first; Android path exists;
iOS is the common RFP gap — treat it as certification, not greenfield app.

### Outcome

Desktop is the supported fleet; Android is “supported with caveats”; iOS is
either **TestFlight-ready** or **explicitly unsupported** with a date — never
ambiguous.

### Deliverables

| # | Work item | Touch points | Status |
|---|-----------|--------------|--------|
| 3.0 | **Android video call path** — signed APK, camera, local/remote surfaces, answer with video | `packaging/android/*`, `pale-core` android_*, `android.yml` | **landed** (emulator API 34; physical E2E open) |
| 3.1 | **Desktop enterprise checklist** — SSO login, MFA, push, SIP TLS, screen share, media permissions, client-only gate verified on Windows + macOS | `IOS_SETUP.md` pattern → `docs/deploy/desktop-fleet.md` | open |
| 3.2 | **Android background calling** — document real limitations; fix critical call-receive path if broken | Android build workflow, push | open (foreground video path is in) |
| 3.3 | **iOS decision gate** — either (A) signed TestFlight build with CallKit/APNs plan executed, or (B) procurement matrix says “iOS not certified; desktop+Android only” with roadmap date | `IOS_SETUP.md`, `ios.yml` | open |
| 3.4 | **MDM notes** — config keys, server URL, cert trust for Intune/Jamf-style rollout (doc only) | desktop-fleet.md | open |

### Acceptance

- Procurement matrix clients row is unambiguous per platform.
- Desktop checklist signed off on last two stable releases.
- No marketing claim of “mobile parity with Teams” until iOS gate A is met.

### Out of scope

Multi-window polish for power users (unless it blocks SSO/MFA); Teams device
ecosystem (Surface Hub, etc.).

---

## Milestone 4 — Capacity evidence for the vertical (2 weeks, parallelizable)

**Theme:** mid-market capacity claims, not Microsoft stadium events.

### Outcome

Published, repeatable numbers for **control plane** load that match the ICP
(not 10k town hall).

### Deliverables

| # | Work item | Touch points |
|---|-----------|--------------|
| 4.1 | **Capacity profile “RM-500”** — target: 500 registered users, 100 concurrent SSE, 50 chat msg/s burst, 40 concurrent meeting joins (signaling) | `scripts/load/*` |
| 4.2 | **Report template** — p95 latency, error rate, CPU/RAM, Postgres size; store under `docs/evidence/` (git-lfs or linked artifact, not huge binaries) | `docs/evidence/RM-500-TEMPLATE.md` |
| 4.3 | **One published lab run** on reference hardware (document hardware) | fill template once |
| 4.4 | **LiveKit appendix** — if SFU configured, separate media capacity note; do not mix signaling-only results with media claims | load README |

### Acceptance

- README / PRODUCTION can state: “Lab-validated for RM-500 profile; see
  docs/evidence/…” without inventing 10k figures.
- Town hall 10k remains out of scope and labeled as such in NEXT_STEPS.

### Out of scope

Broadcast CDN, NDI/RTMP production truck features.

---

## Optional stretch (after M2, only if a deal requires it)

| Item | When to pull in |
|------|-----------------|
| Certified PSTN trunk + E911 lab against a real carrier | Deal needs external dial tone / emergency |
| Collabora/Nextcloud co-authoring | Deal is file-collab heavy vs calling |
| Ollama/Whisper meeting summaries | Deal requires AI meeting notes on-prem |
| Full SPKI cert pinning in clients | High-security questionnaire demands it |
| OpenSIPS/Kamailio front for multi-site SIP | Second site / SBC already in customer network |

Do not start stretch items before M1 acceptance unless a signed deal pays for them.

---

## Definition of Done — “Vertical Ready v1”

Ship the label **Pale Regulated Mid-Market v1** only when all are true:

1. **M0** procurement matrix published and linked from README  
2. **M1** compose CI smoke green; evidence pack script works; SSO + DLP labs pass  
3. **M2** shared session store merged and documented; compliance smoke green  
4. **M3** client matrix unambiguous (desktop certified; mobile status explicit)  
5. **M4** RM-500 lab report published once  

At that point sales may claim:

> Self-hosted alternative to Microsoft Teams for single-site enterprises that
> need SIP/PBX, encrypted chat, meetings, SSO/MFA, and compliance controls —
> with tenant-owned infrastructure and documented provider boundaries.

They still must **not** claim:

> Full global Teams Enterprise / Operator Connect / 10k town hall / iOS fleet
> replacement (unless M3 gate A and stretch items are done).

---

## Suggested ownership split

| Work type | Owner | Examples |
|-----------|-------|----------|
| Product code | Engineering | session store, DLP matrix tests, CI smoke |
| Provider adapters | Engineering + customer | ClamAV, OIDC IdP, LiveKit, MinIO |
| Evidence / load | SE + Eng | RM-500 report, evidence pack |
| Runbooks | Eng + SE | SSO, DLP, regulated env |
| Scope discipline | Product | reject out-of-vertical feature creep |

---

## Tracking

Update this file’s checkboxes as work lands. When a milestone completes, add a
one-line entry under “What Has Landed” in [NEXT_STEPS.md](NEXT_STEPS.md) and
bump the coverage snapshot row statuses only when proof exists.

**Next action:** M1 landed. M2.1–2.2 shared auth sessions landed (Postgres
`admin_sessions`). Android **3.0 video path** landed. Continue M2.3–2.7
(OIDC polish, compliance smoke, retention/ATP proof) and Phase 0.3 SSO client
login — see [procurement/IMPLEMENTATION_BACKLOG.md](procurement/IMPLEMENTATION_BACKLOG.md).
Keep issue #1 open until physical-device sideload is confirmed.
