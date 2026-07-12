# Next Steps

This document is the public roadmap for closing the remaining gap between Pale
and a Microsoft Teams Enterprise-style deployment. It is intentionally honest:
some work is product code, some work is provider integration, and some work is
proof that the system holds up under real tenant load.

## What Has Landed

The current codebase already includes broad coverage across calling, PBX,
messaging, meetings, files, compliance, security, admin governance, devices,
apps, federation, AI provider contracts, and enterprise integration readiness.

Recent enforcement work (not just readiness records):

- DLP policies block chat message send (not only file upload)
- Conditional access `require_mfa` is enforced on password and SSO login
- LiveKit join fails closed when SFU is configured (or `PALE_LIVEKIT_REQUIRED`)
- Message threads persist to Postgres with `thread_id`
- TOTP secrets stored encrypted; MFA disable blocked when CA requires MFA
- ClamAV path with fail-closed `PALE_ATP_REQUIRED`; MinIO/S3 compose profile
- Client MFA enrollment/login UI (setup wizard + server settings)
- Smoke/load scripts under `scripts/`; PSTN lab guide + gateway TCP probe API
- E911 fail-closed on SIP INVITE for emergency numbers without a ready plan
- Enterprise validation report includes live workflow checks (DLP, MFA, LiveKit, ATP, storage, PSTN)
- `/ready` probe endpoint and Content-Security-Policy on API responses
- Admin SIP gateway Probe button
- Native ClamAV zPING probe (`GET /v1/admin/atp/clamav/probe`) in validation
- Load scripts: SSE fanout, meeting join storm (`scripts/load/`)
- HA topology guide, E911 lab guide, secrets rotation, restore drill
- iOS packaging path (`IOS_SETUP.md`, `ios.yml` preview workflow)
- Trivy image/fs scan workflow (`.github/workflows/image-scan.yml`)

Recent enterprise readiness work added:

- provider inventory for external systems required by enterprise deployments
- readiness, health, deployment plan, provider probe, and validation reports
- admin UI for enterprise validation and provider probes
- LLM, STT, and TTS provider APIs
- security score and compliance workflow surfaces
- DLP, eDiscovery, retention, labels, barriers, data residency, and ATP
  quarantine records

Recent client and security work completed:

- VP8 video codec (libvpx) enabled for video calls on all desktop platforms
- screen sharing via PJSIP video device switching
- video stream detection and native window rendering
- push notifications (VAPID Web Push) for incoming calls and chat mentions
- client-only gate: server rejects non-Pale HTTP and SIP requests
- security headers (HSTS, X-Frame-Options, X-Content-Type-Options,
  Referrer-Policy, Permissions-Policy) on all HTTP responses
- HMAC-SHA256 integrity signing on audit log entries
- media permission checks before answering incoming calls
- builds for macOS (ARM + Intel), Windows, Linux, and Android
- emoji picker in the chat compose bar

Those features make the gaps visible and manageable. They do not remove the
need for real external systems where the feature depends on one.

## Remaining Work

Much of the earlier operator packaging and enforcement work has landed (see
“Recent enforcement work” above). What remains is mostly **provider depth**,
**scale evidence**, and **mobile certification**.

### 1. Provider-Specific Adapters

ClamAV now has a native zPING probe and fail-closed upload mode. S3/MinIO is
wired via env. Still deeper adapters needed for:

- YARA rulesets alongside ClamAV
- Whisper/Vosk, Piper/Coqui, Ollama/vLLM provider-native health
- Collabora/LibreOffice presentation rendering
- OPA-style policy engines and CASB providers
- certified carrier, SBC, E911 providers (beyond lab docs)

### 2. End-to-End Enterprise Validation

Validation now includes live workflow checks (DLP, MFA policy, registrar,
LiveKit, ClamAV ping, storage, PSTN). Still useful:

- exportable certification PDF/CSV package for auditors
- automated E2E job that runs `scripts/smoke-test.sh` in CI against compose

### 3. Real-Time Scale Proof

Load scripts exist under `scripts/load/` (SSE fanout, meeting join storm, chat
burst). Still needed for large-event claims:

- 10,000-viewer town hall fanout with LiveKit capacity reports
- published p95 latency numbers under DB pressure

### 4. Client and Runtime Hardening

- iOS path documented (`IOS_SETUP.md`); signed App Store / CallKit / APNs still open
- background calling polish on mobile
- multi-window lifecycle polish

### 5. Security Hardening

CSP, `/ready`, Trivy workflow, secret rotation docs, restore drill script are
in place. Still open:

- OIDC certificate pinning for provider calls
- stricter multi-approver workflows for high-risk admin actions
- shared session store for multi-API-node HA

### 6. Documentation and Operator Guides

See `docs/deploy/` (PRODUCTION, HA, storage-atp, pstn-lab, e911-lab,
secrets-rotation) and `scripts/load/README.md`.

## Coverage Snapshot

This table is the safest way to read the current state. "Product surface" means
there are code paths, records, APIs, or admin screens in the repository.
"Needs proof" means the feature should not be sold as production-ready until it
has provider-specific validation, load results, or deployment evidence.

| Area | Product surface present | Needs proof or deeper work |
|------|-------------------------|----------------------------|
| Calling and PBX | SIP accounts, registrations, routing, queues, IVR, voicemail, recordings, call groups, delegates, SIP gateways, location routing, emergency call plans | Certified PSTN/E911 provider adapters, deeper SIP transport certification, and real carrier test evidence |
| Meetings and webinars | conferences, scheduled meetings, lobby, polls, Q&A, attendance, webinar registration, captions, presentation records, town hall configuration | Media provider adapters, 10,000-viewer load proof, PowerPoint-style rendering integration, and production streaming validation |
| Chat and collaboration | rooms, messages, reactions, read receipts, teams, channels, tags, guests, federation records, tabs, apps, connectors, bots, wiki, tasks, approvals | Tenant-level workflow validation and provider setup guides for any external collaboration services |
| Files | upload, download, versioning, folders, locks, governance metadata, storage readiness | Real storage adapters and co-authoring/rendering providers |
| Security and compliance | retention, eDiscovery, DLP, ATP quarantine records, security score, compliance reviews, labels, barriers, data residency, SSO records, MFA/session APIs, security headers, HMAC-signed audit entries | Malware scanner adapters, CASB adapters, secret rotation, and stricter admin action review |
| AI and speech | LLM, STT, and TTS provider APIs, transcription jobs, speech IVR contracts, meeting assistant report structures | Real provider execution, provider-native health checks, and tenant-level validation reports |
| Devices and rooms | room/device records, common area phone records, scheduling panels, hot desking, provisioning paths | Device certification, installer/runtime testing, and operational guides |
| Platform integration | OAuth/API clients, bots, app catalog, message extensions, automations, federation, calendar/contact sync records | Production connectors, external provider tests, and operator runbooks |
| Clients | desktop app (macOS ARM+Intel, Windows, Linux), Android build, admin UI, enterprise readiness UI, push notifications, VP8 video, screen sharing, media permissions, client-only gate | iOS packaging, background modes, and multi-window lifecycle polish |
