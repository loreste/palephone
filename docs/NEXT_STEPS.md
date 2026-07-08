# Next Steps

This document is the public roadmap for closing the remaining gap between Pale
and a Microsoft Teams Enterprise-style deployment. It is intentionally honest:
some work is product code, some work is provider integration, and some work is
proof that the system holds up under real tenant load.

## What Has Landed

The current codebase already includes broad coverage across calling, PBX,
messaging, meetings, files, compliance, security, admin governance, devices,
apps, federation, AI provider contracts, and enterprise integration readiness.

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

### 1. Provider-Specific Adapters

Generic readiness records and HTTP/TCP probes are in place. The next step is
deep provider adapters that can prove each system is actually usable:

- ClamAV and YARA for malware scanning
- Whisper and Vosk for speech-to-text
- Piper, Coqui, or compatible engines for text-to-speech
- Ollama, vLLM, LocalAI, or compatible engines for LLM workflows
- Nextcloud, S3, and WebDAV for external storage
- LiveKit, SRS, Janus, or mediasoup for broadcast/media paths
- Collabora or LibreOffice services for document presentation/rendering
- OPA-style policy engines and CASB providers
- certified carrier, SBC, E911, and PSTN providers

The test for being done is not "a URL exists." The adapter should authenticate,
run a provider-native health check, and return evidence that an admin can trust.

### 2. End-to-End Enterprise Validation

The admin validation report exists, but it is still mostly a readiness report.
The next version should run a tenant-level validation pass that exercises the
actual workflows:

- chat and channels
- meetings and scheduled meetings
- calling, PBX, queues, voicemail, and recordings
- files, versioning, folders, and external storage
- retention, DLP, eDiscovery, ATP, CASB, and compliance review
- transcription, meeting assistant, LLM, STT, and TTS providers
- PSTN, E911, SIP gateways, and location routing
- town hall and broadcast paths

The output should be a single exportable certification-style report for admins.

### 3. Real-Time Scale Proof

Town hall configuration and broadcast readiness are modeled. Pale still needs
repeatable load proof for large deployments:

- 10,000-viewer town hall fanout
- meeting signaling under load
- SSE fanout under load
- media gateway and broadcast path capacity
- database and cache pressure during large events
- reconnect behavior during network churn

The target is a repeatable load test suite with capacity reports, not a manual
demo.

### 4. Client and Runtime Hardening

Desktop (macOS ARM + Intel, Windows, Linux) and Android paths are working.
Push notifications, VP8 video, screen sharing, and media permission checks
are implemented. Remaining work:

- iOS packaging
- background calling and meeting behavior on mobile
- pop-out and multi-window lifecycle polish
- media runtime certification across supported platforms

### 5. Security Hardening

Security headers (HSTS, X-Frame-Options, CSP, Referrer-Policy, Permissions-Policy)
and HMAC-signed audit log entries are in place. Remaining hardening:

- deeper OIDC and SSO validation (certificate pinning for provider calls)
- secret rotation workflows
- stricter review for high-risk admin actions
- backup, restore, and disaster recovery evidence
- vulnerability scanning for server images and release artifacts

### 6. Documentation and Operator Guides

The public docs should stay accurate as the product moves:

- keep README feature claims tied to real code paths
- keep API.md aligned with the server route table
- keep ARCHITECTURE.md focused on the current system, not old design notes
- add deployment guides for the open-source foundation services Pale expects a
  tenant to install separately
- add provider setup guides for AI, speech, storage, ATP, CASB, PSTN, E911, and
  broadcast media

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
