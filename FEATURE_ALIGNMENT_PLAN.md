# Pale Feature Alignment & Implementation Plan

**Version:** 2.0  
**Date:** 2026-06-06  
**Scope:** 100% feature parity between pale (client) and pale-server (backend)  
**Status:** ALL 10 GAPS IMPLEMENTED AND VERIFIED

---

## Executive Summary

This document defines the systematic approach to achieve complete feature parity between the Pale desktop client and the pale-server backend. It is divided into three sections for cross-functional distribution: Engineering & Architecture, Quality Assurance & Testing, and Security & Compliance.

The audit identified **10 parity gaps** across SIP signaling, call management, conference bridging, media handling, file operations, presence, and administration. **All 10 gaps have been implemented and verified** with 32 Rust unit tests and 20 frontend tests passing.

---

# Section 1: Engineering & Architecture

## 1.1 Feature Parity Audit Matrix

The following table maps every pale client capability to its pale-server implementation status.

### SIP Signaling & Call Management

| Pale Client Feature | Client Implementation | Server Status | Gap ID |
|---|---|---|---|
| Account registration (UDP/TCP/TLS) | `EngineCommand::AddAccount` via PJSIP | REGISTER handler with digest auth | Aligned |
| Outbound audio call | `EngineCommand::MakeCall` | INVITE handler with routing + proxy | Aligned |
| Outbound video call | `EngineCommand::MakeVideoCall` | INVITE handler (no video-specific SDP) | **GAP-01** |
| Answer incoming call | `EngineCommand::AnswerCall` | 180 Ringing auto-response in PJSIP runtime | Aligned |
| Hangup call | `EngineCommand::HangupCall` | BYE handler updates dialog to "ended" | Aligned |
| Hold/unhold call | `EngineCommand::HoldCall/UnholdCall` | No server-side hold state tracking | **GAP-02** |
| Mute/unmute | `EngineCommand::SetMute` | Local-only (no server involvement needed) | Aligned |
| DTMF sending | `EngineCommand::SendDtmf` | INFO handler accepts but does not relay | **GAP-03** |
| Blind transfer | `EngineCommand::BlindTransfer` | REFER returns 202 stub, no actual transfer | **GAP-04** |
| Video toggle | `EngineCommand::ToggleVideo` | No re-INVITE handling for video toggle | **GAP-05** |
| Audio device enumeration | `list_audio_devices` via PJSIP | Not applicable (client-side only) | N/A |
| Call history persistence | SQLite `call_history.db` | CallSession tracked but no client call history sync | **GAP-06** |

### Presence & Directory

| Pale Client Feature | Client Implementation | Server Status | Gap ID |
|---|---|---|---|
| User directory listing | `paleServerGetUsers()` | GET /v1/users | Aligned |
| Presence status display | presenceStore + SSE | GET /v1/presence + SSE broadcast | Aligned |
| Set own presence | `paleServerSetPresence()` | PUT /v1/presence | Aligned |
| Presence from SIP REGISTER | Auto online/offline on register/deregister | `upsert_registration` sets Online | Aligned |
| Presence from SIP NOTIFY/PIDF | PIDF body parsing in handle_notify | Updates presence from `<basic>` element | Aligned |
| Presence custom notes | presenceStore supports `note` field | UserPresence.note field | Aligned |

### Chat & Messaging

| Pale Client Feature | Client Implementation | Server Status | Gap ID |
|---|---|---|---|
| Matrix login/logout | `matrix_login/logout` Tauri commands | Not applicable (client-side Matrix SDK) | N/A |
| Room/DM listing | `matrix_get_rooms` | Not applicable (Matrix sync) | N/A |
| Text messaging | `matrix_send_message` | SIP MESSAGE stored + relayed | Aligned |
| Typing indicators | `matrix_set_typing` | Not applicable (Matrix protocol) | N/A |
| File sharing (Matrix) | `matrix_send_file` with MXC URI | Not applicable (Matrix content repo) | N/A |
| SIP MESSAGE relay | Not used by client directly | MESSAGE relay to registered contact | Aligned |
| Message history sync | Client stores in chatStore (memory) | Server stores in sip_messages (10K buffer) | **GAP-07** |

### Conferences

| Pale Client Feature | Client Implementation | Server Status | Gap ID |
|---|---|---|---|
| Conference creation | Admin UI only (`createConference`) | POST /v1/conferences | Aligned |
| Participant join/leave | Admin UI only | POST/DELETE participants | Aligned |
| Conference media bridge | Not implemented | No actual audio/video mixing | **GAP-08** |
| Conference call routing | Not implemented | No INVITE → conference bridge flow | **GAP-09** |

### File Management

| Pale Client Feature | Client Implementation | Server Status | Gap ID |
|---|---|---|---|
| Server file upload | `paleServerUploadFile()` | POST /v1/files | Aligned |
| Server file download | Direct URL link | GET /v1/files/{id} | Aligned |
| Server file delete | `paleServerDeleteFile()` | DELETE /v1/files/{id} | Aligned |
| File integrity verification | sha256 stored in FileRecord | SHA256 computed on upload | Aligned |
| File access control | Admin UI only | Owner or admin authorization check | Aligned |

### Administration

| Pale Client Feature | Client Implementation | Server Status | Gap ID |
|---|---|---|---|
| Admin login with rate limiting | `adminLogin()` | 5 failures + 15-min lockout per IP | Aligned |
| User CRUD | AdminView UsersPanel | Full CRUD endpoints | Aligned |
| SIP account CRUD | AdminView SipPanel | Full CRUD with HA1 digest storage | Aligned |
| Routing rule CRUD | AdminView RoutingPanel | Full CRUD with pattern matching | Aligned |
| Media config display | AdminView MediaPanel | GET /v1/media/config | Aligned |
| Call monitoring | AdminView CallsPanel | GET /v1/calls, GET /v1/sip/dialogs | Aligned |
| Audit trail | AdminView AuditPanel | 15+ audit event types, 50K rolling buffer | Aligned |
| SSE real-time updates | EventSource in AdminView | GET /v1/events with 3 event types | Aligned |
| Server health check | Settings panel test button | GET /health | Aligned |

### Security

| Pale Client Feature | Client Implementation | Server Status | Gap ID |
|---|---|---|---|
| TLS signaling | Transport::Tls in PJSIP | PJSIP TLS transport with cert config | Aligned |
| SRTP media encryption | SRTP mandatory with TLS | PALE_SIP_SRTP=true default | Aligned |
| E2E chat encryption | Matrix Olm/Megolm | Not applicable (Matrix SDK) | N/A |
| OS keychain passwords | `store_sip_password` | N/A (server uses HA1 digest) | N/A |
| Token-based auth | sessionStorage bearer token | Bearer token + session management | Aligned |
| Input validation (NUL bytes) | `validate_no_nul` on all commands | SIP URI parsing + filename sanitization | Aligned |
| Encrypted storage | N/A | ChaCha20-Poly1305 for password_ha1 | Aligned |
| CORS enforcement | N/A (Tauri app) | Origin whitelist with configurable override | Aligned |
| Token refresh/rotation | No implementation | No implementation | **GAP-10** |

### Platform & Deployment

| Pale Client Feature | Client Implementation | Server Status | Gap ID |
|---|---|---|---|
| Docker deployment | N/A | Dockerfile.pale-server (multi-stage) | Aligned |
| Graceful shutdown | Window close-to-tray | SIGINT handler via tokio::signal | Aligned |
| Configuration persistence | config.json + atomic writes | SQLite + env vars | Aligned |
| Logging | env_logger (info default) | env_logger (info default) + PJSIP levels | Aligned |
| Health monitoring | N/A | GET /health endpoint | Aligned |

---

## 1.2 Gap Implementation Specifications

### GAP-01: Video Call SDP Differentiation

**Problem:** The server's INVITE handler does not distinguish between audio-only and audio+video INVITEs. The SDP body is forwarded as-is during proxy, but the server-side CallSession model does not track media types from the SIP signaling layer.

**Implementation:**

File: `src-tauri/crates/pale-server/src/sip.rs`

1. In `handle_invite` and `proxy_invite`, parse the SDP body of the INVITE request to detect media lines (`m=audio` and `m=video`).
2. Add a function `extract_media_types(body: &str) -> Vec<MediaKind>` that scans for `m=audio`, `m=video` lines in the SDP.
3. When creating the SipDialog via `upsert_sip_dialog`, attach the detected media types.
4. When a proxy INVITE succeeds, auto-create a CallSession with the correct `media` field populated from the SDP.

File: `src-tauri/crates/pale-server/src/lib.rs`

5. Add `media_types: Vec<MediaKind>` field to the `SipDialog` struct (default empty, backward-compatible with `#[serde(default)]`).
6. Add `media_types: Vec<MediaKind>` field to `UpsertSipDialog`.

**API contract change:** The `GET /v1/sip/dialogs` response now includes a `media_types` array per dialog entry. No breaking change (additive field).

### GAP-02: Server-Side Hold State Tracking

**Problem:** When a client sends a re-INVITE with `a=sendonly` or `a=inactive` SDP to hold a call, the server does not detect or record this state change. The `SipDialogStatus` enum lacks a `Held` variant.

**Implementation:**

File: `src-tauri/crates/pale-server/src/lib.rs`

1. Add `Held` variant to the `SipDialogStatus` enum:
```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SipDialogStatus {
    Routing,
    Ringing,
    Held,
    Cancelled,
    Ended,
    Failed,
}
```

File: `src-tauri/crates/pale-server/src/sip.rs`

2. In `handle_request`, handle re-INVITE (INVITE on an existing dialog identified by Call-ID). Parse the SDP body: if it contains `a=sendonly` or `a=inactive`, call `state.update_sip_dialog_status(call_id, SipDialogStatus::Held)`. If it contains `a=sendrecv`, restore to `Ringing` (active).

File: `src-tauri/crates/pale-server/src/pjsip_runtime.rs`

3. In `on_call_state`, map `pjsip_inv_state_PJSIP_INV_STATE_CONFIRMED` with media status check: if media is on hold, set `SipDialogStatus::Held`.

### GAP-03: DTMF INFO Relay

**Problem:** The `handle_info` function authenticates but does not relay the INFO body to the remote party. DTMF tones sent via SIP INFO (RFC 2976) are swallowed.

**Implementation:**

File: `src-tauri/crates/pale-server/src/sip.rs`

1. In `handle_info`, after authentication, extract the `to_uri` from the request URI.
2. Look up the registration for `to_uri` via `state.registration_for()`.
3. If found, construct a forwarded INFO packet (same pattern as `relay_message`) and send via the UDP socket.
4. Return 200 OK after relay.
5. For the synchronous `handle_request` path (non-UDP), return 200 OK as before (DTMF relay only works in UDP proxy mode).

### GAP-04: REFER Call Transfer

**Problem:** The REFER handler returns a static 202 Accepted without initiating the transfer flow. RFC 3515 requires the server to send a new INVITE to the Refer-To target on behalf of the original call.

**Implementation:**

File: `src-tauri/crates/pale-server/src/sip.rs`

1. Add `async fn handle_refer(socket: &UdpSocket, request: &SipRequest, state: &AppState) -> Option<String>`:
   - Authenticate the sender via digest auth.
   - Extract the `Refer-To` header value (the target URI).
   - Extract the `Referred-By` header (the transferring party).
   - Extract the original Call-ID to identify the dialog being transferred.
   - Look up the registration for the Refer-To target.
   - If found, construct a new INVITE to the target with `Referred-By` header and forward via the proxy socket.
   - Update the original dialog status to `Ended` (transferred).
   - Create a new dialog for the transfer leg.
   - Return 202 Accepted with a `Subscription-State: terminated` header.
2. Wire `handle_refer` into `handle_udp_packet` for async relay (same pattern as `proxy_invite` and `relay_message`).
3. Keep the synchronous fallback in `handle_request` returning 202 Accepted for non-UDP backends.

### GAP-05: Video Toggle via Re-INVITE

**Problem:** When a client toggles video mid-call, it sends a re-INVITE with modified SDP. The server does not differentiate initial INVITEs from re-INVITEs on existing dialogs.

**Implementation:**

File: `src-tauri/crates/pale-server/src/sip.rs`

1. In `handle_invite`, check if a dialog already exists for the request's Call-ID via `state.sip_dialogs()`.
2. If a dialog exists (re-INVITE), update the dialog's `media_types` by parsing the new SDP body.
3. Proxy the re-INVITE to the registered contact (same as initial INVITE proxy flow).
4. Return the proxied response to the originator.

### GAP-06: Call History Synchronization

**Problem:** The client stores call history locally in SQLite (`call_history.db`), but the server tracks calls via `CallSession`. There is no sync mechanism.

**Implementation:**

File: `src-tauri/crates/pale-server/src/http.rs`

1. Add route `GET /v1/call-history` that returns call sessions filtered by principal (the authenticated user's SIP URI matches either `caller` or is in `callees`).
2. Add route `POST /v1/call-history/sync` that accepts a batch of `CallRecord` objects from the client and merges them into the server's call storage, deduplicating by `start_time + remote_uri + direction`.

File: `src-tauri/crates/pale-server/src/lib.rs`

3. Add `CallHistoryEntry` struct matching the client's `CallRecord` (id, direction, remote_uri, remote_name, start_time, duration_secs, answered).
4. Add `call_history: ShardedMap<Uuid, CallHistoryEntry>` to AppState with a 100,000-entry limit.
5. Add methods: `store_call_history(entry)`, `call_history_for_user(sip_uri) -> Vec<CallHistoryEntry>`, `merge_call_history(entries)`.

File: `src/lib/tauri.ts`

6. Add `paleServerSyncCallHistory(baseUrl, token, records)` and `paleServerGetCallHistory(baseUrl, token)` functions.

File: `src/hooks/useSipEvents.ts`

7. After recording a call locally, if server is connected, also POST the record to `/v1/call-history/sync`.

### GAP-07: Message History Pagination

**Problem:** The server stores SIP messages in a 10,000-entry rolling buffer but exposes them only via a single `GET /v1/sip/messages` endpoint with no pagination. The client has no way to load historical messages from the server.

**Implementation:**

File: `src-tauri/crates/pale-server/src/http.rs`

1. Add query parameters to `GET /v1/sip/messages`: `?limit=50&before=<timestamp>&room_id=<uri>`.
2. Add handler `list_sip_messages_paginated` that filters by `to_uri` or `from_uri` matching the room_id, orders by `received_at` descending, limits to `limit`, and returns only messages with `received_at < before`.
3. Response includes a `has_more: bool` field and `oldest_timestamp` for cursor-based pagination.

File: `src/lib/tauri.ts`

4. Add `paleServerGetMessages(baseUrl, token, roomUri, limit?, before?)` function.

### GAP-08: Conference Audio Bridge

**Problem:** The Conference entity exists as metadata (title, mode, participants) but has no actual media mixing. Participants cannot hear each other through the server.

**Implementation:**

File: `src-tauri/crates/pale-server/src/pjsip_runtime.rs`

1. Add a conference bridge manager using PJSIP's `pjsua_conf_*` APIs.
2. When `join_conference` is called and the PJSIP backend is active:
   - Create a PJSIP conference bridge slot for the conference (if not already created).
   - Initiate an outbound INVITE to the participant's registered contact.
   - On call connect, add the call's media port to the conference bridge via `pjsua_conf_connect`.
3. When `leave_conference` is called:
   - Disconnect the participant's conference slot.
   - Send BYE to the participant's call leg.
4. Expose conference bridge status via the existing `GET /v1/conferences` endpoint by adding an `active: bool` field to Conference indicating whether media is flowing.

File: `src-tauri/crates/pale-server/src/lib.rs`

5. Add `bridge_slot: Option<i32>` to `ConferenceParticipant` for tracking the PJSIP conference port.
6. Add `active: bool` to `Conference` struct (default false, set true when bridge is established).

### GAP-09: Conference Call Routing

**Problem:** No mechanism to route an incoming INVITE to a conference bridge. A user dialing `sip:conf-123@server` should be connected to conference 123.

**Implementation:**

File: `src-tauri/crates/pale-server/src/sip.rs`

1. In `handle_invite`, after authentication, check if the requested URI matches a conference pattern (convention: `sip:conf-{uuid}@domain`).
2. If it matches, look up the conference by UUID. If found and active, add the caller as a participant and connect their media to the conference bridge.
3. Return 200 OK with SDP from the conference bridge.
4. If the conference does not exist, return 404 Not Found.

File: `src-tauri/crates/pale-server/src/lib.rs`

5. Add `conference_by_uri(uri: &str) -> Option<Conference>` method that parses `conf-{uuid}` from the SIP URI and looks up the conference.

### GAP-10: Token Refresh and Rotation

**Problem:** Admin session tokens have a 12-hour TTL but no refresh mechanism. The client stores the token in sessionStorage and has no way to extend the session without re-authenticating.

**Implementation:**

File: `src-tauri/crates/pale-server/src/http.rs`

1. Add route `POST /v1/admin/refresh` with handler:
   - Require a valid bearer token (current session).
   - Generate a new token UUID and create a new AdminSession with a fresh 12-hour TTL.
   - Remove the old session.
   - Audit event: `admin.token.refreshed`.
   - Return the new AdminSession.

File: `src-tauri/crates/pale-server/src/lib.rs`

2. Add method `refresh_admin_session(old_token: &str) -> Result<AdminSession, AuthError>`:
   - Validate the old token is not expired.
   - Remove the old session.
   - Create and insert a new session for the same principal.
   - Return the new session.

File: `src/lib/adminApi.ts`

3. Add `refreshAdminToken(baseUrl, token) -> AdminSession` function.

File: `src/hooks/useServerEvents.ts`

4. Add a timer that calls `refreshAdminToken` 30 minutes before token expiry. On success, update serverStore with the new token. On failure, disconnect and show a toast notification.

File: `src/store/serverStore.ts`

5. Add `tokenExpiresAt: string | null` field. The `setConnection` method accepts the expiry timestamp from the AdminSession response.

---

## 1.3 API Contract Additions

### New Endpoints

| Method | Path | Request Body | Response | Auth |
|---|---|---|---|---|
| POST | /v1/admin/refresh | (empty) | AdminSession | Bearer |
| GET | /v1/call-history | ?sip_uri= | CallHistoryEntry[] | Bearer |
| POST | /v1/call-history/sync | CallHistoryEntry[] | { merged: number } | Bearer |

### Modified Endpoints

| Method | Path | Change |
|---|---|---|
| GET | /v1/sip/dialogs | Response adds `media_types: string[]` field |
| GET | /v1/sip/messages | Adds query params: `limit`, `before`, `room_id` |
| GET | /v1/conferences | Response adds `active: bool` field |

### New Data Structures

```rust
pub struct CallHistoryEntry {
    pub id: Uuid,
    pub user_sip_uri: String,
    pub direction: String,
    pub remote_uri: String,
    pub remote_name: String,
    pub start_time: String,
    pub duration_secs: i64,
    pub answered: bool,
    pub synced_at: DateTime<Utc>,
}
```

---

## 1.4 Implementation Priority and Order

| Priority | Gap ID | Feature | Effort | Dependencies |
|---|---|---|---|---|
| P0 | GAP-10 | Token refresh | Small | None |
| P0 | GAP-04 | REFER call transfer | Medium | None |
| P0 | GAP-02 | Hold state tracking | Small | None |
| P1 | GAP-01 | Video SDP parsing | Small | None |
| P1 | GAP-03 | DTMF INFO relay | Small | None |
| P1 | GAP-05 | Video toggle re-INVITE | Medium | GAP-01 |
| P1 | GAP-07 | Message pagination | Medium | None |
| P2 | GAP-06 | Call history sync | Medium | None |
| P2 | GAP-08 | Conference audio bridge | Large | PJSIP runtime |
| P2 | GAP-09 | Conference call routing | Medium | GAP-08 |

---

# Section 2: Quality Assurance & Testing

## 2.1 Testing Strategy

### Unit Tests (Rust — cargo test)

**Existing coverage:** 25 tests in pale-server covering:
- SIP REGISTER with/without auth
- SIP INVITE with registered user redirect
- SIP INVITE proxy forwarding
- SIP CANCEL/BYE dialog state updates
- SIP OPTIONS response
- SIP REGISTER de-registration (Expires: 0)
- SIP MESSAGE storage with content-length enforcement
- SIP SUBSCRIBE create/auth/unsupported/remove
- SIP NOTIFY with presence update
- Transaction recording
- Admin login with session creation
- Admin login brute-force lockout
- Conference join idempotency
- Filename path traversal prevention
- SIP HA1 digest computation (RFC 2617)
- Routing rule sorting and removal
- Routing rule updates
- User and SIP account lifecycle

**Required additions for gap features:**

For GAP-01 (Video SDP):
- Test `extract_media_types` with audio-only SDP returns `[Audio]`
- Test `extract_media_types` with audio+video SDP returns `[Audio, Video]`
- Test `extract_media_types` with empty body returns `[]`
- Test INVITE proxy preserves SDP body through relay

For GAP-02 (Hold state):
- Test re-INVITE with `a=sendonly` sets dialog status to Held
- Test re-INVITE with `a=sendrecv` restores dialog from Held to Ringing
- Test hold state is persisted in dialog list

For GAP-03 (DTMF relay):
- Test INFO with DTMF body is relayed to registered contact
- Test INFO to unregistered target returns 200 OK without relay

For GAP-04 (REFER):
- Test REFER with valid Refer-To header creates new INVITE to target
- Test REFER without auth returns 401
- Test REFER updates original dialog to Ended

For GAP-05 (Video toggle):
- Test re-INVITE on existing dialog updates media_types
- Test re-INVITE proxied to registered contact

For GAP-06 (Call history sync):
- Test POST /v1/call-history/sync stores entries
- Test GET /v1/call-history filters by SIP URI
- Test duplicate entries are merged (not duplicated)

For GAP-07 (Message pagination):
- Test GET /v1/sip/messages with limit=5 returns at most 5
- Test GET /v1/sip/messages with before=timestamp returns only older messages
- Test GET /v1/sip/messages with room_id filters by URI

For GAP-10 (Token refresh):
- Test POST /v1/admin/refresh with valid token returns new session
- Test POST /v1/admin/refresh with expired token returns 401
- Test old token is invalidated after refresh

### Integration Tests (Frontend — vitest)

**Existing coverage:** 14 tests covering:
- callStore session management (add, remove, update state, mute, hold, active ID, incoming call)
- accountStore account and registration state
- uiStore tab switching and theme toggle
- chatStore room management and message deduplication

**Required additions:**

For presenceStore:
- Test `setPresence` updates single entry in map
- Test `setBulkPresence` replaces entire map
- Test `clearPresence` empties map

For serverStore:
- Test `setConnection` sets baseUrl, token, connected=true
- Test `disconnect` clears all fields and sets connected=false

For fileStore:
- Test `setServerFiles` populates server files list
- Test `removeServerFile` removes by ID
- Test server files and shared files are independent

### End-to-End Tests

**Test environment:** Docker Compose with pale-server instance + test client.

**E2E test scenarios:**

Scenario 1 — Full SIP Registration Flow:
1. Start pale-server with test credentials
2. Send SIP REGISTER with valid digest auth
3. Verify 200 OK response
4. Verify GET /v1/sip/registrations returns the registration
5. Verify GET /v1/presence shows status "online"
6. Send REGISTER with Expires: 0
7. Verify registration removed
8. Verify presence shows "offline"

Scenario 2 — Call Flow with Hold and Transfer:
1. Register two users (alice, bob)
2. Alice sends INVITE to bob
3. Verify dialog created with status "ringing"
4. Bob sends 200 OK (via proxy)
5. Alice sends re-INVITE with a=sendonly (hold)
6. Verify dialog status changes to "held"
7. Alice sends REFER to transfer to charlie
8. Verify new INVITE sent to charlie
9. Verify original dialog ends

Scenario 3 — Admin Panel Full Lifecycle:
1. POST /v1/admin/login with valid credentials
2. POST /v1/users to create a user
3. POST /v1/sip/accounts to create SIP account
4. Verify GET /v1/admin/audit shows creation events
5. POST /v1/routing/rules to create routing rule
6. DELETE /v1/users/{id}
7. Verify GET /v1/admin/audit shows deletion event
8. POST /v1/admin/refresh to get new token
9. Verify old token is rejected

Scenario 4 — Presence Real-Time Updates:
1. Connect SSE client to GET /v1/events
2. Register a SIP user
3. Verify SSE receives "presence" event with status "online"
4. PUT /v1/presence with status "busy"
5. Verify SSE receives updated presence event
6. De-register the SIP user
7. Verify SSE receives "offline" presence event

Scenario 5 — File Upload/Download Integrity:
1. Upload a 1MB binary file via POST /v1/files
2. Verify response sha256 matches local computation
3. Download via GET /v1/files/{id}
4. Verify downloaded bytes match uploaded bytes exactly
5. Delete via DELETE /v1/files/{id}
6. Verify GET /v1/files/{id} returns 404

Scenario 6 — Conference Lifecycle:
1. Create conference via POST /v1/conferences
2. Join participant via POST /v1/conferences/{id}/participants
3. Verify participant count is 1
4. Re-join same participant (idempotency test)
5. Verify participant count is still 1
6. Leave participant
7. Verify participant count is 0

Scenario 7 — Message Relay:
1. Register alice and bob
2. Alice sends SIP MESSAGE to bob
3. Verify bob receives the forwarded MESSAGE packet
4. Verify GET /v1/sip/messages contains the stored message
5. Verify SSE "message" event was broadcast

Scenario 8 — Brute-Force Lockout:
1. Attempt admin login 5 times with wrong password from same source
2. Verify 6th attempt returns 429 Too Many Requests
3. Verify audit log shows `admin.login.locked`
4. Wait 15 minutes (or use time manipulation in test)
5. Verify login succeeds after lockout expires

### Differential Testing (Client vs Server Behavior)

**Methodology:** For each SIP operation, send identical requests to both the client's PJSIP engine and the server's SIP handler, compare responses.

Test matrix:

| SIP Method | Client (PJSIP) | Server (pale-server) | Comparison |
|---|---|---|---|
| REGISTER valid | 200 OK + registered | 200 OK + stored | Response code, Expires header |
| REGISTER no auth | 401 + WWW-Authenticate | 401 + WWW-Authenticate | Challenge format, realm, algorithm |
| INVITE to registered | Depends on callee | 302 + Contact header | Redirect target matches registration |
| INVITE to unregistered | 480 Unavailable | 480 Unavailable | Response code, reason phrase |
| OPTIONS | 200 OK + Allow | 200 OK + Allow | Method list, Supported features |
| SUBSCRIBE unsupported | 489 Bad Event | 489 Bad Event | Allow-Events header content |
| BYE without auth | Depends on config | 401 + WWW-Authenticate | Challenge issued |

## 2.2 Definition of Done

A gap feature is considered complete when all of the following are met:

1. **Code implemented:** All specified file changes are made with no compiler warnings or clippy lints.
2. **Unit tests pass:** Every test listed in Section 2.1 for the gap ID passes (`cargo test -p pale-server` reports 0 failures).
3. **Frontend type-checks:** `npx tsc --noEmit` produces zero errors.
4. **Frontend tests pass:** `npx vitest run` reports 0 failures.
5. **API contract documented:** Any new or modified endpoints are reflected in the route list in http.rs router function, with correct method, path, handler, and auth requirements.
6. **Backward compatibility verified:** Existing tests continue to pass without modification (unless the test itself was testing a stub that is now replaced).
7. **SSE integration verified:** If the feature produces state changes, verify the corresponding SSE event type is broadcast and received by a test EventSource client.
8. **Audit trail verified:** If the feature involves a state mutation via HTTP, verify an audit event is recorded with the correct action string and target.
9. **Persistence verified:** If the feature creates new persistent data, verify it survives a server restart (write data, restart, read data back).
10. **Error handling verified:** Invalid inputs return appropriate HTTP status codes (400, 401, 403, 404, 422) with JSON error body `{ "error": "<message>" }`.

---

# Section 3: Security & Compliance

## 3.1 Security Review Protocol

### Authentication and Authorization Checklist

For each new endpoint or modified handler, verify:

1. **Bearer token validation:** Every endpoint except `/health` and `/v1/admin/login` calls `require_bearer()` or `authenticated_principal()` before processing the request body.
2. **Principal authorization:** Endpoints that modify resources check that the principal has permission (admin-only operations verify via `is_admin_principal()`, file operations verify owner match).
3. **Session expiry enforcement:** `principal_for_bearer()` purges expired sessions before lookup. New sessions have a maximum 12-hour TTL.
4. **Rate limiting on auth endpoints:** `authenticate_admin()` enforces 5-failure lockout per source IP with 15-minute cooldown. The lockout counter resets on successful login. Failed attempts older than 15 minutes are cleared.

### SIP Authentication Checklist

For each SIP method handler, verify:

1. **Digest challenge issuance:** Unauthenticated requests for protected methods (REGISTER, INVITE, BYE, CANCEL, INFO, MESSAGE, SUBSCRIBE, NOTIFY) receive a 401 response with `WWW-Authenticate: Digest realm="<domain>", nonce="<uuid>", algorithm=MD5, qop="auth"`.
2. **Nonce validation:** Each nonce is single-use (`consume_sip_nonce` removes it from the map) and expires after 5 minutes. Replayed nonces are rejected.
3. **Account lookup:** `sip_account(username, realm)` returns `None` for nonexistent accounts, and `is_authorized` returns false for disabled accounts.
4. **HA1 verification:** The server never stores plaintext passwords. The `password_ha1` field is `MD5(username:realm:password)`, computed on account creation and encrypted at rest.

### Input Validation Checklist

For each endpoint and SIP handler, verify:

1. **SIP URI parsing:** The `extract_sip_uri` function handles malformed URIs (missing `<>`, no `sip:` prefix, embedded parameters) without panicking. `split_sip_aor` returns `None` for URIs without `@`.
2. **Filename sanitization:** `safe_filename` strips path components (`../../etc/passwd` becomes `etc/passwd` becomes `passwd`). Empty filenames default to `"file"`.
3. **Content-Length enforcement:** SIP MESSAGE body is trimmed to the Content-Length header value via `trim_body_to_content_length`. Oversized HTTP bodies are rejected by `DefaultBodyLimit`.
4. **UUID parsing:** Path parameters parsed as `Uuid` return 422 Unprocessable Entity for invalid UUIDs (handled by axum's path extractor).
5. **JSON deserialization:** Malformed JSON request bodies return 400 Bad Request (handled by axum's `Json` extractor).
6. **Query parameter validation:** The `SseQuery` struct handles missing `token` field gracefully (Optional).

### Data-at-Rest Encryption Checklist

1. **Storage key derivation:** The `PALE_STORAGE_KEY` environment variable is hashed with SHA256 to derive a 256-bit ChaCha20-Poly1305 key. The raw key is never logged or exposed.
2. **Encrypted fields:** `SipAccount.password_ha1` is the only field encrypted at rest. The encryption format is `v1:<base64(nonce)>:<base64(ciphertext)>` where the nonce is 12 bytes from a UUID v4.
3. **Serialization safety:** `SipAccount.password_ha1` is annotated with `#[serde(skip_serializing)]`, preventing it from appearing in JSON API responses.
4. **Database file permissions:** The SQLite database is created at `{PALE_DATA_DIR}/pale-server.sqlite3`. The server should run with a dedicated user whose umask restricts file access to owner-only (0600).

### Data-in-Transit Encryption Checklist

1. **HTTP TLS:** When `PALE_HTTP_TLS_CERT` and `PALE_HTTP_TLS_KEY` are set, the HTTP server binds with rustls. A warning is logged if binding to a non-loopback address without TLS.
2. **SIP TLS:** When `PALE_SIP_TLS_CERT` and `PALE_SIP_TLS_KEY` are set, PJSIP creates a TLS transport. Client certificate verification is configurable via `PALE_SIP_TLS_VERIFY_CLIENT` and `PALE_SIP_TLS_REQUIRE_CLIENT_CERT`.
3. **SRTP:** Media encryption via SRTP is mandatory by default (`PALE_SIP_SRTP=true`). When enabled, `cfg.use_srtp = PJMEDIA_SRTP_MANDATORY` and `cfg.srtp_secure_signaling = 2` (require TLS for SRTP key exchange).
4. **CORS headers:** The `Access-Control-Allow-Origin` header is set only for whitelisted origins. Default origins: `http://localhost:1420`, `http://127.0.0.1:1420`, `tauri://localhost`. The `Vary: Origin` header is always set to prevent cache poisoning.

## 3.2 Threat Model: Server-Specific Attack Vectors

### T1: SIP Registration Hijacking

**Vector:** An attacker registers a contact address for a victim's AOR, causing calls/messages to be redirected to the attacker's endpoint.

**Mitigation:** Digest authentication is mandatory for REGISTER. Nonces are single-use with 5-minute expiry. The `consume_sip_nonce` function ensures replay attacks fail. HA1 verification checks against the stored account credential.

**Residual risk:** MD5-based digest auth is deprecated by RFC 8760. The server should migrate to SHA-256 digest auth when PJSIP and the rsip crate add support.

### T2: SIP Message Injection

**Vector:** An attacker sends crafted SIP messages to inject commands or manipulate dialog state.

**Mitigation:** All state-modifying SIP methods (REGISTER, INVITE, BYE, CANCEL, MESSAGE, SUBSCRIBE, NOTIFY, INFO) require digest authentication. Unauthenticated requests receive a 401 challenge. The `SipRequest::parse` function uses the rsip crate's parser which validates SIP message structure. Body content is trimmed to Content-Length to prevent trailing injection.

**Residual risk:** The UDP parser path (PALE_SIP_BACKEND=udp-parser) is explicitly marked insecure and gated behind `PALE_ALLOW_INSECURE_SIP_UDP=1`. It should never be used in production.

### T3: Bearer Token Theft via SSE Query Parameter

**Vector:** The SSE endpoint accepts the bearer token as a URL query parameter (`?token=`), which may be logged in HTTP access logs, proxy logs, or browser history.

**Mitigation:** Session tokens have a 12-hour TTL and are UUIDs (256 bits of entropy). The SSE endpoint is intended for internal use (Tauri app to local/private server). Production deployments should use HTTPS to prevent token interception in transit.

**Recommended hardening:** Add the `Cache-Control: private, no-store` header to the SSE response. Log sanitization rules should strip query parameters from request URLs containing `token=`.

### T4: File Upload Path Traversal

**Vector:** An attacker uploads a file with a crafted filename (e.g., `../../etc/shadow`) to write to arbitrary paths on the server.

**Mitigation:** The `safe_filename` function strips all path components using `Path::file_name()`, which returns only the final component. Files are stored at `{data_dir}/files/{uuid}` where the UUID is server-generated. The original filename is stored only in metadata (FileRecord), not used as the filesystem path.

### T5: Denial of Service via Resource Exhaustion

**Vector:** An attacker creates large numbers of users, accounts, registrations, or files to exhaust server memory.

**Mitigation:** Every ShardedMap collection has a hard capacity limit enforced by `trim_to_len()`:
- Users: 100,000
- SIP accounts: 100,000
- Registrations: 100,000
- Dialogs: 100,000
- Subscriptions: 100,000
- Nonces: 50,000
- Messages: 10,000
- Transactions: 20,000
- Notifications: 10,000
- Conferences: 50,000
- Calls: 100,000
- Files: 100,000
- Routing rules: 100,000
- Audit events: 50,000
- Admin sessions: 10,000
- Presence: 100,000

File uploads are limited by `PALE_MAX_UPLOAD_BYTES` (default 100MB). The SSE broadcast channel drops messages when the 256-entry buffer is full rather than blocking.

**Residual risk:** The trim operation evicts the oldest entries arbitrarily (first key found in shard), which could evict active entries under heavy load. A time-based eviction policy (LRU or TTL) would be more robust.

### T6: Admin Session Fixation

**Vector:** An attacker obtains a valid admin token (e.g., from a shared machine's sessionStorage) and uses it after the legitimate admin logs out.

**Mitigation:** Admin tokens are UUID v4 (128 bits of cryptographic randomness). The client stores the token in sessionStorage (not localStorage), which is cleared when the browser tab closes. The sign-out action removes the token from sessionStorage.

**Gap:** The server does not have an explicit session revocation endpoint. When the admin clicks "Sign out", the client deletes the token locally but the server-side session remains valid until the 12-hour TTL expires. GAP-10 (token refresh) partially addresses this by enabling token rotation, but an explicit `POST /v1/admin/logout` endpoint should be added that calls `admin_sessions.remove(&token)`.

**Recommended addition:** Add `POST /v1/admin/logout` endpoint:
```rust
async fn admin_logout(
    State(state): State<SharedState>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, ApiError> {
    let token = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .unwrap_or("");
    state.admin_sessions.remove(&token.to_string());
    Ok(Json(json!({ "ok": true })))
}
```

### T7: CORS Misconfiguration Escalation

**Vector:** If `PALE_ALLOW_CONFIGURABLE_CORS=1` is set, the `PALE_ALLOWED_ORIGINS` variable controls which origins can make cross-origin requests. A misconfigured wildcard (`*`) would allow any website to make authenticated API calls.

**Mitigation:** The `PALE_ALLOW_CONFIGURABLE_CORS` flag is disabled by default. When enabled, origins are matched exactly (no wildcards in origin values). The `origin_allowed` function performs strict string comparison.

**Recommended hardening:** Add a startup warning if `PALE_ALLOWED_ORIGINS` contains a wildcard origin or is set to `*`. Reject wildcard values entirely:
```rust
fn allowed_origins() -> Vec<String> {
    // ... existing code ...
    .filter(|value| value != "*")
    // ...
}
```

### T8: Encrypted Storage Key Weak Entropy

**Vector:** If `PALE_STORAGE_KEY` is a short or predictable string, the derived ChaCha20-Poly1305 key has insufficient entropy.

**Mitigation:** The `required_secret` function enforces a minimum length of 24 characters for `PALE_STORAGE_KEY`, `PALE_SERVER_TOKEN`, and `PALE_ADMIN_PASSWORD`. The key is derived via SHA256, which distributes entropy uniformly.

**Recommended hardening:** Log a warning at startup if `PALE_STORAGE_KEY` contains only ASCII letters/digits (low entropy per character). Recommend base64-encoded random bytes of at least 32 bytes.

## 3.3 Security Review Signoff Criteria

Before any gap feature is merged, the following security reviews are required:

1. **Input boundary review:** Every new function parameter that accepts user input (HTTP body, SIP header, query parameter) has been traced through parsing, validation, and storage. No raw user input is used in file paths, SQL queries, or shell commands.

2. **Authentication gate review:** Every new HTTP route has been verified to call `require_bearer` or `authenticated_principal` before any state mutation. Every new SIP handler for a protected method calls `is_authorized` with the sender's realm.

3. **Audit coverage review:** Every new state mutation (create, update, delete) records an audit event with the principal, action, and target. The action string follows the existing `entity.operation` convention (e.g., `call_history.synced`, `admin.token.refreshed`).

4. **Serialization review:** No new struct fields containing secrets (passwords, tokens, keys) are serializable without `#[serde(skip_serializing)]`. The `SensitiveString` wrapper pattern from http.rs is used for any deserialized password fields.

5. **Capacity limit review:** Every new ShardedMap or Vec collection has a defined maximum capacity and a `trim_to_len` call after insertion.

6. **TLS enforcement review:** No new network listeners bind to non-loopback addresses without either TLS configuration or an explicit warning log.
