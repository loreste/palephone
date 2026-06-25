# Pale

A self-hosted unified communications platform — voice, video, messaging, and full PBX in a single application. Think Teams or Zoom Phone, but you own the infrastructure.

## Why Pale?

- **Complete phone system.** Not just a softphone — Pale includes its own SIP registrar, call router, PBX, and call center. No need for Asterisk, FreePBX, or any external PBX.
- **Your data stays yours.** Every call, message, and file stays on your servers. SIP signaling, media, chat — all self-hosted. Nothing routes through third-party clouds.
- **End-to-end encrypted chat.** Messages and files encrypted client-side via Matrix protocol. Your server only sees ciphertext.
- **One app, not five.** Calling, video, chat, file sharing, voicemail, presence, and admin — all in one interface.
- **Works everywhere.** macOS, Windows, Linux, Android, and iOS from a single codebase.

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

### Call Center

- **Agent profiles** — roles (agent, supervisor, QA, admin), skills, max concurrent calls
- **Agent state management** — available, on-call, wrap-up, break, training, meeting, offline
- **Real-time wallboard** — live queue metrics, agent status, SLA tracking, calls waiting/active
- **QA scorecards** — score and review agent calls with comments and metrics
- **Supervisor tools** — monitor queue performance, manage agent states

### Messaging & Collaboration

- End-to-end encrypted chat via Matrix protocol (Olm for 1:1, Megolm for groups)
- 1:1 direct messages and group rooms
- Teams-style team spaces with channel rooms
- Scheduled meetings that create/join conference-backed calls
- Typing indicators and read receipts
- Message edit and delete
- Encrypted file sharing (AES-256-CTR per-file keys)
- Drag-and-drop upload with any file type
- Full-text message search

### Presence & Directory

- Real-time presence — online, busy, away, on-call, DND, offline
- Auto-presence from call state (on-call when in a call, online when idle)
- User directory with search and BLF indicators (see who's on a call)
- Click-to-call and click-to-chat from the directory

### Conferences

- Audio and video conference rooms
- Admin and user-facing conference management
- Join from chat view or by dialing conference URI

### Admin Panel

21 management tabs accessible to admin users:

| Category | Tabs |
|----------|------|
| **System** | Overview, Audit Log |
| **Users** | Users (CRUD + role assignment), SIP Accounts, Directory (LDAP/AD) |
| **PBX** | Extensions, Routing, Ring Groups, Queues, IVR, Business Hours, Holidays, Paging, Media |
| **Call Center** | Agents, Wallboard, QA Scorecards, CDR |
| **Collaboration** | Conferences, Files, Active Calls |

- **Role-based access** — admin tab only visible to admin users
- **LDAP/Active Directory integration** — auto-provision users from AD, map groups to roles
- **SCIM-style user provisioning** — `/v1/scim/v2/Users` endpoints for business lifecycle automation
- **Governance controls** — retention policy records and admin eDiscovery export for server-native room messages
- **Audit logging** — every admin action logged with principal, action, target, timestamp
- **Real-time refresh** — SSE events + 30-second polling for live data

### Desktop & Mobile

- Modern dark-first UI with Tailwind CSS
- System tray with status indicators
- Command palette (Cmd+K) and keyboard shortcuts
- Native OS notifications for calls and messages
- OS keychain for credential storage (macOS Keychain, Windows Credential Manager)
- Android and iOS via Tauri 2.x with adaptive UI

## Encryption

| Layer | What | How |
|-------|------|-----|
| SIP signaling | Call setup, registration | TLS (port 5061) |
| Voice/video media | RTP audio and video streams | SRTP with DTLS key exchange |
| Chat messages | 1:1 and group conversations | Olm / Megolm (Matrix E2E) |
| File attachments | Uploaded files | AES-256-CTR with per-file key |
| Server storage | SQLite fallback encryption | ChaCha20-Poly1305 |
| Credentials | Passwords and tokens | OS keychain (never written to disk) |
| Server API | HTTP endpoints | Token-based auth with 12-hour TTL |

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
│  │ Registrar │  │ 79 routes│  │ Router    │  │ + Memory  │ │
│  │ & Proxy   │  │ + SSE    │  │ (PBX)    │  │ Cache     │ │
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
| `pale-server` | Full SIP registrar/proxy, HTTP API, PBX call router, PostgreSQL persistence, SSE/NATS events, rate limiting, Prometheus metrics |

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
- Docker and Docker Compose (for pale-server)
- autoconf, automake (for PJSIP build)

```bash
# macOS
brew install openssl@3 opus autoconf automake

# Ubuntu/Debian
sudo apt install libwebkit2gtk-4.1-dev libgtk-3-dev libasound2-dev \
  libpulse-dev libssl-dev libopus-dev autoconf automake
```

### Deploy Pale Server

```bash
# Generate secrets (writes .env, which docker compose reads automatically)
./scripts/generate-secrets.sh

# Start server stack (PostgreSQL + pale-server + TURN relay)
docker compose up -d

# Verify
curl http://localhost:8090/health
# {"ok":true,"service":"pale-server","status":"healthy"}
```

The server exposes:
- **HTTP API** on port 8090
- **SIP UDP** on port 5060
- **SIP TLS** on port 5061
- **TURN relay** on port 3478

All settings are environment variables — see [`.env.example`](.env.example)
for the full annotated list. Two you will almost certainly want in production:

- `PALE_SIP_EXTERNAL_ADDR` — the public hostname/IP clients use to reach SIP.
  Without it the registrar is advertised as `127.0.0.1` and only local
  clients can register.
- `PALE_SIP_TLS_CERT` / `PALE_SIP_TLS_KEY` — providing both enables SIP over
  TLS automatically (and disables plain UDP by default).
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

Pale Server uses PostgreSQL with 9 migrations applied automatically at startup:

| Migration | Tables |
|-----------|--------|
| 001 | users, sip_accounts, sip_registrations, sip_dialogs, sip_messages, presence, calls, files, routing_rules, conferences, audit_events |
| 002 | rooms, room_members, room_messages, read_receipts, avatars, search |
| 003 | voicemails, call_recordings |
| 004 | DBA constraints, indexes, retention policies |
| 005 | User authentication (password_hash, role) |
| 006 | Ring groups, IVR, extensions, routing enhancements |
| 007 | Voicemail settings, follow-me, call forwarding |
| 008 | Call queues, business hours, holidays, call park, speed dial, CDR, paging, music on hold |
| 009 | Agent profiles, agent state log, queue metrics, QA scorecards |

Data is cached in memory (ShardedMaps) for fast lookups and written through to PostgreSQL for persistence.

## API

Pale Server exposes 79 HTTP endpoints. Key groups:

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

## Documentation

- [ARCHITECTURE.md](ARCHITECTURE.md) — SIP/media stack, codec negotiation, OS-specific build details
- [ARCHITECTURE_V2.md](ARCHITECTURE_V2.md) — Video, Matrix chat, file transfer, E2E encryption design
- [UI_UX_SPEC.md](UI_UX_SPEC.md) — Design system, component wireframes, interaction patterns
- [ANDROID_SETUP.md](ANDROID_SETUP.md) — Android development environment setup

## License

This project is licensed under the GNU General Public License v2.0. See [LICENSE](LICENSE) for details.
