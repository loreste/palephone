# Next Steps — Production Readiness Handoff

## 0. Enterprise Security Features — LANDED (2026-07-03)

The following Microsoft Teams enterprise parity features shipped:

- **MFA / TOTP** (2026-07-03): TOTP-based multi-factor authentication with
  `totp_secrets` table (migration 024), setup/verify/validate/disable endpoints
  (`/v1/mfa/*`), backup codes, and a Security tab in SettingsView with QR
  provisioning URI display and code verification.
- **Session management** (2026-07-03): Concurrent session tracking via
  `user_sessions` table (migration 024), list/revoke/revoke-all endpoints
  (`/v1/sessions/*`), device name/type/IP tracking, and an Active Sessions
  section in the Security settings tab with per-session revoke buttons.
- **Certificate-based authentication** (2026-07-03): Server config fields
  `ca_cert_path` and `verify_client_certs` (`PALE_CA_CERT_PATH`,
  `PALE_VERIFY_CLIENT_CERTS` env vars), plus `extract_cert_identity` helper
  for mapping client certificate CN/SAN to SIP user identity. SIP TLS path
  already supported `PALE_SIP_TLS_VERIFY_CLIENT` and `PALE_SIP_TLS_CA_FILE`.

Produced by a five-team review (SIP/RFC compliance, architecture, UI/UX, API
contract, QA) with adversarial cross-verification: 25 confirmed findings,
0 refuted. The security, UX-trust, and provisioning fixes landed in commit
67887bf. This document preserves the remaining work.

## 1. SIP stack: in-dialog relay foundation — LANDED

The in-dialog relay foundation shipped (see `git log -- src-tauri/crates/pale-server/src/sip.rs`):
dialog peer addressing (from_contact/from_source/to_source), generic
`relay_in_dialog_request` (INFO/DTMF, BYE, UPDATE, re-INVITE, REFER, ACK),
full provisional+final response relay in `proxy_invite` with a Timer-C
deadline, CANCEL matched by top-Via branch and forwarded upstream (never
digest-challenged, per RFC 3261 §22.1), hop-by-hop ACK for non-2xx finals,
Max-Forwards decrement/insertion with 483, deterministic To-tags on non-100
responses, 481 for unknown-dialog in-dialog requests, BYE CDR disposition
from prior dialog state, REFER without illegal Subscription-State + RFC 4488
Refer-Sub honoring + terminal NOTIFY sipfrag, and the MESSAGE nonce
double-consumption fix.

Still open from the original SIP review (lower priority):
- 2xx-to-INVITE responses from server-terminated paths (voicemail/queue/IVR)
  still lack Contact + SDP answers — they need a media engine or honest 480s.
- 407 Proxy-Authenticate on proxied paths (401 is used everywhere today).
- Digest uri compared by strict string equality against rsip re-serialization.
- Transaction-scoped nonce reuse (stale=true) for retransmissions.

## 2. Architectural decision required: the registrar

VERDICT ON THE PRIOR REVIEWER'S CLAIM: verified on every substantive clause; I challenged it and found only two imprecisions. (1) Correct — the default backend (PALE_SIP_BACKEND unset => "pjsip", main.rs:383-392) builds a bare pjsua-lib UA endpoint: pjsua_create/init/start with UDP (and optional TCP/TLS) transports, zero accounts, and no module that implements registrar semantics; incoming REGISTERs are rejected by pjsua's default handler (405), since pjsua's account API only performs outbound client registration. (2) Correct — on_incoming_call (pjsip_runtime.rs:332) sends only 180 Ringing; the crate contains no 200-answer and no pjsua_call_make_call, so no call is ever routed or connected. (3) Correct in substance — on_call_media_state connects the call's conf slot to slot 0 (the sound-device master port) bidirectionally; imprecision: it's the *default* sound device, not explicitly the null device (pjsua_set_null_snd_dev is never called; null is only the headless fallback), and the callback is effectively dead code because inbound calls never reach MEDIA_ACTIVE without a 200/SDP answer. (4) Correct — two softphones cannot register to or call each other through the default backend; the pjsip backend is a telemetry shell (OPTIONS 200, dialog rows into AppState for the dashboard). (5) Correct with one correction — all registrar/proxy/PBX logic (REGISTER bindings, digest auth, INVITE 302 routing + one-shot UDP proxy, DND, call forwarding, MESSAGE relay, REFER transfer/park, SUBSCRIBE/NOTIFY) lives solely in sip.rs, which is UDP-only and refuses to start without PALE_ALLOW_INSECURE_SIP_UDP=1 (sip.rs:24-29); however it is not unauthenticated — every method enforces MD5 digest auth. Compounding bugs: main.rs spawns the parser non-fatally (SIP silently absent on gate failure while HTTP keeps serving), and login provisioning (lib.rs:917) hands every client registrar_uri=sip:{server} regardless of backend, so the shipped first-login auto-register flow fails against the default. The client itself is registrar-agnostic (register_account accepts any registrar_uri/transport; SettingsView exposes it as a free field), so third-party SIP servers work — but the documented "complete phone system, no Asterisk needed" promise (README line 7) hinges on the server registrar, which the default backend does not provide. CRITICAL severity is warranted. ARCHITECTURE RECOMMENDATION: neither (a) nor (b) as posed is the best path. Option (a) — hardening tokio sip.rs with TCP/TLS — means hand-building a SIP transaction state machine, retransmission handling, and TLS transport on top of a string parser; highest effort and risk. Option (b) — pjsua for media only — keeps a library that structurally cannot be a registrar and currently contributes nothing but transports. The honest recommendation is (c): drop pjsua-lib server-side and build the registrar/proxy as a custom module on the *lower-level pjsip endpoint API* that pjsip-sys already vendors (the pjsip-apps/samples/proxy.c pattern): this gets RFC 3261 parsing, transactions, and UDP/TCP/TLS transports from the same battle-tested stack the client uses, while reusing sip.rs's existing auth (sip_ha1 digest), location store (upsert_registration), and routing logic (DND/forwarding/302) nearly unchanged. Pragmatic alternative if FFI surface is unwanted: compose Kamailio/OpenSIPS as the registrar/proxy in docker-compose (coturn already establishes this pattern) with pale-server reduced to HTTP provisioning/API/CDR, syncing accounts via Kamailio's DB tables in the existing PostgreSQL. Interim must-fixes either way: make backend capability part of the provisioning contract (don't advertise sip_registrar from the pjsip backend), make SIP listener startup failure fatal, and correct README/ARCHITECTURE_V2 claims about TLS-secured registrar until one exists.

## 3. Lower-severity backlog (22 items)

- **[sip-rfc/medium]** Max-Forwards is never checked or decremented on any forwarded request; relay_message drops it entirely — proxy loops are unbounded
  - File: /Users/loreste/palephone/src-tauri/crates/pale-server/src/sip.rs:975-1001 and 848-867
  - Fix: In rewrite_invite_for_proxy (and the generalized rewrite_request_for_proxy from finding 3): while copying lines, detect `max-forwards:` case-insensitively, parse the value; if 0, abort the forward and return 483 Too Many Hops to the sender; otherwise write `Max-Forwards: {n-1}`. If no Max-Forwards line existed, insert `Max-Forwards: 69`. In relay_message, add a `Max-Forwards: {orig-1, default 69}`
- **[sip-rfc/medium]** Dialog state machine is wrong: re-INVITE detection matches any prior Call-ID (even Ended dialogs), ACK sets status to Ringing, BYE records CDR disposition 'answered' unconditionally
  - File: /Users/loreste/palephone/src-tauri/crates/pale-server/src/sip.rs:164-183, 471-482, 491-494
  - Fix: Re-INVITE detection: require (a) the request's To header to carry a tag AND (b) the stored dialog status to be a live one (Ringing/Answered/Held), i.e. `state.dialog_for(call_id).map(|d| !matches!(d.status, Ended|Cancelled)).unwrap_or(false) && request.header("to").map(|t| t.contains("tag=")).unwrap_or(false)`. A to-tag-less INVITE with a stale Call-ID should take the normal initial-INVITE path; a
- **[sip-rfc/low]** Digest uri parameter compared by strict string equality against rsip's re-serialized request-URI; proxy paths challenge with 401 instead of 407
  - File: /Users/loreste/palephone/src-tauri/crates/pale-server/src/sip.rs:1169 and 1147-1160
  - Fix: Replace the strict compare with component-wise comparison: parse both auth.uri and the request URI via rsip::Uri and compare scheme + user + lowercased host + effective port (default 5060), ignoring URI parameters; reject only on host/user mismatch. Keep using auth.uri (the client's exact string) in the HA2 computation as today. For the proxy-forwarding code paths (proxy_invite, relay_message, fut
- **[sip-arch/medium]** on_call_media_state bridges remote audio to conf slot 0 (server sound device), not to another endpoint — and is dead code for inbound calls
  - File: /Users/loreste/palephone/src-tauri/crates/pale-server/src/pjsip_runtime.rs:362-374
  - Fix: Delete the slot-0 bridge. If a media path is ever needed server-side (conference, voicemail, MoH), call pjsua_set_null_snd_dev() at init and connect call conf slots to each other or to a pjmedia player/recorder port, never to slot 0 on a headless server.
- **[sip-arch/medium]** Documentation advertises capabilities (SIP TLS 5061 + registrar together) that no single backend provides
  - File: /Users/loreste/palephone/README.md:7, README.md (server section: 'SIP UDP on 5060, SIP TLS on 5061', crate table line 139)
  - Fix: Until the unified backend exists, document the two backends honestly: PALE_SIP_BACKEND=pjsip = transport/ICE shell without registrar (dev/telemetry); PALE_SIP_BACKEND=udp-parser + PALE_ALLOW_INSECURE_SIP_UDP=1 = functional registrar/PBX, UDP-only, lab use. Remove or caveat the 'complete phone system, no Asterisk needed' claim until finding 1 lands.
- **[uiux/medium]** Call events clobber user-chosen presence: DND/Away silently reset to 'online' after every call
  - File: /Users/loreste/palephone/src/hooks/useSipEvents.ts:101-116
  - Fix: Store the user's manually selected status (e.g. presenceStore.manualStatus set by StatusBar.handleSelect). On call connect, only set on_call if manualStatus is online/away; on terminate, restore manualStatus instead of hardcoded "online". Also update the local presenceStore from the PUT response like StatusBar does, so the StatusBar dot reflects on_call during calls.
- **[uiux/medium]** 60 silent .catch(() => {}) swallow failures of core actions; optimistic mute/hold/delete never roll back
  - File: /Users/loreste/palephone/src/components/call/ActiveCallView.tsx:25-38 (pattern repeats in ~15 files)
  - Fix: Establish a rule: every user-initiated IPC gets (a) rollback of optimistic store change in catch, and (b) toast({type:"error"}). Wrap in a helper: `async function callIpc(action, rollback, errTitle)` used by all call controls. For chat delete, remove the message from chatStore on success and toast on failure.
- **[uiux/medium]** Decoy buttons shipped with no handler: Settings Cancel, chat Attach file, StatusBar Audio settings, Files Download
  - File: /Users/loreste/palephone/src/components/settings/SettingsView.tsx:179-186 (and 3 others)
  - Fix: Cancel: reset form to current account values (setForm from useAccountStore) or remove the button. Paperclip: open a file picker (Tauri dialog plugin to get a real path) then matrixSendFile/paleServerUploadFile for the active room. Volume2: setActiveTab("settings") + deep-link to the audio tab (lift SettingsTab into uiStore so CommandPalette's "Audio Settings"/"Server Settings" commands — which cur
- **[uiux/medium]** Recent calls list is a dead end: no tap-to-redial, and merged server records all share key/id -1
  - File: /Users/loreste/palephone/src/components/recent/RecentCallsList.tsx:32,149,183-189
  - Fix: Key by a composite (`${call.start_time}-${call.remote_uri}-${call.direction}`), hide the local-delete button for server records (id < 0), and make the row a button that calls makeCall(call.remote_uri) (or prefills the dialer). Also add a confirm step to the unguarded "Clear All" destructive action (line 101).
- **[uiux/medium]** Mixed server transport: raw fetch() in CallSettings/reactions/health bypasses the mandated Tauri proxy, leaving permanent 'Loading...' dead-ends
  - File: /Users/loreste/palephone/src/components/settings/SettingsView.tsx:459-463,515
  - Fix: Replace all raw fetch calls with paleServerApi()/serverFetch, add an error state to CallSettingsPanel (`{error ? <ErrorRetry onRetry={load}/> : !settings ? <Spinner/> : ...}`). For reactions, either render reaction counts on MessageBubble (needs a store field + SSE event) or remove the quick-reaction buttons until the loop is closed.
- **[uiux/low]** Overlays lack dialog semantics and focus management; CommandPalette exposes Admin Panel to non-admins
  - File: /Users/loreste/palephone/src/components/call/IncomingCallOverlay.tsx:51-63 (also CommandPalette, DtmfOverlay, SearchOverlay)
  - Fix: Give each overlay role="dialog" aria-modal="true" + a small useFocusTrap hook (focus first action on open, restore previous activeElement on close); autofocus the Accept button in IncomingCallOverlay. Filter the admin command by `useServerStore.getState().userRole === "admin"` like BottomNav. Add aria-label="Dismiss" to the toast close button. Longer term, adopt the ui/Button primitive (currently 
- **[contract/medium]** Edit/delete/react endpoints are broadcast-only no-ops, and frontend ignores the resulting SSE events
  - File: /Users/loreste/palephone/src-tauri/crates/pale-server/src/http.rs:959-1020 vs /Users/loreste/palephone/src/hooks/useServerEvents.ts:40-97
  - Fix: Two-sided: (1) server must actually mutate stored messages (remove from `sip_messages`/`room_messages`, persist via pg_spawn) before broadcasting; (2) frontend useServerEvents.ts needs `es.addEventListener("message_deleted"|"message_edited"|"reaction", ...)` handlers that update chatStore (add a removeMessage/editMessage action).
- **[contract/medium]** matrix://transfer-progress, matrix://verification, matrix://sync-error emitted but no frontend listener exists
  - File: /Users/loreste/palephone/src-tauri/src/lib.rs:695-697 vs /Users/loreste/palephone/src/lib/tauri.ts:249-263 (only auth-state/rooms/message/typing wrapped)
  - Fix: Add typed wrappers in tauri.ts (`onMatrixTransferProgress`, `onMatrixVerification`, `onMatrixSyncError`) mirroring the existing onMatrix* helpers, subscribe in useMatrixEvents.ts, and wire payloads to fileStore.addTransfer/updateTransfer and a verification store that opens VerificationDialog.
- **[contract/medium]** Default server URL disagreement: frontend SetupWizard uses :8090, server binds :8080 by default
  - File: /Users/loreste/palephone/src/components/auth/SetupWizard.tsx:53 vs /Users/loreste/palephone/src-tauri/crates/pale-server/src/main.rs:239
  - Fix: Standardize on 8080: change SetupWizard.tsx:53 default to `http://localhost:8080` and fix the advertised port in the http.rs root() HTML. (Or change the server default to 8090 everywhere — but three of the four sites already say 8080.)
- **[contract/medium]** ChatView pagination filters /v1/sip/messages by room_id that the server compares against from_uri/to_uri
  - File: /Users/loreste/palephone/src/components/chat/ChatView.tsx:243-247 vs /Users/loreste/palephone/src-tauri/crates/pale-server/src/http.rs:433-435
  - Fix: For server rooms, ChatView.handleScroll should call `paleServerGetRoomMessages` (already defined in tauri.ts:504-510) instead of paleServerGetMessages; for SIP DM threads, pass the peer's sip URI (room counterpart URI), not the room ID. Server-side, list_room_messages should gain `before`/`limit` params to support pagination.
- **[contract/medium]** Server-room typing contract dead on both ends
  - File: /Users/loreste/palephone/src-tauri/crates/pale-server/src/http.rs:76,856-872 vs /Users/loreste/palephone/src/hooks/useServerEvents.ts (no "typing" listener) and /Users/loreste/palephone/src/components/chat/ChatView.tsx:8 (only matrixSetTyping imported)
  - Fix: In ChatView, when the active room is a server room, POST `{typing}` to `/v1/rooms/{id}/typing` via paleServerApi; in useServerEvents add `es.addEventListener("typing", ...)` that accumulates/removes `payload.user` into the room's user list before calling setTypingUsers (or change the server payload to `user_ids: []` to match the Matrix shape the store already consumes).
- **[contract/low]** AdminPresence status union missing "on_call" variant the server emits
  - File: /Users/loreste/palephone/src/lib/adminApi.ts:185 vs /Users/loreste/palephone/src-tauri/crates/pale-server/src/lib.rs:2995-3003
  - Fix: Change AdminPresence.status in adminApi.ts to reuse the canonical `PresenceStatus` type exported from tauri.ts:346 (which already includes "on_call"), and audit AdminView presence rendering for an on_call branch.
- **[contract/low]** CORS allowlist omits the Windows Tauri webview origin, breaking all raw fetch() paths
  - File: /Users/loreste/palephone/src-tauri/crates/pale-server/src/http.rs:186-191 vs /Users/loreste/palephone/src/lib/adminApi.ts:388-395 and /Users/loreste/palephone/src/lib/tauri.ts:659
  - Fix: Add `"http://tauri.localhost"` (and `"https://tauri.localhost"`) to the default vec in allowed_origins(), or migrate the remaining raw fetch()/EventSource call sites to the Rust-side pale_server_request proxy as tauri.ts:583-585 already recommends.
- **[contract/low]** SSE listeners registered as empty no-op handlers mask unimplemented features
  - File: /Users/loreste/palephone/src/hooks/useServerEvents.ts:86-92 vs /Users/loreste/palephone/src-tauri/crates/pale-server/src/lib.rs:2595-2598, http.rs:941-948
  - Fix: Either implement the handlers (recordings list refresh, read-receipt badges on messages) or delete the dead listeners so the gap is visible; for agent/queue events, wire them in the admin wallboard view if call-center UI is in scope, otherwise document them as server-to-server only.
- **[contract/low]** matrixGetRooms and Matrix event payloads typed as any/unknown — drift between RoomSummary definitions
  - File: /Users/loreste/palephone/src/lib/tauri.ts:224-226,249-263 vs /Users/loreste/palephone/src-tauri/crates/pale-matrix/src/types.rs:23-34
  - Fix: Type matrixGetRooms as Promise<RoomSummary[]> using the chatStore types, add avatar_url to the TS RoomSummary, and define MatrixAuthEvent/MatrixRoomsEvent/MatrixTypingEvent interfaces once in tauri.ts (they already exist privately in useMatrixEvents.ts) so the listener wrappers are generically typed instead of unknown.
- **[qa/medium]** IncomingCallOverlay optimistically marks the call connected before answer_call resolves; zero tests for IncomingCallOverlay/ActiveCallView/useSipEvents
  - File: /Users/loreste/palephone/src/components/call/IncomingCallOverlay.tsx:13-36
  - Fix: First test: vitest + @testing-library/react — render IncomingCallOverlay with a mocked callStore holding an incomingCall and `vi.mock("@/lib/tauri")` where answerCall rejects; click Accept; assert toast fired, removeSession called, and activeCallId reset to null (this pins the current rollback contract). Then refactor handleAccept to set state "connecting" and only flip to "connected" on the call-
- **[qa/medium]** storage.rs ChaCha20Poly1305 encrypt/decrypt and pg_store.rs (778 lines) have zero tests — silent data-loss/corruption path for persisted secrets
  - File: /Users/loreste/palephone/src-tauri/crates/pale-server/src/storage.rs:167-215 (no mod tests in file); /Users/loreste/palephone/src-tauri/crates/pale-server/src/pg_store.rs (no tests)
  - Fix: First test: in storage.rs add `mod tests` with `fn encrypt_decrypt_roundtrip()` (construct the store with a fixed key, assert decrypt(encrypt(s)) == s) and `fn decrypt_rejects_tampered_ciphertext()` (flip one byte, assert Err not panic/garbage). For pg_store, add a #[ignore]-by-default tokio test gated on PALE_TEST_PG_URL that roundtrips upsert_registration, runnable in CI against the compose post

## 4. Teams Enterprise Parity — Admin/Governance (LANDED 2026-07-03)

- [x] **Information barriers** — `information_barriers` table (migration 024), CRUD endpoints (POST/GET/PUT/DELETE /v1/admin/barriers), enforcement check function (GET /v1/admin/barriers/check), "Barriers" tab in AdminView.tsx
- [x] **Sensitivity labels** — `sensitivity_labels` table (migration 025), `sensitivity_label_id` added to files and room_messages, CRUD endpoints (/v1/admin/labels), "Labels" tab in AdminView.tsx with color swatches and property toggles
- [x] **Custom RBAC roles** — `custom_roles` table (migration 026), `role_id` on users, CRUD endpoints (/v1/admin/roles), 14 permission constants, GET /v1/admin/roles/permissions, "Roles" tab with permission checkboxes
- [x] **Policy packages** — `policy_packages` table (migration 027), CRUD endpoints (/v1/admin/policy-packages), POST /v1/admin/policy-packages/{id}/assign, "Packages" tab in AdminView.tsx
- [x] **Bulk user operations** — POST /v1/admin/users/import (CSV), GET /v1/admin/users/export (CSV download), import/export buttons in Analytics tab
- [x] **Usage analytics dashboard** — GET /v1/admin/analytics (active users, messages, calls, meetings, files, storage), "Analytics" tab with metric cards
- [x] **Meeting attendance CSV export** — GET `/v1/conferences/{id}/attendance/export?format=csv`, "Export CSV" button in MeetingPanel attendance section
- [x] **Meeting templates** — Admin-configurable meeting defaults (migration 024), CRUD at `/v1/admin/meeting-templates`, "Meeting Templates" tab in AdminView
- [x] **Spotlight** — Organizer pins a participant's video for all via POST `/v1/conferences/{id}/spotlight`, SSE `spotlight_changed`, spotlight star button on participants
- [x] **Live animated reactions** — POST `/v1/conferences/{id}/reactions`, SSE `meeting_reaction`, reaction bar with 8 emoji buttons + floating reaction overlay
- [x] **Persistent meeting chat** — `chat_room_id` on conferences (migration 025), auto-created chat room linked to meeting, "Chat" tab in MeetingPanel
- [x] **Green room / presenter staging** — `green_room_enabled` on conferences, join/ready endpoints, SSE `green_room_updated`, "Green Room" tab in MeetingPanel
- [x] **Out-of-office auto-reply** — `out_of_office_message`/`out_of_office_until` on users (migration 025), GET/PUT `/v1/users/out-of-office`, "Out of Office" tab in Settings
- [x] **File versioning** — Track version history for uploaded files (2026-07-03)
- [x] **Folder structure per channel** — Organize files in directories (2026-07-03)
- [x] **File locking / checkout** — Prevent concurrent edits (2026-07-03)
- [x] **Approvals workflow** — Request and track approvals (2026-07-03)
- [x] **Policy-based compliance recording** — Auto-record based on policies (2026-07-03)
- [x] **Configurable music on hold** — Custom hold music (2026-07-03)
- [x] **Per-user call analytics dashboard** — Individual call quality view (2026-07-03)
- [x] **User-configurable personal call groups** — Ring multiple devices/numbers (2026-07-03)
- [x] **Chat density toggle** — Compact vs comfortable vs spacious view (2026-07-03)

## 5. Microsoft Teams enterprise parity — Chat/Messaging

- [x] **Scheduled send** (2026-07-03) — `scheduled_at`/`delivered` columns on `room_messages`, `POST /v1/rooms/{id}/messages/schedule` endpoint, background task every 30s delivers due messages, SSE `scheduled_message_delivered` event, datetime picker + clock button in ChatView compose bar.
- [x] **Message delivery/failure status** (2026-07-03) — `delivery_status` column on `room_messages` (pending/sent/delivered/failed), status indicators (checkmarks, clock, warning) in message bubbles, SSE events carry delivery_status field.
- [x] **Tags for targeted communication** (2026-07-03) — `tags` table (id, team_id, name, members), CRUD endpoints at `/v1/teams/{id}/tags`, `@tag` mention resolution in messages that notifies all tag members, tag suggestions in mention autocomplete dropdown.
- [x] **GIF integration** (2026-07-03) — `GET /v1/gif/search?q=...` proxy endpoint (Tenor/Giphy, key via `PALE_TENOR_API_KEY` or `PALE_GIPHY_API_KEY`), GIF picker panel in ChatView compose bar with search + grid selection, sends as markdown image.
- [x] **Per-channel notification granularity** (2026-07-03) — `notification_preferences` table (room_id, user_uri, notification_level), `GET/PUT /v1/rooms/{id}/notifications` endpoints, notification level dropdown (all/mentions-only/muted) in ChatView room header.
