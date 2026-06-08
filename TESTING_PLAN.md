# Pale Platform — Cross-Functional Testing Plan

**Version:** 1.0  
**Date:** 2026-06-07  
**Prepared for:** Engineering, QA, UI/UX, and DBA teams  
**Scope:** Full application validation before production release

---

## Team Assignments

| Team | Lead Responsibility | Key Deliverables |
|------|-------------------|------------------|
| **Engineering** | Backend integration testing, SIP protocol validation, build verification | Green CI pipeline, all 62 tests passing, Docker image builds |
| **QA** | End-to-end test execution, regression testing, defect tracking | Test execution reports, defect log, sign-off |
| **UI/UX** | Visual audit, accessibility review, interaction testing, mobile validation | A11y report, responsive audit, UX sign-off |
| **DBA** | Schema review, query performance, backup validation, data integrity | Schema sign-off, query plan review, backup/restore test |

---

# Section 1: Engineering Testing

## 1.1 Prerequisites

### Environment Setup

**Local development:**
```bash
# Clone and install
git clone https://github.com/loreste/palephone.git
cd palephone
npm install

# Start PostgreSQL
docker compose up postgres -d

# Set required environment variables
export PALE_SERVER_TOKEN=$(openssl rand -base64 32)
export PALE_ADMIN_PASSWORD=$(openssl rand -base64 32)
export PALE_STORAGE_KEY=$(openssl rand -base64 32)
export PALE_DATABASE_URL="host=localhost user=pale password=$POSTGRES_PASSWORD dbname=pale"

# Build and run pale-server
cd src-tauri
cargo run -p pale-server --bin pale-server
```

**Verify server is running:**
```bash
curl http://localhost:8080/health
# Expected: {"ok":true,"service":"pale-server","status":"healthy"}
```

### Run Automated Tests

```bash
# Rust unit tests (32 tests)
cd src-tauri && cargo test -p pale-server

# Frontend unit tests (30 tests)
cd .. && npx vitest run

# TypeScript type check
npx tsc --noEmit
```

**Pass criteria:** All 62 tests pass, zero TypeScript errors.

## 1.2 SIP Protocol Test Matrix

Execute each test with a SIP client (e.g., Ooh323c, Ooh323ctest, or PJSUA CLI tool) pointed at the pale-server SIP address.

| Test ID | SIP Method | Scenario | Expected Result | Verify |
|---------|-----------|----------|-----------------|--------|
| SIP-01 | REGISTER | Valid credentials | 200 OK, registration stored | `GET /v1/sip/registrations` shows entry |
| SIP-02 | REGISTER | No Authorization header | 401 with WWW-Authenticate Digest challenge | Response contains realm, nonce, algorithm |
| SIP-03 | REGISTER | Wrong password | 401 Unauthorized | Audit log shows `admin.login.failed` |
| SIP-04 | REGISTER | Expires: 0 | 200 OK, registration removed | `GET /v1/sip/registrations` empty |
| SIP-05 | REGISTER | Compact headers (f/t/i/m) | 200 OK | Same behavior as full headers |
| SIP-06 | INVITE | To registered user | 302 Moved Temporarily with Contact | Dialog created in `GET /v1/sip/dialogs` |
| SIP-07 | INVITE | To unregistered user | 480 Temporarily Unavailable | Dialog status is "failed" |
| SIP-08 | INVITE | To conference URI (sip:conf-{uuid}@domain) | 200 OK if active, 480 if not | Dialog created |
| SIP-09 | INVITE | Re-INVITE with a=sendonly | 200 OK | Dialog status changes to "held" |
| SIP-10 | INVITE | Re-INVITE with a=sendrecv | 200 OK | Dialog status changes to "ringing" |
| SIP-11 | INVITE | With m=audio + m=video SDP | 302 redirect | Dialog has media_types ["audio","video"] |
| SIP-12 | BYE | Authenticated | 200 OK | Dialog status changes to "ended" |
| SIP-13 | BYE | No auth | 401 Unauthorized | Dialog unchanged |
| SIP-14 | CANCEL | Authenticated | 200 OK | Dialog status changes to "cancelled" |
| SIP-15 | MESSAGE | Authenticated | 202 Accepted | Message stored in `GET /v1/sip/messages` |
| SIP-16 | MESSAGE | To registered user (UDP) | 202 Accepted | Message relayed to target |
| SIP-17 | SUBSCRIBE | Event: presence | 200 OK with Expires | Subscription in `GET /v1/sip/subscriptions` |
| SIP-18 | SUBSCRIBE | Event: unknown | 489 Bad Event | Allow-Events header lists supported events |
| SIP-19 | SUBSCRIBE | Expires: 0 | 200 OK | Subscription removed |
| SIP-20 | NOTIFY | With PIDF open body | 200 OK | Presence updated to "online" |
| SIP-21 | NOTIFY | With PIDF closed body | 200 OK | Presence updated to "offline" |
| SIP-22 | REFER | With Refer-To header | 202 Accepted | Original dialog ended, audit logged |
| SIP-23 | OPTIONS | No auth required | 200 OK with Allow header | Lists all supported methods |
| SIP-24 | INFO | Authenticated | 200 OK | Transaction recorded |

## 1.3 HTTP API Test Matrix

Use `curl` or Postman against each endpoint. First obtain a token:

```bash
TOKEN=$(curl -s -X POST http://localhost:8080/v1/admin/login \
  -H "Content-Type: application/json" \
  -d '{"username":"admin","password":"YOUR_PASSWORD"}' | jq -r '.token')
```

| Test ID | Method | Path | Test | Expected |
|---------|--------|------|------|----------|
| API-01 | POST | /v1/admin/login | Valid credentials | 200, token returned |
| API-02 | POST | /v1/admin/login | Wrong password x5 | 429 Too Many Requests on 6th attempt |
| API-03 | POST | /v1/admin/refresh | Valid token | 200, new token, old token invalidated |
| API-04 | POST | /v1/admin/logout | Valid token | 200, token no longer works |
| API-05 | POST | /v1/users | Create user | 201, user in GET /v1/users |
| API-06 | DELETE | /v1/users/{id} | Delete user | 200, user removed |
| API-07 | POST | /v1/sip/accounts | Create account | 200, account listed |
| API-08 | PUT | /v1/sip/accounts/{u}/{d} | Disable account | 200, enabled=false |
| API-09 | POST | /v1/routing/rules | Create rule | 200, rule in sorted list |
| API-10 | POST | /v1/conferences | Create conference | 200, conference listed |
| API-11 | POST | /v1/conferences/{id}/participants | Join conference | 200, participant count +1 |
| API-12 | POST | /v1/calls | Create call | 200, call listed |
| API-13 | POST | /v1/files | Upload 1MB file | 200, sha256 matches, file downloadable |
| API-14 | GET | /v1/files/{id} | Download file | Binary matches upload |
| API-15 | DELETE | /v1/files/{id} | Delete file | 200, GET returns 404 |
| API-16 | GET | /v1/presence | List presence | 200, array returned |
| API-17 | PUT | /v1/presence | Set busy status | 200, SSE receives presence event |
| API-18 | POST | /v1/rooms | Create room | 200, room with members |
| API-19 | POST | /v1/rooms/{id}/messages | Send message | 200, SSE receives room_message |
| API-20 | GET | /v1/search/messages?q=test | Search | 200, results match query |
| API-21 | PUT | /v1/messages/{id}/read | Mark read | 200, SSE receives read_receipt |
| API-22 | POST | /v1/messages/{id}/react | Add reaction | 200, SSE receives reaction |
| API-23 | GET | /v1/sip/messages?limit=5 | Pagination | At most 5 results |
| API-24 | POST | /v1/call-history | Sync history | Merged count, no duplicates on re-sync |
| API-25 | GET | /v1/voicemail | List voicemails | 200, array for user |
| API-26 | GET | /v1/recordings | List recordings | 200, array for user |
| API-27 | GET | /v1/events | SSE stream | text/event-stream, keep-alive |
| API-28 | GET | /metrics | Prometheus metrics | Text format with http_requests_total |
| API-29 | GET | /health | Health check | 200, ok=true |
| API-30 | Any | Any authenticated endpoint | Rate limit at 100 RPS | 429 after burst |

## 1.4 SSE Real-Time Event Tests

Open an SSE connection and verify events arrive:

```bash
curl -N -H "Authorization: Bearer $TOKEN" http://localhost:8080/v1/events
```

| Test ID | Trigger Action | Expected SSE Event Type | Payload Contains |
|---------|---------------|------------------------|------------------|
| SSE-01 | Register a SIP user | `presence` | status="online" |
| SSE-02 | De-register a SIP user | `presence` | status="offline" |
| SSE-03 | PUT /v1/presence {status:"busy"} | `presence` | status="busy" |
| SSE-04 | Send SIP MESSAGE | `message` | from_uri, to_uri, body |
| SSE-05 | POST /v1/rooms/{id}/messages | `room_message` | room_id, sender_uri, body |
| SSE-06 | PUT /v1/messages/{id}/read | `read_receipt` | message_id, reader |
| SSE-07 | POST /v1/messages/{id}/react | `reaction` | message_id, emoji, user |
| SSE-08 | PUT /v1/messages/{id} | `message_edited` | message_id, new_body |
| SSE-09 | DELETE /v1/messages/{id} | `message_deleted` | message_id, deleted_by |

---

# Section 2: QA Testing

## 2.1 End-to-End Test Scenarios

Each scenario must be executed on both desktop (macOS/Windows/Linux) and mobile (Android if available).

### Scenario E2E-01: First-Run Onboarding

**Steps:**
1. Launch Pale with no prior configuration
2. Verify setup wizard appears automatically
3. Complete Step 1 (SIP): enter test SIP credentials, click Next
4. Complete Step 2 (Matrix): enter test homeserver, click Next
5. Complete Step 3 (Server): enter pale-server URL, authenticate
6. Click "Start Using Pale"
7. Verify main app loads with all 7 tabs visible

**Pass criteria:** All steps complete without errors. Server connection indicator shows green. SIP registration status shows "Registered" or "Registering".

### Scenario E2E-02: Voice Call Flow

**Steps:**
1. Register two SIP accounts (Alice and Bob)
2. Alice navigates to Dialpad, enters Bob's SIP URI, clicks Call
3. Bob sees incoming call overlay
4. Bob clicks Answer
5. Verify both sides show "Connected" state with timer
6. Alice clicks Mute — verify mute indicator toggles
7. Alice clicks Hold — verify "On Hold" state
8. Alice clicks Resume — verify "Connected" state restored
9. Alice clicks Hang Up
10. Verify both sides return to normal, call appears in Recent

**Pass criteria:** All call states transition correctly. Call history shows the call with correct duration.

### Scenario E2E-03: Chat Messaging

**Steps:**
1. Navigate to Chat tab
2. Click "+" to create a new DM with a Matrix user ID
3. Type a message and press Enter
4. Verify message appears in the thread with timestamp
5. Receive a reply — verify it appears with sender name
6. Verify typing indicator shows when other party types
7. Hover over a received message — verify reaction emojis appear
8. Click a reaction emoji — verify it sends

**Pass criteria:** Messages send and receive in real-time. Typing indicators work. Reactions dispatch.

### Scenario E2E-04: Server Room Chat

**Steps:**
1. Click "+" in Chat, switch to "Group Room" tab
2. Enter room name "Test Room" and add member SIP URIs
3. Click "Create Room"
4. Verify room appears in conversation list
5. Select the room, type and send a message
6. Verify message appears in the room thread
7. From another client, send a message to the room
8. Verify it appears in real-time via SSE

**Pass criteria:** Room created, messages sent/received via server (not Matrix).

### Scenario E2E-05: People Directory with Presence

**Steps:**
1. Navigate to People tab
2. Verify user list loads from server
3. Verify online users show green presence dot, sorted first
4. Click the presence indicator in StatusBar
5. Change status to "Busy"
6. Verify dot changes to red in StatusBar
7. On another client, verify the user shows as "Busy"
8. Click Call button on a contact — verify call initiates
9. Click Chat button on a contact — verify DM opens

**Pass criteria:** Directory loads, presence updates in real-time across clients.

### Scenario E2E-06: File Management

**Steps:**
1. Navigate to Files tab
2. Switch to "Server Files" sub-tab
3. Click Upload, select a file
4. Verify file appears in list with size and timestamp
5. Click Download — verify file downloads correctly
6. Click Delete — verify file removed from list
7. Switch to "Chat Files" — verify Matrix-shared files listed
8. Drag-and-drop a file onto the Files view — verify upload toast

**Pass criteria:** Upload, download, delete all work. File integrity verified via SHA256.

### Scenario E2E-07: Admin Panel

**Steps:**
1. Navigate to Admin tab
2. Log in with admin credentials
3. Verify dashboard loads with metrics (Users, Registered, Online, etc.)
4. Navigate to Users tab — create a new user
5. Navigate to SIP tab — create a SIP account
6. Navigate to Routing tab — create a routing rule
7. Navigate to Conferences tab — create a conference
8. Navigate to Audit tab — verify creation events logged
9. On another client, trigger a SIP registration
10. Verify admin dashboard auto-refreshes (SSE)

**Pass criteria:** All CRUD operations succeed. Audit trail captures actions. SSE refresh works.

### Scenario E2E-08: Search

**Steps:**
1. Send several messages via chat (some containing "budget")
2. Press Cmd+F (or click search icon in StatusBar)
3. Type "budget" in search overlay
4. Verify matching messages appear with sender and timestamp
5. Click a result — verify navigation to the conversation

**Pass criteria:** Search returns relevant results. Click-to-navigate works.

### Scenario E2E-09: Voicemail and Recordings

**Steps:**
1. Navigate to Recent tab
2. Click "Voicemail" sub-tab
3. Verify voicemail list loads (or shows empty state)
4. If voicemails exist, click Play — verify audio playback
5. Click Mark Listened — verify visual change
6. Click Delete — verify removal
7. Switch to "Recordings" sub-tab — repeat play/delete tests

**Pass criteria:** All voicemail/recording operations work. Audio plays correctly.

### Scenario E2E-10: Settings Persistence

**Steps:**
1. Navigate to Settings > Network
2. Change STUN server, click Save
3. Navigate to Settings > Notifications
4. Enable DND, set schedule, click Save
5. Navigate to Settings > Server
6. Verify connection status shows correctly
7. Close and reopen the app
8. Verify all settings are preserved

**Pass criteria:** All settings persist across app restarts.

### Scenario E2E-11: DND Enforcement

**Steps:**
1. Enable DND in Settings > Notifications
2. Set DND window to include the current time
3. Save settings
4. Trigger an incoming SIP call
5. Verify NO toast notification appears
6. Verify call still arrives (ringtone may play, but no toast)
7. Disable DND
8. Trigger another call — verify toast appears

**Pass criteria:** DND suppresses toasts during the configured window.

## 2.2 Regression Checklist

After any code change, run through these quick checks:

- [ ] App launches without errors
- [ ] SIP registration succeeds
- [ ] Outbound call connects
- [ ] Incoming call overlay appears
- [ ] Chat messages send and receive
- [ ] Presence status updates across clients
- [ ] Server connection indicator is correct
- [ ] Admin panel loads and shows data
- [ ] Settings persist after app restart
- [ ] All 62 automated tests pass

## 2.3 Defect Severity Levels

| Severity | Definition | SLA |
|----------|-----------|-----|
| S1 (Critical) | App crashes, data loss, security vulnerability | Fix within 4 hours |
| S2 (Major) | Feature completely broken, no workaround | Fix within 24 hours |
| S3 (Minor) | Feature partially broken, workaround exists | Fix within 1 week |
| S4 (Cosmetic) | Visual issue, typo, alignment | Fix in next sprint |

---

# Section 3: UI/UX Testing

## 3.1 Visual Audit Checklist

Execute on both dark and light themes.

| Area | Check | Dark Theme | Light Theme |
|------|-------|-----------|-------------|
| StatusBar | Registration dot color matches state | | |
| StatusBar | Server indicator shows correct color | | |
| StatusBar | Presence dropdown renders correctly | | |
| BottomNav | Active tab indicator visible | | |
| BottomNav | Unread badge on Chat tab | | |
| ChatView | Message bubbles aligned (own=right, other=left) | | |
| ChatView | Presence dot on DM avatars | | |
| ChatView | Typing indicator animation | | |
| ChatView | Image preview renders inline | | |
| ChatView | Reaction emojis visible on hover | | |
| PeopleView | Online users sorted first | | |
| PeopleView | Call/Chat action buttons on hover | | |
| FilesView | Tab switcher (Chat/Server) | | |
| FilesView | Drag-drop overlay | | |
| AdminView | Metric cards readable | | |
| AdminView | Table data aligned | | |
| SearchOverlay | Results layout clean | | |
| SettingsView | All tabs render without overflow | | |
| SetupWizard | Step indicators correct | | |
| CommandPalette | Keyboard navigation highlight | | |

## 3.2 Accessibility (WCAG 2.1 AA) Audit

| Criterion | Test Method | Status |
|-----------|------------|--------|
| 1.1.1 Non-text content | All images have alt text or aria-hidden | |
| 1.3.1 Info and relationships | Form labels associated with inputs | |
| 1.4.3 Contrast (minimum) | 4.5:1 ratio for normal text, 3:1 for large text | |
| 2.1.1 Keyboard accessible | All interactive elements reachable via Tab | |
| 2.1.2 No keyboard trap | Tab/Escape can exit all modals | |
| 2.4.3 Focus order | Tab order follows visual layout | |
| 2.4.7 Focus visible | Focus indicator visible on all interactive elements | |
| 3.1.1 Language of page | html lang attribute set | |
| 3.3.2 Labels or instructions | All form fields have labels | |
| 4.1.2 Name, role, value | All buttons have aria-label or visible text | |

**Tools:** axe DevTools extension, Lighthouse accessibility audit, manual keyboard-only navigation test.

## 3.3 Responsive / Mobile Audit

Test on these breakpoints:

| Device | Width | Test Areas |
|--------|-------|-----------|
| iPhone SE | 375px | All views render, no overflow, touch targets 44px+ |
| iPhone 14 | 390px | Safe area insets, bottom nav height, notch handling |
| iPad Mini | 768px | Layout transitions, grid adjustments |
| Desktop | 1280px | Full layout, all panels visible |

| Check | Mobile | Tablet | Desktop |
|-------|--------|--------|---------|
| Bottom nav visible and tappable | | | |
| Chat messages scrollable | | | |
| Dialpad buttons reachable | | | |
| Settings tabs fit without horizontal scroll | | | |
| Admin metrics cards wrap properly | | | |
| Command palette positioned correctly | | | |
| Search overlay doesn't overlap status bar | | | |
| Incoming call overlay fills screen | | | |
| File drag-drop area covers full view | | | |

---

# Section 4: DBA Testing

## 4.1 Schema Review

**Files to review:**
- `src-tauri/crates/pale-server/migrations/001_initial_schema.sql`
- `src-tauri/crates/pale-server/migrations/002_rooms_search_receipts_avatars.sql`
- `src-tauri/crates/pale-server/migrations/003_voicemail_recordings.sql`

### Schema Review Checklist

| Check | Details | Status |
|-------|---------|--------|
| **Primary keys** | All tables use UUID primary keys (uuid_generate_v4) | |
| **Foreign keys** | conference_participants.conference_id FK with CASCADE delete | |
| | room_members.room_id FK with CASCADE delete | |
| | room_messages.room_id FK with CASCADE delete | |
| | calls.conference_id FK with SET NULL | |
| | voicemails.file_id FK with SET NULL | |
| | call_recordings.file_id FK with SET NULL | |
| | users.avatar_file_id FK with SET NULL | |
| **Unique constraints** | sip_accounts (username, domain) | |
| | sip_registrations (aor) | |
| | sip_dialogs (call_id) | |
| | sip_subscriptions (subscription_id) | |
| | room_members (room_id, user_sip_uri) | |
| | conference_participants (conference_id, user_id) | |
| | message_reads (message_id, reader_uri) | |
| | call_history dedup (user_sip_uri, start_time, remote_uri, direction) | |
| **Indexes** | Verify all indexes listed in migrations exist | |
| **Partial indexes** | sip_dialogs: WHERE status NOT IN (ended, failed, cancelled) | |
| | calls: WHERE status NOT IN (ended, failed) | |
| | presence: WHERE status != 'offline' | |
| | routing_rules: WHERE enabled = true | |
| | voicemails: WHERE listened = false | |
| **Triggers** | update_updated_at on 7 tables | |
| | sip_messages_search_trigger (tsvector) | |
| | room_messages_search_trigger (tsvector) | |
| **Functions** | cleanup_expired() purges registrations, subscriptions, sessions | |
| **Extensions** | uuid-ossp, pgcrypto | |
| **Data types** | All timestamps are TIMESTAMPTZ (UTC-aware) | |
| | JSONB for media_types, callees, media, search_vector | |
| | TEXT for SIP URIs (not VARCHAR with length limit) | |

### Normalization Review

| Table | Normal Form | Notes |
|-------|------------|-------|
| users | 3NF | sip_uri is unique, could be PK but UUID preferred for API stability |
| sip_accounts | 3NF | Composite unique on (username, domain) |
| conferences + conference_participants | 3NF | Properly normalized join table |
| rooms + room_members + room_messages | 3NF | Three-table normalized design |
| calls | 2NF | callees and media stored as JSONB arrays (intentional denormalization for read performance) |
| sip_dialogs | 2NF | media_types as JSONB (intentional, variable-length list) |

## 4.2 Query Performance Testing

Connect to the database and run EXPLAIN ANALYZE on critical queries:

```sql
-- 1. Presence lookup (hot path, called on every SSE connection)
EXPLAIN ANALYZE SELECT * FROM presence WHERE status != 'offline';

-- 2. Message search (user-facing, could be slow on large datasets)
EXPLAIN ANALYZE SELECT * FROM sip_messages
WHERE search_vector @@ to_tsquery('english', 'budget')
ORDER BY received_at DESC LIMIT 50;

-- 3. Room messages (loaded on every room open)
EXPLAIN ANALYZE SELECT * FROM room_messages
WHERE room_id = 'some-uuid' ORDER BY created_at DESC LIMIT 100;

-- 4. Call history dedup check (called on every sync)
EXPLAIN ANALYZE SELECT * FROM call_history
WHERE user_sip_uri = 'sip:alice@example.com'
AND start_time = '2026-06-07T10:00:00Z'
AND remote_uri = 'sip:bob@example.com'
AND direction = 'outbound';

-- 5. Active dialogs (admin dashboard refresh)
EXPLAIN ANALYZE SELECT * FROM sip_dialogs
WHERE status NOT IN ('ended', 'failed', 'cancelled');

-- 6. Unlistened voicemails (badge count)
EXPLAIN ANALYZE SELECT count(*) FROM voicemails
WHERE callee_uri = 'sip:alice@example.com' AND listened = false;

-- 7. Routing rule resolution (called on every INVITE)
EXPLAIN ANALYZE SELECT * FROM routing_rules
WHERE enabled = true ORDER BY priority ASC;

-- 8. Audit log (admin panel)
EXPLAIN ANALYZE SELECT * FROM audit_events
ORDER BY created_at DESC LIMIT 500;
```

**Pass criteria:** All queries use index scans (no sequential scans on tables with 1000+ rows). Response time under 10ms for single-row lookups, under 100ms for list queries.

## 4.3 Data Integrity Testing

```sql
-- 1. Verify no orphaned room members
SELECT rm.* FROM room_members rm
LEFT JOIN rooms r ON rm.room_id = r.id
WHERE r.id IS NULL;
-- Expected: 0 rows

-- 2. Verify no orphaned conference participants
SELECT cp.* FROM conference_participants cp
LEFT JOIN conferences c ON cp.conference_id = c.id
WHERE c.id IS NULL;
-- Expected: 0 rows

-- 3. Verify call history dedup constraint
INSERT INTO call_history (user_sip_uri, direction, remote_uri, remote_name, start_time, duration_secs, answered)
VALUES ('sip:test@example.com', 'outbound', 'sip:bob@example.com', 'Bob', '2026-06-07T10:00:00Z', 60, true);
INSERT INTO call_history (user_sip_uri, direction, remote_uri, remote_name, start_time, duration_secs, answered)
VALUES ('sip:test@example.com', 'outbound', 'sip:bob@example.com', 'Bob', '2026-06-07T10:00:00Z', 60, true);
-- Expected: Second insert silently ignored (ON CONFLICT DO NOTHING)

-- 4. Verify expired registrations are filterable
SELECT * FROM sip_registrations WHERE expires_at < now();
-- These should be cleaned by cleanup_expired()

-- 5. Verify SIP account password not exposed
SELECT password_ha1 FROM sip_accounts LIMIT 1;
-- This column exists but the API serialization skips it (verify via API response)

-- 6. Verify cascade deletes
DELETE FROM rooms WHERE id = 'test-room-id';
SELECT * FROM room_members WHERE room_id = 'test-room-id';
-- Expected: 0 rows (CASCADE)
SELECT * FROM room_messages WHERE room_id = 'test-room-id';
-- Expected: 0 rows (CASCADE)
```

## 4.4 Backup and Recovery Testing

### Backup Test

```bash
# Run the backup script
PGPASSWORD=$POSTGRES_PASSWORD ./scripts/backup.sh

# Verify backup file created
ls -la backups/pale_*.sql.gz

# Verify backup is valid
gunzip -c backups/pale_$(date +%Y%m%d)*.sql.gz | head -20
# Should show valid SQL statements
```

### Recovery Test

```bash
# Create a test database
PGPASSWORD=$POSTGRES_PASSWORD createdb -h localhost -U pale pale_recovery_test

# Restore from backup
gunzip -c backups/pale_$(date +%Y%m%d)*.sql.gz | \
  PGPASSWORD=$POSTGRES_PASSWORD psql -h localhost -U pale pale_recovery_test

# Verify data integrity
PGPASSWORD=$POSTGRES_PASSWORD psql -h localhost -U pale pale_recovery_test \
  -c "SELECT count(*) FROM users; SELECT count(*) FROM sip_accounts; SELECT count(*) FROM audit_events;"

# Clean up
PGPASSWORD=$POSTGRES_PASSWORD dropdb -h localhost -U pale pale_recovery_test
```

**Pass criteria:** Backup completes without errors. Restored database has identical row counts. All tables and indexes present after restore.

## 4.5 Connection Pool and Load Testing

```bash
# Verify connection pool settings
# Default: PALE_PG_MAX_CONNECTIONS=10

# Test concurrent connections (requires pgbench or similar)
pgbench -h localhost -U pale -d pale -c 20 -j 4 -T 30 \
  -f - <<SQL
SELECT * FROM presence WHERE status != 'offline';
SQL

# Monitor pool exhaustion in pale-server logs
# Look for: "Postgres write failed" errors under load
```

**Pass criteria:** Server handles 20 concurrent database clients without pool exhaustion. No connection timeouts under normal load.

## 4.6 Migration Safety

```bash
# Test idempotent migration (run twice)
PGPASSWORD=$POSTGRES_PASSWORD psql -h localhost -U pale -d pale \
  -f src-tauri/crates/pale-server/migrations/001_initial_schema.sql
PGPASSWORD=$POSTGRES_PASSWORD psql -h localhost -U pale -d pale \
  -f src-tauri/crates/pale-server/migrations/001_initial_schema.sql

# Expected: No errors (all CREATE statements use IF NOT EXISTS)

# Same for migration 002 and 003
```

**Pass criteria:** All migrations are idempotent — running them multiple times produces no errors and no duplicate objects.

---

# Section 5: Sign-Off

## Sign-Off Requirements

All four teams must sign off before production deployment:

| Team | Sign-Off Criteria | Signed By | Date |
|------|------------------|-----------|------|
| **Engineering** | All 62 automated tests pass. CI pipeline green. Docker image builds and runs. All SIP-xx and API-xx tests pass. | | |
| **QA** | All E2E-xx scenarios pass on desktop and mobile. Regression checklist complete. No S1/S2 defects open. | | |
| **UI/UX** | Visual audit complete on both themes. Accessibility audit passes WCAG 2.1 AA. Responsive audit passes on all breakpoints. | | |
| **DBA** | Schema review complete. All query plans use indexes. Data integrity queries return 0 orphans. Backup/restore verified. Migrations idempotent. | | |

## Defect Log Template

| ID | Severity | Component | Description | Found By | Status | Assigned To |
|----|----------|-----------|-------------|----------|--------|-------------|
| | | | | | | |
