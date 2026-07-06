# Pale

Pale means “to speak” in Haitian Creole. That is the point of the project: people should be able to speak, meet, call, and work together without handing the whole conversation to someone else's cloud.

A self-hosted communications platform for voice, video, messaging, meetings, files, compliance workflows, and PBX. Pale is building toward Microsoft Teams-style enterprise coverage while keeping the infrastructure under the tenant's control.

## Why We Are Here

Communication is where work actually happens. It is the call with a customer, the message that unblocks a team, the meeting where a decision gets made, and the record an organization may need years later. That should not be trapped inside one vendor's cloud, pricing model, policy changes, or roadmap.

Pale exists for teams that want to own that layer. To speak freely here means you can run the system yourself, choose the providers you trust, keep control of your records, and decide how your communication data moves through your organization.

We are also not pretending the hard parts are magic. Speech, AI, malware scanning, storage, broadcast media, PSTN, E911, and policy enforcement all need real systems behind them. Pale makes those dependencies visible, lets administrators check whether they are ready, and keeps the core communications stack under your control.

## Why Pale?

- **Built to own the phone layer.** Pale includes SIP account records, PBX routing, emergency calling models, PSTN gateway readiness, and call center workflows. The built-in parser backend provides the current registrar/PBX path over SIP TLS/TCP; UDP is an explicit fallback, not the production default.
- **Your data stays yours.** Core calls, messages, files, SIP signaling, media, and chat run on your servers. Optional external providers are explicit integrations, not hidden dependencies.
- **Matrix-backed encrypted chat.** The Matrix client path supports Olm/Megolm encryption for conversations and encrypted file transfer.
- **One app, not five.** Calling, video, chat, files, meetings, compliance, voicemail, presence, and admin live in one interface.
- **Enterprise controls without lock-in.** DLP, eDiscovery, retention, security score, audit, SSO, conditional access, data residency, federation, guest access, app governance, and provider readiness are represented in the product.
- **Bring the right engines.** Pale exposes provider contracts for LLM, STT, TTS, malware scanning, storage, broadcast, media, CASB, PSTN, and E911 instead of pretending those systems can be safely faked in app code.
- **Desktop and Android path.** macOS, Windows, Linux, and Android builds are covered by the current project workflows.

## Features

### Calling & PBX

- Voice and video calls via SIP (PJSIP 2.14.1)
- UDP, TCP, and TLS transports with SRTP media encryption
- Hold, mute, blind/attended transfer, DTMF, multi-line
- **Call recording** — record any call with one click, WAV files stored locally and on server
- **Call park/retrieve** — park calls to numbered slots, pick up from any extension
- **Ring groups** — simultaneous, sequential, and random ring strategies with fallback
- **Call queues (ACD)** — round-robin, longest-idle, random, ring-all, and skills-based distribution
- **IVR / Auto-attendant** — multi-level menus with DTMF routing, custom greetings (text or audio file)
- **Call forwarding** — unconditional, busy, and no-answer forwarding per user
- **Follow-me / Find-me** — sequential dialing across multiple numbers with configurable timeouts
- **Do Not Disturb** — per-user DND with optional forward destination
- **Business hours routing** — timezone-aware schedules with after-hours destinations
- **Holiday calendar** — recurring and one-time holidays with custom routing
- **Voicemail** — per-user voicemail with greeting, playback, and listened/unread tracking
- **Speed dial** — server-synced speed dial buttons on the dialpad
- **CDR (Call Detail Records)** — every call logged with disposition, duration, queue info
- **Extensions** — map short codes to users, queues, ring groups, IVR, voicemail, or park slots
- **Inbound/outbound routing rules** — pattern-based source/destination matching with priority
- **PSTN and Operator Connect readiness** — SIP gateway inventory, TLS/auth checks, E.164 route coverage, and provider availability reporting
- **Emergency calling model** — emergency locations, user assignments, provider location IDs, and call plans that block unsafe routing until E911/PSTN providers are configured
- **CNAM and caller identity** — provider configuration and lookup readiness for enterprise caller ID workflows

### Call Center

- **Agent profiles** — roles (agent, supervisor, QA, admin), skills, max concurrent calls
- **Agent state management** — available, on-call, wrap-up, break, training, meeting, offline
- **Real-time wallboard** — live queue metrics, agent status, SLA tracking, calls waiting/active
- **QA scorecards** — score and review agent calls with comments and metrics
- **Supervisor tools** — monitor queue performance, manage agent states

### Messaging & Collaboration

- Matrix-backed encrypted chat via Olm for 1:1 and Megolm for groups
- 1:1 direct messages and group rooms
- Team spaces with channel rooms
- Scheduled meetings that create/join conference-backed calls
- Recurring meetings, meeting templates, webinar registration, and town hall configuration
- Typing indicators and read receipts
- Message edit and delete
- Message priority, saved messages, mentions, reactions, and Loop component records
- Encrypted file sharing (AES-256-CTR per-file keys)
- Drag-and-drop upload with any file type
- File versioning, folders, governance metadata, and external storage readiness tracking
- Full-text message search plus indexed discovery records for governed content
- Wiki pages, task boards, adaptive cards, channel tabs, message extensions, connectors, bots, app catalog, custom emojis, whiteboards, automations, signage displays, guest users, and federation records

### Meetings & Events

- Audio and video conference rooms
- Scheduled meetings with policy-aware join flows
- Meeting lifecycle controls, lobby policy, reactions, polls, attendance records, and recordings
- Recording policies, retention hooks, and admin review paths
- Call quality dashboard data and meeting quality signals
- Presentation session model with renderer/provider readiness
- Meeting media settings for gallery/together-mode style layout readiness, virtual background readiness, NDI/RTMP streaming readiness, and runtime capability reporting
- Town hall configuration with capacity enforcement and broadcast-provider readiness for large events
- Transcription job orchestration and AI meeting assistant report structures without fabricating AI output when no provider is configured

### Presence & Directory

- Real-time presence — online, busy, away, on-call, DND, offline
- Auto-presence from call state (on-call when in a call, online when idle)
- User directory with search and BLF indicators (see who's on a call)
- Click-to-call and click-to-chat from the directory

### AI, Speech & Automation

- LLM provider API for chat/assistant dispatch contracts
- STT provider API for transcription and speech IVR integration
- TTS provider API for generated speech workflows
- Provider status APIs for `llm`, `stt`, and `tts`, including supported protocols and readiness warnings
- Meeting assistant report structures, action items, speaker stats, and transcription workflow state
- Speech IVR routing that requires a configured provider and only matches configured phrases
- Automation rules and admin-visible integration health checks

Pale does not fake AI output locally. The server reports whether the tenant has configured a provider such as Ollama, vLLM, Whisper, Vosk, or another compatible service, and dispatches through that contract.

### Security, Compliance & Governance

- Retention policies with scheduled enforcement
- eDiscovery search, exports, cases, custodians, saved queries, and case-scoped exports
- DLP policies, scan previews, violations, CSV export, and file-upload blocking
- Malware/ATP quarantine model with admin review and provider readiness tracking for scanners such as ClamAV or YARA
- Security score dashboard with controls, recommendations, and posture summary
- Compliance review queue and admin remediation workflow
- Information barriers, sensitivity labels, governance records, and policy packages
- SSO provider management, conditional access policies, MFA/certificate-auth data structures, and privileged access review
- Encryption configuration, audit logs, data residency regions, and admin action traceability
- CASB/provider readiness tracking for external security controls

### Enterprise Integrations

- Admin-managed registry for external systems needed to approach Teams Enterprise-style parity
- Readiness report that refuses to mark the tenant ready while tracked critical dependencies are missing
- Health report that flags missing URLs, invalid protocols, missing provider details, and partial configurations
- Provider probe report for generic HTTP/WebDAV/gRPC and TCP reachability checks
- Enterprise validation report that combines readiness, provider probes, security posture, and deployment guidance
- Deployment plan that prioritizes open-source or self-hosted foundation services where possible
- Integrations tracked for AI, speech, transcription, noise suppression, virtual backgrounds, media layouts, streaming, presentation rendering, E911, PSTN/SBC, storage, ATP, CASB, mobile/web/runtime hardening, multi-window, push, device permissions, and town hall scale

The code models these integrations and readiness states. Real production use still requires installing and configuring the relevant providers, for example Matrix/Synapse, coturn, PostgreSQL, NATS, ClamAV/YARA, Whisper/Vosk, Ollama/vLLM, Nextcloud/S3/WebDAV, LiveKit/SRS, Collabora, OPA-style policy engines, and certified carrier/E911/PSTN providers.

### Admin Panel

The admin console covers PBX, collaboration, compliance, security, devices, apps, integrations, and enterprise readiness:

| Category | Tabs |
|----------|------|
| **System** | Overview, Audit Log |
| **Users** | Users (CRUD + role assignment), SIP Accounts, Directory (LDAP/AD) |
| **PBX** | Extensions, Routing, Ring Groups, Queues, IVR, Business Hours, Holidays, Paging, Media |
| **Call Center** | Agents, Wallboard, QA Scorecards, CDR |
| **Collaboration** | Conferences, Files, Active Calls, Meeting Templates, Recording Policies, Guests, Message Extensions, App Store |
| **Compliance** | Security Score, Retention, eDiscovery, DLP, Compliance Reviews, Information Barriers, Sensitivity Labels, Data Residency |
| **Identity & Security** | SSO, Encryption, Privileged Access, Conditional Access, Custom Roles, Policy Packages, API Clients |
| **Devices & Rooms** | Common Area Phones, Meeting Rooms, Devices, SIP Gateways, Scheduling Panels, Signage |
| **Enterprise** | Emergency Calling, Location Routing, Federation, Automations, Enterprise Integrations, Bandwidth Policies |

- **Role-based access** — admin tab only visible to admin users
- **LDAP/Active Directory integration** — auto-provision users from AD, map groups to roles
- **SCIM-style user provisioning** — `/v1/scim/v2/Users` endpoints for business lifecycle automation
- **Governance controls** — retention, eDiscovery, DLP, compliance review, and security posture reporting
- **Audit logging** — every admin action logged with principal, action, target, timestamp
- **Real-time refresh** — SSE events + 30-second polling for live data

### Desktop & Mobile

- Modern dark-first UI with Tailwind CSS
- System tray with status indicators
- Command palette (Cmd+K) and keyboard shortcuts
- Native OS notifications for calls and messages
- OS keychain for credential storage (macOS Keychain, Windows Credential Manager)
- Android via Tauri 2.x with adaptive UI

## Encryption

| Layer | What | How |
|-------|------|-----|
| SIP signaling | Call setup, registration | SIP TLS by default when certificate paths are configured; TCP fallback; UDP only when explicitly enabled |
| Voice/video media | RTP audio and video streams | SRTP with DTLS key exchange |
| Chat messages | Matrix-backed 1:1 and group conversations | Olm / Megolm |
| File attachments | Uploaded files | AES-256-CTR with per-file key |
| Server storage | SQLite fallback encryption | ChaCha20-Poly1305 |
| Credentials | Passwords and tokens | OS keychain (never written to disk) |
| Server API | HTTP endpoints | Token-based auth with 12-hour TTL |
| Admin governance | Audit, DLP, eDiscovery, retention | Server-side policy engine + export controls |

## Architecture

```
┌─────────────────────────────────────────────────────────────┐
│  Pale Desktop / Mobile Client                               │
│  ┌──────────┐  ┌──────────┐  ┌───────────┐  ┌───────────┐  │
│  │  React   │  │  Tauri   │  │  PJSIP    │  │  Matrix   │  │
│  │  UI      │  │  Rust    │  │  Engine   │  │  SDK      │  │
│  └──────────┘  └──────────┘  └───────────┘  └───────────┘  │
└───────────────────────┬─────────────────────────────────────┘
                        │
              SIP (UDP/TCP/TLS) + HTTP API + SSE
                        │
┌───────────────────────┴─────────────────────────────────────┐
│  Pale Server (self-hosted)                                   │
│  ┌───────────┐  ┌──────────┐  ┌───────────┐  ┌───────────┐ │
│  │ SIP       │  │ HTTP API │  │ Call      │  │ PostgreSQL│ │
│  │ Registrar │  │ + SSE    │  │ Router    │  │ + Memory  │ │
│  │ & Proxy   │  │ + NATS   │  │ (PBX)    │  │ Cache     │ │
│  └───────────┘  └──────────┘  └───────────┘  └───────────┘ │
│  ┌───────────┐  ┌──────────┐  ┌───────────┐                │
│  │ TURN      │  │ Metrics  │  │ LDAP/AD   │                │
│  │ (coturn)  │  │ Prometheus│ │ Auth      │                │
│  └───────────┘  └──────────┘  └───────────┘                │
└─────────────────────────────────────────────────────────────┘
```

### Rust Crates

| Crate | Purpose |
|-------|---------|
| `pjsip-sys` | Downloads PJSIP source, compiles per-platform, generates FFI bindings via bindgen |
| `pale-core` | SIP engine with dedicated worker thread, call management, audio devices, call recording, call history (SQLite), config persistence, OS keychain |
| `pale-matrix` | Matrix client lifecycle, E2E encrypted messaging, file upload/download, room management, sync loop |
| `pale-server` | Full SIP registrar/proxy, HTTP API, PBX call router, PostgreSQL persistence, compliance engine, enterprise integration registry, AI provider dispatch contracts, SSE/NATS events, rate limiting, Prometheus metrics |

### SIP Call Routing Decision Tree

When an INVITE arrives, pale-server evaluates in order:

```
1.  Authentication (SIP digest)
2.  Re-INVITE detection (hold/video toggle)
3.  CDR creation
4.  DND check → reject or forward
5.  Forward-Always → redirect
6.  Holiday check → holiday destination or reject
7.  Business hours → after-hours destination or reject
8.  Conference → join active conference
9.  Queue/ACD → select agent by strategy, overflow if none
10. Ring group → simultaneous/sequential/random to members
11. IVR → accept and play greeting
12. Extension → resolve by type (user/queue/voicemail/park)
13. Routing rules → pattern match source/destination
14. Registration lookup → redirect to registered contact
15. Follow-me → sequential dial, then final action
16. Voicemail fallback → create voicemail entry
17. 480 Unavailable
```

## Quick Start

### Prerequisites

- Node.js 22+
- Rust 1.93+
- Docker and Docker Compose (only for the Docker server path)
- autoconf, automake (for PJSIP build)

```bash
# macOS
brew install openssl@3 opus autoconf automake

# Ubuntu/Debian
sudo apt install libwebkit2gtk-4.1-dev libgtk-3-dev libasound2-dev \
  libpulse-dev libssl-dev libopus-dev autoconf automake
```

### Install Pale Server

Pale Server can run from Docker, Linux packages, a bare-metal binary, or the
Windows server installer. For a first production-style install, use one of the
packaged paths so the service, data directory, and local configuration are
created consistently.

#### Windows Server Installer

Download the **Pale Server** installer,
`PaleServerSetup-<version>-x64.exe`, from the project downloads page or from the
`pale-server-windows-installer` GitHub Actions artifact.

The installer:

- installs `pale-server.exe` under `Program Files`;
- creates `C:\ProgramData\Pale Server`;
- generates local server/storage secrets;
- asks for the admin password during configuration;
- creates a `PaleServer` Windows service;
- adds Start Menu shortcuts for configure, start, stop, restart, health check,
  and uninstall.

By default the Windows installer binds the HTTP API to `127.0.0.1:8080` and
does not expose SIP until an admin configures a SIP backend. Use TLS termination
in front of the service before exposing it to users over a network.

#### Docker Compose

```bash
# Generate secrets (writes .env, which docker compose reads automatically)
./scripts/generate-secrets.sh

# Start server stack (PostgreSQL + pale-server + TURN relay)
docker compose up -d

# Verify
curl http://localhost:8090/health
# {"ok":true,"service":"pale-server","status":"healthy"}
```

#### Bare-Metal Linux

For Debian, Ubuntu, RHEL, Rocky, AlmaLinux, CentOS, and Fedora-compatible
systems, use the installer script:

```bash
curl -fsSL https://drcpbx.com/install-pale-server.sh | sudo bash
```

The script installs from the public Pale package repository, writes local secrets
to `/etc/pale-server/pale-server.env`, creates or updates the systemd service,
and starts Pale Server.

For a manual install, keep the environment file outside the repository and set
at least:

```bash
PALE_SERVER_TOKEN=<strong random value>
PALE_ADMIN_PASSWORD=<strong admin password>
PALE_STORAGE_KEY=<strong random value>
PALE_HTTP_ADDR=127.0.0.1:8080
PALE_DATA_DIR=/var/lib/pale-server
PALE_SIP_BACKEND=udp-parser
PALE_SIP_TLS_CERT=/etc/letsencrypt/live/your-host/fullchain.pem
PALE_SIP_TLS_KEY=/etc/letsencrypt/live/your-host/privkey.pem
PALE_SIP_EXTERNAL_ADDR=your-host.example.com:5060
PALE_SIP_TLS_EXTERNAL_ADDR=your-host.example.com:5061
```

The built-in parser is the current registrar/PBX path. It starts SIP TLS on
5061 when `PALE_SIP_TLS_CERT` and `PALE_SIP_TLS_KEY` are set, starts SIP TCP on
5060 by default, and keeps UDP off unless `PALE_SIP_UDP=true` plus
`PALE_ALLOW_INSECURE_SIP_UDP=1` are both set. If you prefer OpenSIPS, Kamailio,
or another registrar/proxy in front of Pale, keep Pale's HTTP API behind TLS and
point clients at that SIP edge.

The server exposes:
- **HTTP API** on port 8090
- **SIP TCP** on port 5060
- **SIP TLS** on port 5061 when cert/key paths are set
- **SIP UDP** on port 5060 only when explicitly enabled as a fallback
- **TURN relay** on port 3478

All settings are environment variables — see [`.env.example`](.env.example)
for the full annotated list. Two you will almost certainly want in production:

- `PALE_SIP_EXTERNAL_ADDR` — the public hostname/IP clients use to reach SIP TCP.
  Without it, remote clients may be given a loopback registrar address.
- `PALE_SIP_TLS_EXTERNAL_ADDR` — optional public SIP TLS hostname and port. If
  omitted, Pale derives it from `PALE_SIP_EXTERNAL_ADDR` and `PALE_SIP_TLS_PORT`.
- `PALE_SIP_BACKEND` — defaults to `udp-parser`, which now means the built-in
  parser registrar over TLS/TCP first. Use a dedicated SIP registrar/proxy in
  front of Pale if you need a deeper SIP edge.
- `PALE_SIP_TLS_CERT` / `PALE_SIP_TLS_KEY` — providing both enables SIP over
  TLS automatically. UDP remains disabled unless explicitly enabled.
- `NATS_URL` — optional NATS server URL, for example `nats://nats:4222`.
  When set, pale-server publishes SSE events to `pale.events.<event_type>` as
  JSON for server-side automations and integrations.
- `PALE_RETENTION_ENFORCEMENT_INTERVAL_SECS` — optional scheduled retention
  enforcement interval. Unset/`0` disables it; `86400` runs daily.

### Build & Run Desktop Client

```bash
git clone https://github.com/loreste/palephone.git
cd palephone

npm install
npm run tauri dev       # Development with hot reload
npm run tauri build     # Production installer
```

### First Login

1. Launch the Pale app
2. Enter your server URL (e.g., `http://your-server:8090`)
3. Sign in with your SIP URI and password
4. The app provisions your SIP account, stores credentials in OS keychain, and connects

Admin users see an **Admin** tab in the bottom navigation for full PBX management.

### Android

```bash
rustup target add aarch64-linux-android
npm run tauri android init
npm run tauri android dev     # USB debugging
npm run tauri android build   # Release APK
```

See [ANDROID_SETUP.md](ANDROID_SETUP.md) for detailed environment setup.

## Docker Compose Services

| Service | Image | Purpose |
|---------|-------|---------|
| `pale-server` | Built from source | SIP registrar, HTTP API, PBX, call center |
| `postgres` | postgres:16-alpine | User data, CDR, voicemail, call settings |
| `coturn` | coturn/coturn | TURN relay for NAT traversal |

### Environment Variables

| Variable | Required | Description |
|----------|----------|-------------|
| `PALE_SERVER_TOKEN` | Yes | API authentication token (min 24 chars) |
| `PALE_ADMIN_PASSWORD` | Yes | Admin account password (min 24 chars) |
| `PALE_STORAGE_KEY` | Yes | Encryption key for SQLite fallback (min 24 chars) |
| `POSTGRES_PASSWORD` | Yes | PostgreSQL password |
| `TURN_SECRET` | Yes | TURN relay shared secret |
| `PALE_ADMIN_USERNAME` | No | Admin username (default: `admin`) |
| `PALE_SIP_EXTERNAL_ADDR` | No | Public SIP address for clients (default: derived from bind address) |
| `PALE_RETENTION_ENFORCEMENT_INTERVAL_SECS` | No | Scheduled retention enforcement interval in seconds (`0` disables) |
| `PALE_RETENTION_ENFORCEMENT_RUN_ON_STARTUP` | No | Run retention enforcement once when the server starts |
| `RUST_LOG` | No | Log level (default: `info`) |

## Database

Pale Server uses PostgreSQL with migrations applied automatically at startup. The schema covers users, SIP accounts, registrations, calls, rooms, messages, files, recordings, PBX routing, queues, IVR, call center data, meetings, governance, policy, security, devices, app integrations, federation, signage, compliance, and data residency.

Data is cached in memory for fast lookups and written through to PostgreSQL for persistence.

## API

Pale Server exposes a broad HTTP API plus SSE events. Key groups:

| Group | Endpoints | Auth |
|-------|-----------|------|
| Auth | `POST /v1/auth/login` | None |
| Users | `/v1/users` CRUD | Bearer token |
| SIP | `/v1/sip/accounts`, `/v1/sip/registrations` | Bearer token |
| Presence | `/v1/presence` GET/PUT | Bearer token |
| Call Settings | `/v1/call-settings` GET/PUT | Bearer token |
| Voicemail | `/v1/voicemail` GET/PUT/DELETE | Bearer token |
| Queues | `/v1/queues` CRUD | Bearer token (admin) |
| Ring Groups | `/v1/ring-groups` CRUD | Bearer token (admin) |
| IVR | `/v1/ivrs` CRUD | Bearer token (admin) |
| Extensions | `/v1/extensions` CRUD | Bearer token (admin) |
| Routing | `/v1/routing-rules` CRUD | Bearer token (admin) |
| Business Hours | `/v1/business-hours` CRUD | Bearer token (admin) |
| Holidays | `/v1/holidays` CRUD | Bearer token (admin) |
| CDR | `/v1/cdrs` GET | Bearer token (admin) |
| Agents | `/v1/agents` CRUD, `/v1/agents/{uri}/state` PUT | Bearer token (admin) |
| Wallboard | `/v1/wallboard` GET | Bearer token (admin) |
| QA | `/v1/qa/scorecards` CRUD | Bearer token (admin) |
| Conferences | `/v1/conferences` CRUD | Bearer token |
| Files | `/v1/files` POST/GET/DELETE | Bearer token |
| Rooms | `/v1/rooms` CRUD, `/v1/rooms/{id}/messages` | Bearer token |
| Meetings | Scheduled meetings, templates, recordings, attendance, polls, reactions | Bearer token |
| Compliance | Retention, eDiscovery, DLP, security score, reviews, labels, barriers | Bearer token (admin) |
| Identity | SSO, MFA foundations, conditional access, custom roles, policy packages | Bearer token (admin) |
| Enterprise Integrations | `/v1/admin/enterprise-integrations`, readiness, health, provider probes, validation, deployment plan | Bearer token (admin) |
| AI Providers | `/v1/ai/providers`, `/v1/ai/llm/chat`, `/v1/ai/stt/transcribe`, `/v1/ai/tts/synthesize` | Bearer token |
| Emergency/PSTN | Emergency locations, assignments, call plans, SIP gateways, location routing | Bearer token (admin) |
| Events | `GET /v1/events` (SSE stream) | Bearer token |
| Health | `GET /health`, `GET /metrics` | None |

## Project Structure

```
pale/
  src/                        React frontend
    components/               UI components (dialpad, chat, call, admin, settings, etc.)
    store/                    Zustand state stores (call, chat, presence, server, UI)
    hooks/                    SIP events, server events, platform detection
    lib/                      Tauri IPC wrappers, admin API, utilities
  src-tauri/                  Rust backend
    src/                      Tauri app entry, IPC commands, event bridge
    crates/
      pjsip-sys/              PJSIP FFI bindings (auto-built per platform)
      pale-core/              SIP engine, call recording, audio, config, keychain
      pale-matrix/            Matrix E2E chat, file transfer, room sync
      pale-server/            Full SIP server, HTTP API, PBX, PostgreSQL
  docker-compose.yml          Server deployment (PostgreSQL + pale-server + TURN)
  Dockerfile.pale-server      Multi-stage production build
  turnserver.conf             coturn TURN relay configuration
  .github/workflows/          CI/CD (build, test, release for all platforms)
```

## CI/CD

| Workflow | Trigger | Produces |
|----------|---------|----------|
| `ci.yml` | Push / PR to main | Build + test on macOS, Windows, Linux |
| `release.yml` | Git tag `v*` | .dmg, .msi, .exe, .deb, .AppImage |
| `android.yml` | Push / PR to main | .apk |

## Security

- SIP digest authentication (MD5 HA1)
- Token-based API auth with 12-hour TTL and auto-refresh
- Admin login rate limiting (5 failures → 15-minute lockout per IP)
- API rate limiting (100 RPS per IP)
- No hardcoded credentials — all secrets via environment variables
- CORS and Content-Type enforcement on all endpoints
- Audit trail for all administrative actions
- DLP, eDiscovery, retention, compliance review, and security score workflows
- External ATP/CASB/provider readiness checks so risky integrations are visible before a tenant is marked ready

## Documentation

- [ARCHITECTURE.md](ARCHITECTURE.md) — Current system architecture, server/client responsibilities, integration boundaries
- [ARCHITECTURE_V2.md](ARCHITECTURE_V2.md) — Video, Matrix chat, file transfer, E2E encryption design
- [UI_UX_SPEC.md](UI_UX_SPEC.md) — Design system, component wireframes, interaction patterns
- [ANDROID_SETUP.md](ANDROID_SETUP.md) — Android development environment setup

## License

This project is licensed under the GNU General Public License v2.0. See [LICENSE](LICENSE) for details.
