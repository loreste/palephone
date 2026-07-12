# Pale Architecture

> Last updated: July 2026

Pale is a self-hosted communications workspace for calling, messaging,
meetings, files, PBX workflows, and governance. The desktop app is built with
Tauri, React, TypeScript, Rust, PJSIP, and Matrix. The server side adds the
tenant-controlled pieces: SIP account management, PBX routing, HTTP APIs,
events, compliance records, files, integrations, and operational checks.

The project is not trying to hide the hard parts of an enterprise
communications system. AI, speech, malware scanning, PSTN, emergency calling,
cloud storage, broadcast media, and policy engines need real providers behind
them. Pale exposes those dependencies as visible, configurable integration
contracts so administrators can see what is ready and what still needs to be
installed.

## System Shape

```
Pale desktop / mobile app
  React UI
  Tauri IPC
  pale-core SIP/media runtime
  pale-matrix Matrix client
        |
        | HTTP API, SSE, SIP, Matrix, media
        v
Pale server
  HTTP API and admin API
  SIP registrar/proxy and PBX state
  conference, meeting, call center, file, compliance, and governance models
  provider readiness, health, probe, validation, and deployment reports
        |
        v
Tenant infrastructure
  PostgreSQL, coturn, optional NATS
  Matrix/Synapse or another Matrix homeserver
  optional LLM/STT/TTS, ATP, CASB, storage, media, PSTN, and E911 providers
```

## Major Components

### React and Tauri Client

The user-facing app lives in `src/`. It provides calling, chat, meetings,
files, admin views, settings, and enterprise readiness screens. The React app
uses Tauri commands and events rather than giving the webview broad direct
access to local system APIs.

The client is intended for macOS, Windows, Linux, and Android. Android build
workflows exist. Browser-only deployment and fully packaged mobile app parity
still need more hardening around push notifications, background modes, device
permissions, and runtime certification.

### pale-core

`pale-core` wraps the local SIP and media runtime used by the Tauri app. It is
responsible for account registration, call lifecycle, call controls, audio
devices, local recordings, local call history, configuration, and OS keychain
storage.

PJSIP is used for the desktop SIP/media path because it provides a mature SIP
stack, RTP/RTCP, codec handling, ICE/STUN/TURN support, and platform audio
integration.

### pale-matrix

`pale-matrix` owns Matrix client lifecycle, room sync, encrypted chat, and file
transfer flows. It gives Pale a standards-based path for secure messaging while
keeping the product UI unified with calling, files, and meetings.

### pale-server

`pale-server` is the self-hosted control plane. It exposes the HTTP API, SSE
events, admin API, SIP/PBX records, file records, meeting records, compliance
records, provider integration registry, and Prometheus metrics.

The server uses PostgreSQL for persistent data when configured, with in-memory
state for fast runtime lookups (one active SIP registrar instance). Docker
Compose local stack: PostgreSQL, pale-server, coturn, and NATS. Production
compose (`docker-compose.prod.yml`) adds private networking, required TLS/TURN
hostnames, optional Caddy, and optional LiveKit.

The process default for `PALE_HTTP_ADDR` is `127.0.0.1:8080`. The Docker Compose
deployment maps that internal port to `localhost:8090` on the host, which is
why user-facing examples normally use `http://localhost:8090`.

## Server Responsibilities

### PBX and Calling

pale-server tracks SIP accounts, registrations, dialogs, calls, call detail
records, routing rules, extensions, ring groups, queues, IVR, business hours,
holidays, call park, voicemail, call groups, delegates, SIP gateways, location
routing, and emergency call plans.

Production and Docker deploys should set `PALE_SIP_BACKEND=udp-parser`, the
built-in REGISTER/PBX parser path over SIP TLS/TCP (UDP is off unless
explicitly enabled). The optional `pjsip` backend requires a
`native-pjsip` build and does not replace the registrar path on standard
server images. Deployments may also put OpenSIPS, Kamailio, or another SIP edge
in front of Pale and use pale-server for provisioning, records, routing data,
governance, and admin workflows.

Operator runbooks: `docs/deploy/PRODUCTION.md`, Linux/Windows guides under
`docs/deploy/`, and Kubernetes manifests under `deploy/k8s/`.

The server has models and APIs for PSTN and E911 readiness. Production PSTN and
emergency calling still require configured carrier/SBC/E911 providers. Pale
does not claim that those services are available just because the records exist.

### Meetings and Events

The server models conferences, scheduled meetings, webinar registration,
attendance, polls, Q&A, lobby, green room, whiteboard, annotations, captions,
transcripts, presentation sessions, media settings, streaming sessions, and
town hall configuration.

Large broadcast delivery, production RTMP/NDI output, PowerPoint-style live
presentation rendering, and AI-assisted summaries depend on external media,
rendering, and AI providers.

### Messaging and Collaboration

The API covers rooms, room messages, typing, reactions, read receipts, saved
messages, pins, favorites, teams, channels, tags, guests, federation records,
tabs, apps, connectors, bots, message extensions, wiki pages, task boards,
approvals, custom emoji, and automation records.

Matrix handles the encrypted messaging path. pale-server also stores governed
collaboration records used for search, eDiscovery, retention, and compliance.

### Files

Files can be uploaded, listed, downloaded, versioned, locked, and organized into
folders. External cloud storage is represented through provider readiness and
status APIs. Real production storage integrations still need provider-specific
adapters such as Nextcloud, S3, WebDAV, SharePoint, OneDrive, or Google Drive.

### Security, Compliance, and Governance

pale-server includes APIs and admin surfaces for audit events, retention,
eDiscovery, DLP, ATP quarantine records, security score, communication
compliance reviews, information barriers, sensitivity labels, policy packages,
custom roles, conditional access, SSO providers, MFA status, sessions,
certificate-auth configuration, encryption status, privileged access, data
residency, SCIM-style provisioning, and CSV import/export.

Some controls are complete application features. Others are readiness,
configuration, or evidence surfaces that expect external infrastructure. For
example, ATP records do not replace a malware scanner, and CASB readiness does
not replace a CASB provider.

### AI and Speech

The server exposes provider contracts for:

- LLM chat and assistant workflows
- STT transcription and speech IVR
- TTS synthesis

Provider APIs report readiness and refuse to fabricate results when no provider
is configured. Typical deployments would connect services such as Ollama,
vLLM, LocalAI, Whisper, Vosk, Piper, Coqui, or another compatible provider.

### Enterprise Integration Registry

The enterprise integration registry tracks external systems required for
Teams-style enterprise coverage. It exposes:

- inventory of configured integrations
- readiness report
- health report
- provider probe report
- tenant validation report
- deployment plan

Generic HTTP and TCP probes can confirm basic reachability. Provider-specific
adapters are still needed for deeper checks such as S3 buckets, RTMP servers,
carrier trunks, E911 validation, CASB policies, malware engines, and document
rendering services.

## API and Events

The HTTP API is implemented in `src-tauri/crates/pale-server/src/http.rs`.
Important API groups are documented in [API.md](API.md).

Real-time updates use `GET /v1/events` server-sent events. SSE is also bridged
to NATS when `NATS_URL` is configured, allowing other tenant systems to consume
Pale events without polling.

## Data Flow

1. A user signs in through the Pale app or an admin session.
2. The app uses the HTTP API for user, room, meeting, file, and admin state.
3. Calling uses the local SIP/media runtime and, when configured, pale-server's
   SIP/PBX records and routing data.
4. Matrix handles encrypted chat sync where Matrix is configured.
5. pale-server publishes events over SSE and optionally NATS.
6. Governance features write records that feed audit, search, retention,
   eDiscovery, DLP, compliance review, and exports.
7. Enterprise readiness views combine internal configuration with provider
   readiness, health, probes, and validation checks.

## Deployment

The standard development deployment is Docker Compose:

- `pale-server` built from this repository
- `postgres:16-alpine`
- `coturn/coturn`

The container listens internally on port `8080`; Compose exposes it as host
port `8090`. SIP UDP is mapped on `5060`. SIP TLS is only active when
`PALE_SIP_TLS=true` and certificate/key paths are configured. TURN is exposed
separately through coturn.

Production deployments should add TLS termination, durable PostgreSQL storage,
backup/restore, log shipping, monitoring, secret rotation, and provider-specific
services required by the tenant.

## Security Model

Pale's security posture is built around tenant control and explicit provider
boundaries:

- no hardcoded production secrets
- bearer-token API sessions with expiration and refresh flows
- admin login rate limiting
- SIP digest authentication for SIP accounts
- OS keychain storage for local credentials
- Matrix E2E encryption path for chat
- encrypted file metadata/content paths where implemented
- audit records for administrative actions
- DLP, retention, eDiscovery, labels, barriers, security score, and compliance
  review surfaces
- provider readiness checks for external ATP, CASB, PSTN, E911, storage, and AI
  systems

Security-critical integrations must still be validated in the deployment
environment. A configured record is not the same as a certified provider path.

## Testing and Validation

The current repository includes Rust tests for server logic and frontend build
checks. GitHub Actions cover the general CI path, pale-server CI, and Android
builds.

The enterprise validation report is an admin-facing runtime check. It is not a
full certification harness yet. Remaining validation work includes tenant-level
end-to-end runs that exercise chat, meetings, calling, files, DLP, ATP, CASB,
transcription, AI providers, E911/PSTN, town hall, and external integrations as
one report.

## Known Boundaries

Pale is broad, but public documentation should stay precise:

- Provider contracts do not mean every provider-specific adapter is finished.
- Town hall configuration does not prove 10,000-viewer fanout by itself.
- ATP/CASB readiness does not replace malware scanning or CASB products.
- PSTN/E911 records do not replace certified carrier and emergency providers.
- Android build support exists, but full mobile runtime parity still needs more
  packaging and operational hardening.
- Browser deployment and pop-out multi-window parity still need dedicated
  lifecycle, permission, and notification work.
