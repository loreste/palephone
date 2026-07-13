# Procurement Matrix: Pale vs Microsoft Teams Enterprise (vertical slice)

Use this with security questionnaires and RFP responses. Status meanings:

| Status | Meaning |
|--------|---------|
| **enforced** | Product blocks or requires the control in runtime (not only admin UI) |
| **configured-provider** | Works when tenant installs/configures a real provider |
| **readiness-only** | Records, APIs, or admin screens exist; do not sell as finished alone |
| **lab** | Documented lab path; not certified against a named commercial carrier |
| **out of scope** | Not a v1 claim for the regulated mid-market vertical |

Vertical scope and milestones: [../MILESTONES.md](../MILESTONES.md).  
Roadmap honesty: [../NEXT_STEPS.md](../NEXT_STEPS.md).

Last updated: 2026-07-12.

---

## Identity and access

| Capability | Pale status | Notes |
|------------|-------------|--------|
| Local accounts / SIP auth | enforced | SIP digest; user auth APIs |
| SSO (OIDC/SAML-style providers) | configured-provider | Admin SSO providers + `/v1/auth/sso/*`; needs real IdP |
| MFA (TOTP) | enforced | Enrollment UI; secrets encrypted; CA can require MFA |
| Conditional access `require_mfa` | enforced | Password and SSO login paths |
| Conditional access (block) | enforced / partial | Login can be blocked when CA policy requires it; treat other CA dimensions carefully |
| Session list / revoke | enforced | Device inventory in PG; revoke clears shared auth sessions |
| Multi-API-node session HA | enforced (with Postgres) | Bearer sessions in `admin_sessions`; see `docs/deploy/ha.md` |
| LDAP/AD directory | configured-provider | LDAP auth path |
| SCIM-style provisioning | readiness-only / partial | SCIM-style endpoints; validate against customer IdP |
| Custom roles / policy packages | readiness-only | Models and admin APIs present |
| Certificate auth | readiness-only | Config surface; full mTLS client story is deploy-specific |
| Dual-admin / break-glass | configured-provider | `PALE_REQUIRE_DUAL_ADMIN`, server token procedures |

## Calling and PBX

| Capability | Pale status | Notes |
|------------|-------------|--------|
| 1:1 / multi-line SIP voice | enforced | PJSIP client + pale-server registrar (udp-parser) |
| SIP TLS + SRTP | configured-provider | Production compose expects TLS certs |
| Video (VP8) + screen share | enforced | Desktop path |
| PBX: queues, IVR, ring groups, park, VM | enforced | Core PBX router |
| Call center wallboard / QA | readiness-only | Product surface; operational maturity varies |
| PSTN / SBC / Operator Connect | lab | Gateway inventory, probes, lab guides — needs carrier |
| E911 / emergency calling | lab + fail-closed | Blocks unsafe emergency INVITE without ready plan |
| CNAM | readiness-only | Provider configuration surface |
| Common area phones / room devices | readiness-only | Records and provisioning paths |

## Meetings and collaboration

| Capability | Pale status | Notes |
|------------|-------------|--------|
| Scheduled meetings / lobby / attendance | enforced / partial | Lifecycle APIs; media depends on LiveKit etc. |
| Multi-party SFU media | configured-provider | LiveKit; fail-closed when required |
| Webinar registration | readiness-only | Models/APIs |
| Town hall 10k viewers | out of scope | Config only; no capacity proof for v1 |
| Polls, Q&A, breakouts | readiness-only | Surfaces exist; validate per deploy |
| Whiteboards / Loop-like components | readiness-only | Not a vertical win theme |
| Wiki, tasks, approvals | readiness-only | Collaboration extras |
| Guests / federation | readiness-only | Records; external federation needs setup |

## Chat and files

| Capability | Pale status | Notes |
|------------|-------------|--------|
| 1:1 and group chat | enforced | Server rooms + Matrix path for E2E |
| Reactions, receipts, threads, mentions | enforced / partial | Threads persist; UX completeness varies |
| File upload / download / versioning | enforced | Local or S3/MinIO |
| External storage (SharePoint-class) | configured-provider | S3/MinIO path; SharePoint adapter not first-class |
| Co-authoring / Office live | readiness-only | Needs Collabora/LibreOffice-class provider |
| Push notifications | configured-provider | VAPID Web Push |

## Security and compliance

| Capability | Pale status | Notes |
|------------|-------------|--------|
| Audit log (admin actions) | enforced | Keyed SHA-256 integrity hash on entries (not true HMAC-SHA256; uses server token material) |
| DLP on chat send | enforced | Blocks send when policy hits |
| DLP on file upload | enforced | Upload blocking path |
| DLP export / violations | enforced | CSV export |
| eDiscovery cases / export | readiness-only / partial | Workflow surface; prove on tenant data |
| Retention policies | configured-provider | Worker via `PALE_RETENTION_ENFORCEMENT_*` |
| Information barriers | enforced / partial | Chat send path checks barriers between room members; broader org-wide IB maturity still limited |
| Sensitivity labels | readiness-only | Models present |
| ATP / malware scan | configured-provider | ClamAV probe + `PALE_ATP_REQUIRED` fail-closed |
| CASB | readiness-only | Integration registry — not a CASB product |
| Security score | readiness-only | Dashboard / recommendations surface |
| Data residency regions | readiness-only | Records; physical residency is deploy topology |
| Client-only API gate | enforced | Non-Pale clients rejected |
| Security headers / CSP | enforced | HTTP responses |
| Encryption at rest (app-level) | configured-provider | Storage key + encrypted paths; full disk is operator |

## AI and speech

| Capability | Pale status | Notes |
|------------|-------------|--------|
| LLM / STT / TTS dispatch | configured-provider | No fabricated output without provider |
| Meeting AI summaries | readiness-only | Structures only until provider wired |
| Speech IVR | configured-provider | Requires STT provider |

## Clients and operations

| Capability | Pale status | Notes |
|------------|-------------|--------|
| Desktop macOS / Windows / Linux | enforced | Shipping builds |
| Android | lab / partial | Signed sideload APK + full SIP video path (camera, overlays, answer/outbound video); emulator API 34 validated. Background calling + physical two-party confirmation still open. Download: https://drcpbx.com/downloads/Pale.apk |
| iOS | out of scope (until M3 gate A) | Packaging docs only; not certified fleet |
| Browser-only full client | readiness-only | Hardening incomplete vs desktop |
| Multi-window lifecycle | readiness-only | Polish open |
| Single-node production | configured-provider | PRODUCTION runbook |
| HA (API scale-out + shared sessions) | partial | Shared auth sessions with Postgres; SIP still single registrar; SSE sticky |
| Multi-active SIP registrar | out of scope | One registrar or external SIP edge |
| Backup / restore | configured-provider | `scripts/backup.sh`, restore drill |
| Metrics | available | Prometheus `/metrics` (keep private) |
| Enterprise validation report | available | Live checks + CSV export; does not replace tenant certification |
| Load / capacity proof (RM-500) | lab | Scripts exist; published report is M4 |
| 10k town hall proof | out of scope | Explicit non-goal for v1 |

---

## Suggested RFP language (safe)

**Yes, with providers:**

> Pale provides a self-hosted communications stack for SIP calling/PBX, messaging,
> meetings, and compliance controls (including DLP enforcement, MFA, SSO
> integration, audit logging, and malware scanning via ClamAV). Critical
> dependencies such as identity providers, media SFU, object storage, and PSTN
> carriers are explicit tenant-configured systems. Enterprise validation and
> readiness reports surface configuration gaps rather than hiding them.

**No (unless stretch closed):**

> Pale is not a certified global Microsoft Teams cloud substitute, does not
> claim Operator Connect marketplace coverage, does not claim 10,000-viewer
> town hall capacity by default, and does not currently certify an iOS App Store
> managed fleet.

---

## Mapping to milestones

| Matrix gap | Milestone that closes it |
|------------|--------------------------|
| Compose CI + evidence pack + SSO/DLP labs | M1 (landed) |
| Shared session store / multi-API auth | M2 (0.1/0.2 landed with Postgres) |
| Android video sideload path | M3.0 (landed; physical E2E open) |
| Client platform clarity / iOS gate / Android background | M3.1–3.3 |
| Published RM-500 capacity numbers | M4 |
| Real carrier PSTN/E911 | Stretch (deal-driven) |
