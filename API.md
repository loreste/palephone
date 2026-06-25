# Pale Server API Reference

**Base URL:** `http://localhost:8080` (configurable via `PALE_HTTP_ADDR`)  
**Authentication:** Bearer token in `Authorization: Bearer <token>` header  
**Content-Type:** `application/json` (unless noted)

## Authentication

### POST /v1/admin/login
Authenticate and receive a session token (12-hour TTL).

**Request:**
```json
{ "username": "admin", "password": "your-password" }
```

**Response:**
```json
{ "token": "uuid-string", "principal": "admin", "expires_at": "2026-06-08T12:00:00Z" }
```

**Rate limit:** 5 failures per IP triggers 15-minute lockout.

### POST /v1/admin/refresh
Rotate the session token. Old token is invalidated.

**Headers:** `Authorization: Bearer <current-token>`

**Response:** New `AdminSession` with fresh 12-hour TTL.

### POST /v1/admin/logout
Revoke the current session.

**Headers:** `Authorization: Bearer <token>`

### GET /health
Health check. No authentication required.

**Response:**
```json
{ "ok": true, "service": "pale-server", "status": "healthy" }
```

Status is `"degraded"` when PostgreSQL circuit breaker is open.

### GET /metrics
Prometheus metrics in text format. No authentication required.

---

## Users

### GET /v1/users
List all users.

**Response:** `User[]`
```json
[{ "id": "uuid", "display_name": "Alice", "sip_uri": "sip:alice@example.com", "matrix_user_id": "@alice:matrix.org", "created_at": "..." }]
```

### POST /v1/users
Create a user.

**Request:**
```json
{ "display_name": "Alice", "sip_uri": "sip:alice@example.com", "matrix_user_id": "@alice:matrix.org" }
```

### DELETE /v1/users/{id}
Delete a user by UUID.

### PUT /v1/users/{id}/avatar
Upload user avatar image.

**Headers:** `Content-Type: image/png` (or image/jpeg)  
**Body:** Binary image data

**Response:**
```json
{ "file_id": "uuid", "url": "/v1/files/uuid" }
```

---

## SIP Accounts

### GET /v1/sip/accounts
List all SIP accounts. Password hashes are not included.

### POST /v1/sip/accounts
Create or upsert a SIP account.

**Request:**
```json
{ "username": "alice", "domain": "example.com", "password": "your-password", "display_name": "Alice" }
```

Password is stored as MD5 HA1 digest, never in plaintext.

### PUT /v1/sip/accounts/{username}/{domain}
Update account enabled status.

**Request:** `{ "enabled": false }`

### DELETE /v1/sip/accounts/{username}/{domain}
Delete a SIP account.

---

## SIP Monitoring

### GET /v1/sip/registrations
List active registrations (expired entries auto-filtered).

### GET /v1/sip/dialogs
List SIP dialogs with status, media types, and routing info.

### GET /v1/sip/messages
List SIP messages with pagination.

**Query params:**
- `limit` (default 100, max 500)
- `before` (ISO 8601 timestamp for cursor pagination)
- `room_id` (filter by from_uri or to_uri)

### GET /v1/sip/transactions
List SIP transaction history.

### GET /v1/sip/subscriptions
List active SUBSCRIBE subscriptions (expired auto-filtered).

### GET /v1/sip/notifications
List NOTIFY messages.

---

## Presence

### GET /v1/presence
List all user presence records.

**Response:** `UserPresence[]`
```json
[{ "sip_uri": "sip:alice@example.com", "status": "online", "note": "In a meeting", "updated_at": "..." }]
```

Status values: `online`, `offline`, `busy`, `away`, `dnd`

### GET /v1/presence/{sip_uri}
Get presence for a specific user. The `sip_uri` can omit the `sip:` prefix.

### PUT /v1/presence
Set your own presence.

**Request:**
```json
{ "status": "busy", "note": "In a meeting until 3pm" }
```

---

## Group Chat Rooms

### GET /v1/rooms
List rooms the authenticated user belongs to.

### POST /v1/rooms
Create a new group room.

**Request:**
```json
{ "name": "Engineering", "description": "Team channel", "members": ["sip:bob@example.com"] }
```

### GET /v1/rooms/{id}
Get room details including members.

### GET /v1/rooms/{id}/messages
List messages in a room.

**Query:** `limit` (default `100`, max `500`), `before` (RFC3339 timestamp)

### GET /v1/rooms/{id}/message-state
List per-message reactions and read receipts for a room.

### POST /v1/rooms/{id}/messages
Send a message to a room.

**Request:** `{ "body": "Hello team!" }`

### POST /v1/rooms/{id}/call
Start or join a room group call.

**Request:** `{ "mode": "audio" }`

### DELETE /v1/rooms/{id}/call
End the active room group call.

### POST /v1/rooms/{id}/members
Add a member to a room.

**Request:** `{ "user_sip_uri": "sip:charlie@example.com" }`

### DELETE /v1/rooms/{id}/members
Leave a room (removes the authenticated user).

---

## Messages

### PUT /v1/messages/{id}
Edit a message.

**Request:** `{ "body": "Updated text" }`

### DELETE /v1/messages/{id}
Delete a message.

### PUT /v1/messages/{id}/read
Mark a message as read. Broadcasts a `read_receipt` SSE event.

### POST /v1/messages/{id}/react
Toggle an emoji reaction for the authenticated user.

**Request:** `{ "emoji": "\ud83d\udc4d" }`

**Response:** `{ "message_id": "...", "room_id": "...", "emoji": "\ud83d\udc4d", "user_uri": "sip:bob@example.com", "added": true, "created_at": "..." }`

---

## Search

### GET /v1/search/messages
Full-text search across all messages.

**Query params:**
- `q` (required) — Search query
- `limit` (default 50, max 200)

**Response:** `SearchResult[]`
```json
[{ "id": "uuid", "source": "sip", "from_uri": "sip:alice@example.com", "body": "matching text", "timestamp": "...", "room_id": null }]
```

---

## Conferences

### GET /v1/conferences
List all conferences.

### POST /v1/conferences
Create a conference.

**Request:** `{ "title": "Standup", "mode": "audio" }`

Mode values: `audio`, `video`, `webinar`

### POST /v1/conferences/{id}/participants
Join a conference.

**Request:** `{ "user_id": "uuid", "sip_uri": "sip:alice@example.com", "role": "member" }`

### DELETE /v1/conferences/{id}/participants/{user_id}
Leave a conference.

---

## Calls

### GET /v1/calls
List all call sessions.

### POST /v1/calls
Create a call session.

**Request:**
```json
{ "caller": "sip:alice@example.com", "callees": ["sip:bob@example.com"], "media": ["audio"] }
```

### PUT /v1/calls/{id}/status
Update call status.

**Request:** `{ "status": "active" }`

Status values: `ringing`, `active`, `held`, `ended`, `failed`

---

## Call History

### GET /v1/call-history
Get call history for the authenticated user.

### POST /v1/call-history
Sync call history from client. Deduplicates by `(start_time, remote_uri, direction)`.

**Request:**
```json
{ "entries": [{ "direction": "outbound", "remote_uri": "sip:bob@example.com", "remote_name": "Bob", "start_time": "2026-06-07T10:00:00Z", "duration_secs": 120, "answered": true }] }
```

**Response:** `{ "merged": 1 }`

---

## Routing Rules

### GET /v1/routing/rules
List routing rules sorted by priority (ascending).

### POST /v1/routing/rules
Create a routing rule.

**Request:**
```json
{ "name": "VIP", "source_pattern": "sip:vip@example.com", "destination_pattern": "sip:*", "target": "sip:vip-desk@example.com", "priority": 10, "enabled": true }
```

### PUT /v1/routing/rules/{id}
Update a routing rule.

### DELETE /v1/routing/rules/{id}
Delete a routing rule.

---

## Files

### GET /v1/files
List files. Admins see all; regular users see only their own.

### POST /v1/files
Upload a file.

**Headers:**
- `Content-Type`: File MIME type
- `X-Pale-Filename`: Original filename
- `Authorization`: Bearer token

**Body:** Raw binary file data

**Response:**
```json
{ "id": "uuid", "owner": "admin", "filename": "report.pdf", "content_type": "application/pdf", "size": 1048576, "sha256": "hex-hash", "created_at": "..." }
```

Max size: configurable via `PALE_MAX_UPLOAD_BYTES` (default 100MB).

### GET /v1/files/{id}
Download a file. Returns binary with `Content-Disposition: attachment` header.

### DELETE /v1/files/{id}
Delete a file. Owner or admin only.

---

## Media Configuration

### GET /v1/media/config
Get ICE/STUN/TURN configuration.

**Response:**
```json
{ "ice_enabled": true, "stun_servers": ["stun:stun.l.google.com:19302"], "stun_ignore_failure": true, "turn": null }
```

---

## Voicemail

### GET /v1/voicemail
List voicemails for the authenticated user.

### PUT /v1/voicemail/{id}/listen
Mark a voicemail as listened.

### DELETE /v1/voicemail/{id}
Delete a voicemail.

---

## Call Recordings

### GET /v1/recordings
List call recordings for the authenticated user.

### DELETE /v1/recordings/{id}
Delete a recording.

---

## Audit Log

### GET /v1/admin/audit
List admin audit events (last 500, reverse chronological).

**Response:** `AuditEvent[]`
```json
[{ "id": "uuid", "principal": "admin", "action": "user.created", "target": "uuid", "created_at": "..." }]
```

---

## Server-Sent Events

### GET /v1/events
SSE stream for real-time updates.

**Authentication:** Bearer token via header OR `?token=<token>` query param.

**Event types:**
| Event | Payload | Trigger |
|-------|---------|---------|
| `presence` | `UserPresence` | User status change or registration |
| `message` | `SipMessage` | New SIP MESSAGE received |
| `room_message` | `RoomMessage` | New room message sent |
| `notification` | `SipNotification` | SIP NOTIFY received |
| `voicemail` | `Voicemail` | New voicemail deposited |
| `recording` | `CallRecording` | Call recording completed |
| `read_receipt` | `{ message_id, room_id, reader_uri, read_at }` | Message marked as read |
| `message_edited` | `{ message_id, new_body, edited_by, edited_at }` | Message edited |
| `message_deleted` | `{ message_id, deleted_by, deleted_at }` | Message deleted |
| `reaction` | `{ message_id, room_id, emoji, user, added, created_at }` | Reaction toggled |
| `room_call_started` | `{ room_id, conference_id, call_uri, mode }` | Room group call started |
| `room_call_ended` | `{ room_id, conference_id, call_uri }` | Room group call ended |

**Keep-alive:** Default interval. Buffer: 256 messages (older dropped if client lags).

---

## Error Responses

All errors return JSON:
```json
{ "error": "human-readable message" }
```

| Status | Meaning |
|--------|---------|
| 400 | Bad Request — invalid input |
| 401 | Unauthorized — missing or invalid token |
| 403 | Forbidden — insufficient permissions |
| 404 | Not Found — resource doesn't exist |
| 413 | Payload Too Large — file exceeds limit |
| 429 | Too Many Requests — rate limit exceeded |
| 500 | Internal Server Error |

---

## Environment Variables

| Variable | Required | Default | Description |
|----------|----------|---------|-------------|
| `PALE_SERVER_TOKEN` | Yes | — | Static bearer token (min 24 chars) |
| `PALE_ADMIN_PASSWORD` | Yes | — | Admin password (min 24 chars) |
| `PALE_STORAGE_KEY` | Yes | — | SQLite encryption key (min 24 chars) |
| `PALE_DATABASE_URL` | No | — | PostgreSQL connection string |
| `PALE_HTTP_ADDR` | No | `127.0.0.1:8080` | HTTP listen address |
| `PALE_SIP_ADDR` | No | `0.0.0.0:5060` | SIP listen address |
| `PALE_DATA_DIR` | No | `./pale-data` | Data directory |
| `PALE_ADMIN_USERNAME` | No | `admin` | Admin login username |
| `PALE_RATE_LIMIT_RPS` | No | `100` | Requests per second per user |
| `PALE_MAX_UPLOAD_BYTES` | No | `104857600` | Max file upload size |
| `PALE_LOG_JSON` | No | `false` | Enable JSON structured logging |
| `PALE_SIP_BACKEND` | No | `pjsip` | SIP backend (pjsip or udp-parser) |
| `PALE_SIP_SRTP` | No | `true` | Require SRTP encryption |
| `PALE_ICE` | No | `true` | Enable ICE |
| `PALE_STUN_SERVERS` | No | — | Comma-separated STUN servers |
| `PALE_TURN_SERVER` | No | — | TURN server address |
| `PALE_PG_MAX_CONNECTIONS` | No | `10` | PostgreSQL pool size |
