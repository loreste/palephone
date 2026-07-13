# Implementation backlog: closest practical Teams Enterprise alternative

This is the **engineering backlog** for getting Pale as close as possible to a
Microsoft Teams Enterprise–class self-hosted product. It is not a claim of full
parity with Microsoft’s cloud + M365 ecosystem.

**Related:** [TEAMS_PARITY.md](TEAMS_PARITY.md) · [MILESTONES.md](../MILESTONES.md) ·
[NEXT_STEPS.md](../NEXT_STEPS.md)

**Ceiling:** regulated mid-market self-hosted Teams+PBX (~70–85% of daily
enterprise use). Explicit non-goals: Operator Connect marketplace, 10k town hall,
full Graph/SharePoint/Power Platform, multi-active multi-region SIP.

Status: `todo` · `doing` · `done` · `deferred`

---

## Phase 0 — Enterprise-ready foundation

| ID | Feature | Side | Status | Notes |
|----|---------|------|--------|-------|
| 0.1 | Shared session store (Postgres) | Server | **done** | Bearer sessions in `admin_sessions`; cache + PG miss |
| 0.2 | Session revoke fan-out | Server | **done** | Revoke/refresh/principal wipe hit PG |
| 0.3 | SSO end-user login in SetupWizard | Client | todo | OIDC redirect UX |
| 0.4 | OIDC claim → role mapping | Server | todo | Groups/roles from IdP |
| 0.5 | Compliance smoke suite | Server | todo | MFA, DLP, retention, dual-admin, ATP |
| 0.6 | Desktop enterprise checklist | Client | todo | Win + mac sign-off doc |
| 0.7 | Android physical + background calling | Client | todo | FCM, OEM polish |
| 0.8 | iOS gate (CallKit/APNs or explicit no) | Client | todo | Decision + path |
| 0.9 | RM-500 capacity report | Ops | todo | Published lab numbers |

---

## Phase 1 — Daily-driver parity

| ID | Feature | Side | Status |
|----|---------|------|--------|
| 1.1 | LiveKit production path polish | Both | todo |
| 1.2 | Meeting lobby UX complete | Both | todo |
| 1.3 | Breakout rooms (real) | Both | todo |
| 1.4 | Meeting chat / reactions / raise hand | Both | todo |
| 1.5 | Screen share + spotlight rules | Both | todo |
| 1.6 | Gallery / active-speaker layout | Client | todo |
| 1.7 | Call park / transfer / multi-line polish | Client | todo |
| 1.8 | Voicemail + STT hook | Both | todo |
| 1.9 | Presence reliability cross-device | Both | todo |
| 1.10 | Teams/channels hierarchy | Both | todo |
| 1.11 | Mentions / threads completeness | Both | todo |
| 1.12 | Global search | Both | todo |
| 1.13 | Guest access controlled | Both | todo |
| 1.14 | Offline message queue solid | Client | todo |
| 1.15 | Notification policy + push all platforms | Both | todo |
| 1.16 | S3/MinIO multi-node default | Server | todo |
| 1.17 | File preview | Client | todo |
| 1.18 | Share links + permissions | Both | todo |
| 1.19 | Co-authoring provider path | Both | todo |

---

## Phase 2 — Compliance (Purview-lite)

| ID | Feature | Side | Status |
|----|---------|------|--------|
| 2.1 | eDiscovery v1 (case, search, export package) | Server | todo |
| 2.2 | Legal hold enforcement | Server | todo |
| 2.3 | Retention proven for chat/files/meetings | Server | todo |
| 2.4 | DLP matrix complete | Server | todo |
| 2.5 | Information barriers v2 | Server | todo |
| 2.6 | Sensitivity labels enforce | Both | todo |
| 2.7 | Audit export + integrity | Server | todo |
| 2.8 | ATP depth (ClamAV + optional YARA) | Server | todo |
| 2.9 | Conditional access v2 (IP / locations) | Server | todo |
| 2.10 | SCIM Users + Groups | Server | todo |
| 2.11 | RBAC packages enforce on all admin APIs | Server | todo |

---

## Phase 3 — Mobile fleet

| ID | Feature | Side | Status |
|----|---------|------|--------|
| 3.1 | Android FCM push | Both | todo |
| 3.2 | Android background ring/answer | Client | todo |
| 3.3 | Android physical video QA | Client | todo |
| 3.4 | iOS CallKit | Client | todo |
| 3.5 | iOS PushKit / APNs VoIP | Both | todo |
| 3.6 | iOS TestFlight path | Client | todo |
| 3.7 | MDM managed app config | Both | todo |
| 3.8 | Mobile SSO | Client | todo |

---

## Phase 4 — Phone system / PSTN / E911

| ID | Feature | Side | Status |
|----|---------|------|--------|
| 4.1 | SBC/trunk production profile + OPTIONS health | Server | todo |
| 4.2 | Inbound DID routing hardened | Server | todo |
| 4.3 | Outbound dial plan + emergency set | Server | todo |
| 4.4 | E911 provider integration | Server | todo |
| 4.5 | Wallboard ops-grade | Both | todo |
| 4.6 | Call recording policy | Both | todo |
| 4.7 | Delegates finish | Both | todo |
| 4.8 | Common-area / hotdesk | Both | todo |

---

## Phase 5 — Meetings scale (not 10k)

| ID | Feature | Side | Status |
|----|---------|------|--------|
| 5.1 | Webinar mode real | Both | todo |
| 5.2 | LiveKit egress recording + playback | Both | todo |
| 5.3 | RTMP stream out | Server | todo |
| 5.4 | Live captions via STT | Both | todo |
| 5.5 | Polls / Q&A product depth | Both | todo |
| 5.6 | RM-Meet-100 capacity report | Ops | todo |

---

## Phase 6 — AI (provider-backed)

| ID | Feature | Side | Status |
|----|---------|------|--------|
| 6.1 | Meeting summary pipeline | Server | todo |
| 6.2 | Live transcription | Both | todo |
| 6.3 | Thread/meeting assist | Both | todo |
| 6.4 | Speech IVR | Server | todo |
| 6.5 | AI provider health in validation | Server | todo |

---

## Phase 7 — Platform / extensibility

| ID | Feature | Side | Status |
|----|---------|------|--------|
| 7.1 | Bots v1 | Server | todo |
| 7.2 | Incoming webhooks | Server | todo |
| 7.3 | OAuth apps scoped tokens | Server | todo |
| 7.4 | Slash commands / message extensions | Both | todo |
| 7.5 | Calendar free/busy connector | Server | todo |
| 7.6 | Federation v1 | Server | todo |

---

## Phase 8 — Hardening & polish

| ID | Feature | Side | Status |
|----|---------|------|--------|
| 8.1 | SPKI cert pinning | Client | todo |
| 8.2 | Multi-window lifecycle | Client | todo |
| 8.3 | Browser client strategy | Client | todo |
| 8.4 | Virtual background | Client | todo |
| 8.5 | Whiteboard v1 | Both | todo |
| 8.6 | Accessibility pass | Client | todo |
| 8.7 | Dual-control dangerous admin actions | Server | todo |

---

## Explicit non-goals (do not schedule unless funded)

- Operator Connect marketplace  
- Full Microsoft Graph / SharePoint / Power Platform  
- 10,000-viewer stadium town hall  
- Multi-active multi-region SIP registrar (use Kamailio/OpenSIPS edge)  
- Full CASB product (integrate OPA/Wazuh)  
- Teams Rooms / Surface Hub OS  

---

## Definition of “as close as possible”

1. Phase 0 complete  
2. Phase 1 daily paths solid with LiveKit + S3  
3. Phase 2 eDiscovery/hold/retention/SCIM groups usable  
4. Phase 3: Android reliable; iOS CallKit **or** explicit unsupported  
5. Phase 4 when phone deals require it  
6. Procurement matrix rows flipped with proof  

Update this file when items land.
