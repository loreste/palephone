# Pale Server — DBA Schema Review & Sign-Off

**Reviewer:** DBA Team  
**Date:** 2026-06-07  
**Schema version:** Migrations 001 + 002 + 003  
**Database:** PostgreSQL 16  
**Tables reviewed:** 24

---

## Review Summary

| Category | Findings | Severity |
|----------|----------|----------|
| Redundant indexes | 2 | Low |
| Missing constraints | 3 | Medium |
| Data type concerns | 2 | Medium |
| Missing retention policy | 1 | High |
| Trigger gap | 1 | Low |
| Schema design issues | 2 | Medium |
| Total findings | 11 | |

---

## Finding 1: Redundant index on users.sip_uri

**Table:** `users`  
**Issue:** `idx_users_sip_uri` is redundant because `sip_uri` already has a `UNIQUE` constraint, which implicitly creates a unique index.

**Action:** Drop the explicit index.
```sql
DROP INDEX IF EXISTS idx_users_sip_uri;
```

**Severity:** Low — no functional impact, just wasted space.

## Finding 2: Redundant index on sip_accounts(username, domain)

**Table:** `sip_accounts`  
**Issue:** `idx_sip_accounts_aor` on `(username, domain)` duplicates the `UNIQUE (username, domain)` constraint which already creates an index.

**Action:** Drop the explicit index.
```sql
DROP INDEX IF EXISTS idx_sip_accounts_aor;
```

**Severity:** Low.

## Finding 3: call_history.start_time should be TIMESTAMPTZ, not TEXT

**Table:** `call_history`  
**Issue:** `start_time` is stored as `TEXT` (ISO 8601 string from client). This prevents proper time-range queries, index-based sorting, and timezone handling. Every query must parse the string.

**Action:** Change to `TIMESTAMPTZ`. The application layer should parse the ISO 8601 string before insertion.
```sql
ALTER TABLE call_history ALTER COLUMN start_time TYPE TIMESTAMPTZ USING start_time::timestamptz;
```

**Severity:** Medium — affects query performance on call history lookups.

## Finding 4: Missing CHECK constraints on status columns

**Tables:** `sip_dialogs`, `calls`, `presence`, `conferences`  
**Issue:** Status columns use `TEXT` with no `CHECK` constraint. Invalid values like `"foobar"` can be inserted. The application enforces valid values but the database should too.

**Action:** Add CHECK constraints.
```sql
ALTER TABLE sip_dialogs ADD CONSTRAINT chk_dialog_status
  CHECK (status IN ('routing', 'ringing', 'held', 'cancelled', 'ended', 'failed'));

ALTER TABLE calls ADD CONSTRAINT chk_call_status
  CHECK (status IN ('ringing', 'active', 'held', 'ended', 'failed'));

ALTER TABLE presence ADD CONSTRAINT chk_presence_status
  CHECK (status IN ('online', 'offline', 'busy', 'away', 'dnd'));

ALTER TABLE conferences ADD CONSTRAINT chk_conference_mode
  CHECK (mode IN ('audio', 'video', 'webinar'));
```

**Severity:** Medium — data integrity risk without constraint.

## Finding 5: Missing NOT NULL on room_messages.room_id FK reference

**Table:** `room_messages`  
**Issue:** The FK `room_id UUID NOT NULL REFERENCES rooms(id) ON DELETE CASCADE` is correct. No issue here. However, `message_reads.message_id` lacks a FK constraint — it references either `sip_messages.id` or `room_messages.id` via a `message_source` discriminator, making a proper FK impossible.

**Recommendation:** This is a known polymorphic association pattern. Accept as-is with application-layer enforcement, but add a comment documenting the design decision.

**Severity:** Low — intentional design trade-off.

## Finding 6: No retention policy for high-volume tables

**Tables:** `sip_messages`, `sip_transactions`, `sip_notifications`, `audit_events`  
**Issue:** These tables grow unboundedly. The in-memory caches have limits (10K-50K) but the PostgreSQL tables have no partitioning, archival, or cleanup.

**Action:** Add time-based partitioning or a scheduled cleanup job.
```sql
-- Add to cleanup_expired() function:
CREATE OR REPLACE FUNCTION cleanup_expired() RETURNS void AS $$
BEGIN
    DELETE FROM sip_registrations WHERE expires_at < now();
    DELETE FROM sip_subscriptions WHERE expires_at < now();
    DELETE FROM admin_sessions WHERE expires_at < now();
    -- Retention: keep 90 days of high-volume data
    DELETE FROM sip_transactions WHERE created_at < now() - interval '90 days';
    DELETE FROM sip_notifications WHERE received_at < now() - interval '90 days';
    DELETE FROM sip_messages WHERE received_at < now() - interval '90 days';
    DELETE FROM audit_events WHERE created_at < now() - interval '365 days';
    DELETE FROM room_messages WHERE created_at < now() - interval '365 days';
END;
$$ LANGUAGE plpgsql;
```

Schedule via `pg_cron` or external cron:
```sql
-- If pg_cron is available:
SELECT cron.schedule('pale-cleanup', '0 3 * * *', 'SELECT cleanup_expired()');
```

**Severity:** High — will cause disk space issues in production.

## Finding 7: Missing updated_at trigger on presence table

**Table:** `presence`  
**Issue:** The `presence` table has an `updated_at` column but no `update_updated_at()` trigger. Updates via direct SQL won't auto-set `updated_at`.

**Action:**
```sql
CREATE TRIGGER trg_presence_updated_at BEFORE UPDATE ON presence
  FOR EACH ROW EXECUTE FUNCTION update_updated_at();
```

**Severity:** Low — the application always sets `updated_at` explicitly, but the trigger is a safety net.

## Finding 8: conference_participants.user_id lacks FK to users

**Table:** `conference_participants`  
**Issue:** `user_id UUID NOT NULL` has no FK reference to `users(id)`. If a user is deleted, orphaned participants remain (CASCADE delete won't fire).

**Action:**
```sql
ALTER TABLE conference_participants
  ADD CONSTRAINT fk_participant_user
  FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE CASCADE;
```

**Severity:** Medium — data integrity risk.

## Finding 9: sip_dialogs.call_id index is redundant

**Table:** `sip_dialogs`  
**Issue:** `idx_sip_dialogs_call_id` on `(call_id)` duplicates the `UNIQUE` constraint on `call_id`.

**Action:** Drop the explicit index.
```sql
DROP INDEX IF EXISTS idx_sip_dialogs_call_id;
```

**Severity:** Low.

## Finding 10: sip_registrations.aor index is redundant

**Table:** `sip_registrations`  
**Issue:** `idx_sip_registrations_aor` on `(aor)` duplicates the `UNIQUE` constraint.

**Action:** Drop the explicit index.
```sql
DROP INDEX IF EXISTS idx_sip_registrations_aor;
```

**Severity:** Low.

## Finding 11: No connection pooling configuration guidance

**Issue:** The application uses `deadpool-postgres` with a default pool size. No documentation on recommended pool sizing relative to `max_connections` in `postgresql.conf`.

**Recommendation:** Document that `PALE_PG_MAX_CONNECTIONS` should be set to at most `(max_connections - 5) / number_of_server_instances` to leave headroom for admin connections and replication.

**Severity:** Medium — can cause connection exhaustion in multi-instance deployments.

---

## Required Migration: 004_dba_fixes.sql

All fixes from this review have been implemented in `migrations/004_dba_fixes.sql`:

- 4 redundant indexes dropped (Finding 1, 2, 9, 10)
- `call_history.start_time` converted from TEXT to TIMESTAMPTZ (Finding 3)
- 8 CHECK constraints added on status/mode/role/direction columns (Finding 4)
- Retention policy added: 90 days for transactions/notifications/messages, 1 year for audit/chat (Finding 6)
- Missing `updated_at` trigger added to presence table (Finding 7)
- FK from `conference_participants.user_id` to `users(id)` with CASCADE delete (Finding 8)

All ALTER statements use `DO $$ ... EXCEPTION WHEN duplicate_object` blocks for idempotency.

---

## Post-Fix Verification Queries

Run these after applying migration 004 to verify all fixes:

```sql
-- Verify redundant indexes are gone
SELECT indexname FROM pg_indexes WHERE tablename = 'users' AND indexname = 'idx_users_sip_uri';
-- Expected: 0 rows

-- Verify CHECK constraints exist
SELECT conname FROM pg_constraint WHERE conrelid = 'sip_dialogs'::regclass AND contype = 'c';
-- Expected: chk_dialog_status

SELECT conname FROM pg_constraint WHERE conrelid = 'presence'::regclass AND contype = 'c';
-- Expected: chk_presence_status

-- Verify call_history.start_time is TIMESTAMPTZ
SELECT data_type FROM information_schema.columns
WHERE table_name = 'call_history' AND column_name = 'start_time';
-- Expected: timestamp with time zone

-- Verify presence trigger exists
SELECT tgname FROM pg_trigger WHERE tgrelid = 'presence'::regclass AND tgname = 'trg_presence_updated_at';
-- Expected: 1 row

-- Verify conference_participants FK
SELECT conname FROM pg_constraint
WHERE conrelid = 'conference_participants'::regclass AND confrelid = 'users'::regclass;
-- Expected: fk_participant_user

-- Test CHECK constraint enforcement
INSERT INTO presence (sip_uri, status) VALUES ('sip:test@check.com', 'invalid_status');
-- Expected: ERROR: violates check constraint "chk_presence_status"
```

---

## Capacity Planning

| Table | Growth Rate (est.) | Retention | Projected 1-Year Size |
|-------|-------------------|-----------|----------------------|
| users | ~10/week | Permanent | ~500 rows |
| sip_accounts | ~10/week | Permanent | ~500 rows |
| sip_registrations | Volatile (TTL) | Auto-expire | ~200 active |
| sip_dialogs | ~1000/day | Permanent | ~365K rows |
| sip_messages | ~5000/day | 90 days | ~450K rows |
| sip_transactions | ~10000/day | 90 days | ~900K rows |
| room_messages | ~2000/day | 1 year | ~730K rows |
| audit_events | ~500/day | 1 year | ~180K rows |
| call_history | ~200/day | Permanent | ~73K rows |
| files | ~50/day | Permanent | ~18K rows |
| voicemails | ~20/day | Permanent | ~7K rows |

**Disk estimate:** ~2-5 GB at 1 year with GIN indexes. Recommend monitoring `pg_total_relation_size()` monthly.

**Recommended PostgreSQL settings:**
```
shared_buffers = 256MB          -- 25% of available RAM for dedicated server
work_mem = 16MB                 -- Per-operation memory for sorts/joins
effective_cache_size = 1GB      -- OS page cache estimate
maintenance_work_mem = 128MB    -- For VACUUM, CREATE INDEX
max_connections = 50            -- Leave headroom beyond pool size
```

---

## DBA Sign-Off

| Check | Status |
|-------|--------|
| Schema normalization reviewed (3NF with documented denormalization) | |
| All primary keys are UUIDs (uuid_generate_v4) | |
| All timestamps are TIMESTAMPTZ (UTC-aware) | |
| Foreign keys have appropriate ON DELETE actions | |
| CHECK constraints enforce valid enum values | |
| Indexes cover all query patterns (no redundant indexes) | |
| Partial indexes used for active-only queries | |
| Full-text search uses GIN indexes on tsvector columns | |
| Retention policy implemented for high-volume tables | |
| updated_at triggers on all mutable tables | |
| Migrations are idempotent (IF NOT EXISTS, DO $$ EXCEPTION) | |
| Backup/restore procedure verified | |
| Connection pooling documented | |
| Capacity planning documented | |

**Signed:** ____________________________  
**Date:** ____________________________  
**Comments:** ________________________________________________________________________
