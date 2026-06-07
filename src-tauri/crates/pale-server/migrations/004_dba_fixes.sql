-- Migration 004: DBA review fixes
-- Addresses findings from DBA schema review (DBA_REVIEW.md)

-- ============================================================================
-- Finding 1, 2, 9, 10: Remove redundant indexes (covered by UNIQUE constraints)
-- ============================================================================

DROP INDEX IF EXISTS idx_users_sip_uri;           -- UNIQUE constraint covers this
DROP INDEX IF EXISTS idx_sip_accounts_aor;         -- UNIQUE (username, domain) covers this
DROP INDEX IF EXISTS idx_sip_dialogs_call_id;      -- UNIQUE constraint covers this
DROP INDEX IF EXISTS idx_sip_registrations_aor;    -- UNIQUE constraint covers this

-- ============================================================================
-- Finding 3: Convert call_history.start_time from TEXT to TIMESTAMPTZ
-- ============================================================================

ALTER TABLE call_history
  ALTER COLUMN start_time TYPE TIMESTAMPTZ
  USING start_time::timestamptz;

-- Recreate the dedup index with the corrected type
DROP INDEX IF EXISTS idx_call_history_dedup;
CREATE UNIQUE INDEX idx_call_history_dedup
  ON call_history (user_sip_uri, start_time, remote_uri, direction);

DROP INDEX IF EXISTS idx_call_history_start_time;
CREATE INDEX idx_call_history_start_time
  ON call_history (user_sip_uri, start_time DESC);

-- ============================================================================
-- Finding 4: Add CHECK constraints on status/mode columns
-- ============================================================================

DO $$ BEGIN
  ALTER TABLE sip_dialogs ADD CONSTRAINT chk_dialog_status
    CHECK (status IN ('routing', 'ringing', 'held', 'cancelled', 'ended', 'failed'));
EXCEPTION WHEN duplicate_object THEN NULL;
END $$;

DO $$ BEGIN
  ALTER TABLE calls ADD CONSTRAINT chk_call_status
    CHECK (status IN ('ringing', 'active', 'held', 'ended', 'failed'));
EXCEPTION WHEN duplicate_object THEN NULL;
END $$;

DO $$ BEGIN
  ALTER TABLE presence ADD CONSTRAINT chk_presence_status
    CHECK (status IN ('online', 'offline', 'busy', 'away', 'dnd'));
EXCEPTION WHEN duplicate_object THEN NULL;
END $$;

DO $$ BEGIN
  ALTER TABLE conferences ADD CONSTRAINT chk_conference_mode
    CHECK (mode IN ('audio', 'video', 'webinar'));
EXCEPTION WHEN duplicate_object THEN NULL;
END $$;

DO $$ BEGIN
  ALTER TABLE room_members ADD CONSTRAINT chk_room_member_role
    CHECK (role IN ('admin', 'member'));
EXCEPTION WHEN duplicate_object THEN NULL;
END $$;

DO $$ BEGIN
  ALTER TABLE conference_participants ADD CONSTRAINT chk_participant_role
    CHECK (role IN ('host', 'moderator', 'member'));
EXCEPTION WHEN duplicate_object THEN NULL;
END $$;

-- ============================================================================
-- Finding 6: Retention policy for high-volume tables
-- ============================================================================

CREATE OR REPLACE FUNCTION cleanup_expired() RETURNS void AS $$
BEGIN
    -- TTL-based cleanup
    DELETE FROM sip_registrations WHERE expires_at < now();
    DELETE FROM sip_subscriptions WHERE expires_at < now();
    DELETE FROM admin_sessions WHERE expires_at < now();

    -- Retention: 90 days for high-volume operational data
    DELETE FROM sip_transactions WHERE created_at < now() - interval '90 days';
    DELETE FROM sip_notifications WHERE received_at < now() - interval '90 days';
    DELETE FROM sip_messages WHERE received_at < now() - interval '90 days';

    -- Retention: 1 year for audit and chat history
    DELETE FROM audit_events WHERE created_at < now() - interval '365 days';
    DELETE FROM room_messages WHERE created_at < now() - interval '365 days';
END;
$$ LANGUAGE plpgsql;

-- ============================================================================
-- Finding 7: Add missing updated_at trigger on presence
-- ============================================================================

DO $$ BEGIN
  CREATE TRIGGER trg_presence_updated_at
    BEFORE UPDATE ON presence
    FOR EACH ROW EXECUTE FUNCTION update_updated_at();
EXCEPTION WHEN duplicate_object THEN NULL;
END $$;

-- ============================================================================
-- Finding 8: Add FK from conference_participants.user_id to users
-- ============================================================================

DO $$ BEGIN
  ALTER TABLE conference_participants
    ADD CONSTRAINT fk_participant_user
    FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE CASCADE;
EXCEPTION WHEN duplicate_object THEN NULL;
END $$;

-- ============================================================================
-- Additional: Add CHECK on call_history.direction and message_reads.message_source
-- ============================================================================

DO $$ BEGIN
  ALTER TABLE call_history ADD CONSTRAINT chk_call_history_direction
    CHECK (direction IN ('inbound', 'outbound'));
EXCEPTION WHEN duplicate_object THEN NULL;
END $$;

DO $$ BEGIN
  ALTER TABLE message_reads ADD CONSTRAINT chk_message_reads_source
    CHECK (message_source IN ('sip', 'room'));
EXCEPTION WHEN duplicate_object THEN NULL;
END $$;
