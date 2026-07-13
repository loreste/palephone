# Implementation backlog: closest practical Teams Enterprise alternative

This is the **engineering backlog** for getting Pale as close as possible to a
Microsoft Teams Enterprise–class self-hosted product. It is not a claim of full
parity with Microsoft’s cloud + M365 ecosystem.

**Related:** [TEAMS_PARITY.md](TEAMS_PARITY.md) · [MILESTONES.md](../MILESTONES.md) ·
[NEXT_STEPS.md](../NEXT_STEPS.md)

**Ceiling:** regulated mid-market self-hosted Teams+PBX (~70–85% of daily
enterprise use). Explicit non-goals: Operator Connect marketplace, 10k town hall,
full Graph/SharePoint/Power Platform, multi-active multi-region SIP.

Status: `todo` · `doing` · `done` · `deferred` · `partial`

Last updated: 2026-07-13.

---

## Phase 0 — Enterprise-ready foundation

| ID | Feature | Side | Status | Notes |
|----|---------|------|--------|-------|
| 0.1 | Shared session store (Postgres) | Server | **done** | `admin_sessions` + cache/miss |
| 0.2 | Session revoke fan-out | Server | **done** | Revoke/refresh/principal wipe hit PG |
| 0.3 | SSO end-user login in SetupWizard | Client | **done** | Public providers + browser OIDC + callback |
| 0.4 | OIDC claim → role mapping | Server | **done** | `groups_claim`, `role_mappings`, migration 067 |
| 0.5 | Compliance smoke suite | Server | **done** | `scripts/compliance-smoke.sh` |
| 0.6 | Desktop enterprise checklist | Client | **done** | `docs/deploy/desktop-fleet.md` |
| 0.7 | Android physical + background calling | Client | **partial** | Docs + service present; FCM/physical open |
| 0.8 | iOS gate (CallKit/APNs or explicit no) | Client | **done** | Gate B — `docs/deploy/ios-gate.md` |
| 0.9 | RM-500 capacity report | Ops | **partial** | Template `docs/evidence/RM-500-TEMPLATE.md` |

---

## Phase 1 — Daily-driver parity

| ID | Feature | Side | Status | Notes |
|----|---------|------|--------|-------|
| 1.1 | LiveKit production path polish | Both | partial | Fail-closed + client room exist |
| 1.2 | Meeting lobby UX complete | Both | partial | APIs + MeetingPanel |
| 1.3 | Breakout rooms (real) | Both | partial | Surface; harden UX |
| 1.4 | Meeting chat / reactions / raise hand | Both | partial | |
| 1.5 | Screen share + spotlight rules | Both | partial | Desktop SIP share done |
| 1.6 | Gallery / active-speaker layout | Client | todo | LiveKit layout |
| 1.7 | Call park / transfer / multi-line polish | Client | partial | |
| 1.8 | Voicemail + STT hook | Both | partial | VM exists; STT provider |
| 1.9 | Presence reliability cross-device | Both | partial | |
| 1.10 | Teams/channels hierarchy | Both | partial | Rooms/teams models |
| 1.11 | Mentions / threads completeness | Both | partial | |
| 1.12 | Global search | Both | **done** | `/v1/search` unified |
| 1.13 | Guest access controlled | Both | partial | Guest APIs |
| 1.14 | Offline message queue solid | Client | partial | Banner + queue |
| 1.15 | Notification policy + push all platforms | Both | partial | VAPID web; FCM/APNs open |
| 1.16 | S3/MinIO multi-node default | Server | partial | Env wired |
| 1.17 | File preview | Client | todo | |
| 1.18 | Share links + permissions | Both | partial | Schema 068; API next |
| 1.19 | Co-authoring provider path | Both | todo | Collabora |

---

## Phase 2 — Compliance (Purview-lite)

| ID | Feature | Side | Status | Notes |
|----|---------|------|--------|-------|
| 2.1 | eDiscovery v1 | Server | partial | Cases APIs; export package deepen |
| 2.2 | Legal hold enforcement | Server | partial | File hold flags |
| 2.3 | Retention proven | Server | partial | Worker + smoke list |
| 2.4 | DLP matrix complete | Server | partial | Chat/file + compliance smoke |
| 2.5 | Information barriers v2 | Server | partial | Chat/call paths |
| 2.6 | Sensitivity labels enforce | Both | todo | |
| 2.7 | Audit export + integrity | Server | partial | Keyed integrity |
| 2.8 | ATP depth | Server | partial | ClamAV |
| 2.9 | Conditional access v2 (IP) | Server | partial | require_mfa strong |
| 2.10 | SCIM Users + Groups | Server | partial | Users live; Groups schema 069 |
| 2.11 | RBAC packages enforce | Server | partial | |

---

## Phase 3 — Mobile fleet

| ID | Feature | Side | Status | Notes |
|----|---------|------|--------|-------|
| 3.1 | Android FCM push | Both | todo | See android-fleet.md |
| 3.2 | Android background ring/answer | Client | partial | Foreground service |
| 3.3 | Android physical video QA | Client | todo | Issue #1 |
| 3.4 | iOS CallKit | Client | deferred | Gate B |
| 3.5 | iOS PushKit / APNs | Both | deferred | Gate B |
| 3.6 | iOS TestFlight path | Client | deferred | Gate B |
| 3.7 | MDM managed app config | Both | todo | |
| 3.8 | Mobile SSO | Client | partial | Same wizard SSO |

---

## Phase 4 — Phone system / PSTN / E911

| ID | Feature | Side | Status |
|----|---------|------|--------|
| 4.1–4.8 | Trunk, DID, E911 provider, wallboard, recording policy, delegates, hotdesk | Both | partial / lab |

---

## Phase 5 — Meetings scale (not 10k)

| ID | Feature | Side | Status |
|----|---------|------|--------|
| 5.1–5.6 | Webinar, egress recording, RTMP, captions, polls, RM-Meet-100 | Both | partial / todo |

---

## Phase 6 — AI (provider-backed)

| ID | Feature | Side | Status |
|----|---------|------|--------|
| 6.1–6.5 | Summaries, live STT, assist, speech IVR, validation probes | Both | partial / configured-provider |

---

## Phase 7 — Platform / extensibility

| ID | Feature | Side | Status |
|----|---------|------|--------|
| 7.1–7.6 | Bots, webhooks, OAuth apps, slash commands, calendar free/busy, federation | Both | partial / readiness |

---

## Phase 8 — Hardening & polish

| ID | Feature | Side | Status |
|----|---------|------|--------|
| 8.1–8.7 | SPKI pin, multi-window, browser strategy, VB, whiteboard, a11y, dual-control | Both | partial / todo |

---

## Explicit non-goals

- Operator Connect marketplace  
- Full Microsoft Graph / SharePoint / Power Platform  
- 10,000-viewer stadium town hall  
- Multi-active multi-region SIP registrar  
- Full CASB product  
- Teams Rooms / Surface Hub OS  

---

## Definition of “as close as possible”

1. Phase 0 complete (remaining: Android FCM/physical, filled RM-500 report)  
2. Phase 1 daily paths solid with LiveKit + S3  
3. Phase 2 eDiscovery/hold/retention/SCIM groups usable  
4. Phase 3: Android reliable; iOS Gate A **or** documented Gate B  
5. Phase 4 when phone deals require it  
6. Procurement matrix rows flipped with proof  

Update this file when items land.
