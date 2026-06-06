-- Pale Server PostgreSQL Schema
-- Designed for a Teams-like unified communications platform
-- Normalization: 3NF with strategic denormalization for read-heavy paths
-- All timestamps in UTC, UUIDs for primary keys, soft-delete not used (hard delete + audit trail)

-- ============================================================================
-- EXTENSIONS
-- ============================================================================

CREATE EXTENSION IF NOT EXISTS "uuid-ossp";
CREATE EXTENSION IF NOT EXISTS "pgcrypto";

-- ============================================================================
-- USERS & AUTHENTICATION
-- ============================================================================

CREATE TABLE users (
    id              UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    display_name    TEXT NOT NULL,
    sip_uri         TEXT NOT NULL UNIQUE,
    matrix_user_id  TEXT,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_users_sip_uri ON users (sip_uri);
CREATE INDEX idx_users_matrix_user_id ON users (matrix_user_id) WHERE matrix_user_id IS NOT NULL;

CREATE TABLE admin_sessions (
    token           TEXT PRIMARY KEY,
    principal       TEXT NOT NULL,
    expires_at      TIMESTAMPTZ NOT NULL,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_admin_sessions_expires ON admin_sessions (expires_at);

-- ============================================================================
-- SIP ACCOUNTS & REGISTRATIONS
-- ============================================================================

CREATE TABLE sip_accounts (
    id              UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    username        TEXT NOT NULL,
    domain          TEXT NOT NULL,
    display_name    TEXT,
    password_ha1    TEXT NOT NULL,  -- MD5(username:realm:password), encrypted at app layer
    enabled         BOOLEAN NOT NULL DEFAULT true,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (username, domain)
);

CREATE INDEX idx_sip_accounts_aor ON sip_accounts (username, domain);

CREATE TABLE sip_registrations (
    id              UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    aor             TEXT NOT NULL UNIQUE,  -- sip:user@domain (Address of Record)
    contact         TEXT NOT NULL,          -- sip:user@ip:port (registered endpoint)
    source          TEXT NOT NULL,          -- peer IP:port
    user_agent      TEXT,
    expires_at      TIMESTAMPTZ NOT NULL,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_sip_registrations_aor ON sip_registrations (aor);
CREATE INDEX idx_sip_registrations_expires ON sip_registrations (expires_at);

-- ============================================================================
-- SIP SIGNALING STATE
-- ============================================================================

CREATE TABLE sip_dialogs (
    id              UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    call_id         TEXT NOT NULL UNIQUE,   -- SIP Call-ID header
    from_uri        TEXT NOT NULL,
    to_uri          TEXT NOT NULL,
    target_contact  TEXT,
    status          TEXT NOT NULL DEFAULT 'routing',  -- routing, ringing, held, cancelled, ended, failed
    media_types     JSONB NOT NULL DEFAULT '[]',      -- ["audio"], ["audio","video"]
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_sip_dialogs_call_id ON sip_dialogs (call_id);
CREATE INDEX idx_sip_dialogs_status ON sip_dialogs (status) WHERE status NOT IN ('ended', 'failed', 'cancelled');
CREATE INDEX idx_sip_dialogs_from_uri ON sip_dialogs (from_uri);
CREATE INDEX idx_sip_dialogs_to_uri ON sip_dialogs (to_uri);

CREATE TABLE sip_messages (
    id              UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    call_id         TEXT,
    from_uri        TEXT NOT NULL,
    to_uri          TEXT NOT NULL,
    content_type    TEXT NOT NULL DEFAULT 'text/plain',
    body            TEXT NOT NULL DEFAULT '',
    received_at     TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_sip_messages_from_uri ON sip_messages (from_uri);
CREATE INDEX idx_sip_messages_to_uri ON sip_messages (to_uri);
CREATE INDEX idx_sip_messages_received_at ON sip_messages (received_at DESC);
-- Composite index for paginated message queries (room_id filter + cursor)
CREATE INDEX idx_sip_messages_conversation ON sip_messages (from_uri, to_uri, received_at DESC);

CREATE TABLE sip_transactions (
    id              UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    method          TEXT NOT NULL,          -- REGISTER, INVITE, BYE, etc.
    uri             TEXT NOT NULL,
    call_id         TEXT,
    cseq            TEXT,
    source          TEXT NOT NULL,          -- peer IP:port
    status_code     SMALLINT,
    reason          TEXT,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_sip_transactions_method ON sip_transactions (method);
CREATE INDEX idx_sip_transactions_call_id ON sip_transactions (call_id) WHERE call_id IS NOT NULL;
CREATE INDEX idx_sip_transactions_created_at ON sip_transactions (created_at DESC);

-- ============================================================================
-- SIP SUBSCRIPTIONS & NOTIFICATIONS (Presence, Dialog Events)
-- ============================================================================

CREATE TABLE sip_subscriptions (
    id                  UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    subscription_id     TEXT NOT NULL UNIQUE,
    subscriber          TEXT NOT NULL,      -- who is subscribing
    target              TEXT NOT NULL,      -- what they're watching
    event               TEXT NOT NULL,      -- presence, dialog, message-summary, conference
    expires_at          TIMESTAMPTZ NOT NULL,
    created_at          TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at          TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_sip_subscriptions_subscriber ON sip_subscriptions (subscriber);
CREATE INDEX idx_sip_subscriptions_target ON sip_subscriptions (target);
CREATE INDEX idx_sip_subscriptions_expires ON sip_subscriptions (expires_at);

CREATE TABLE sip_notifications (
    id                  UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    subscription_id     TEXT,
    notifier            TEXT NOT NULL,
    target              TEXT NOT NULL,
    event               TEXT,
    subscription_state  TEXT,
    content_type        TEXT NOT NULL DEFAULT 'application/pidf+xml',
    body                TEXT NOT NULL DEFAULT '',
    received_at         TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_sip_notifications_received_at ON sip_notifications (received_at DESC);
CREATE INDEX idx_sip_notifications_subscription ON sip_notifications (subscription_id) WHERE subscription_id IS NOT NULL;

-- ============================================================================
-- PRESENCE
-- ============================================================================

CREATE TABLE presence (
    sip_uri         TEXT PRIMARY KEY,
    status          TEXT NOT NULL DEFAULT 'offline',  -- online, offline, busy, away, dnd
    note            TEXT,
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_presence_status ON presence (status) WHERE status != 'offline';

-- ============================================================================
-- CONFERENCES & CALLS
-- ============================================================================

CREATE TABLE conferences (
    id              UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    title           TEXT NOT NULL,
    mode            TEXT NOT NULL DEFAULT 'audio',  -- audio, video, webinar
    active          BOOLEAN NOT NULL DEFAULT false,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE conference_participants (
    id              UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    conference_id   UUID NOT NULL REFERENCES conferences(id) ON DELETE CASCADE,
    user_id         UUID NOT NULL,
    sip_uri         TEXT NOT NULL,
    role            TEXT NOT NULL DEFAULT 'member',  -- host, moderator, member
    bridge_slot     INTEGER,
    joined_at       TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (conference_id, user_id)
);

CREATE INDEX idx_conference_participants_conference ON conference_participants (conference_id);

CREATE TABLE calls (
    id              UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    conference_id   UUID REFERENCES conferences(id) ON DELETE SET NULL,
    caller          TEXT NOT NULL,
    callees         JSONB NOT NULL DEFAULT '[]',
    media           JSONB NOT NULL DEFAULT '["audio"]',
    status          TEXT NOT NULL DEFAULT 'ringing',  -- ringing, active, held, ended, failed
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_calls_status ON calls (status) WHERE status NOT IN ('ended', 'failed');
CREATE INDEX idx_calls_caller ON calls (caller);
CREATE INDEX idx_calls_conference ON calls (conference_id) WHERE conference_id IS NOT NULL;

-- ============================================================================
-- CALL HISTORY (synced from clients)
-- ============================================================================

CREATE TABLE call_history (
    id              UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    user_sip_uri    TEXT NOT NULL,
    direction       TEXT NOT NULL,         -- inbound, outbound
    remote_uri      TEXT NOT NULL,
    remote_name     TEXT NOT NULL DEFAULT '',
    start_time      TEXT NOT NULL,          -- ISO 8601 from client
    duration_secs   BIGINT NOT NULL DEFAULT 0,
    answered        BOOLEAN NOT NULL DEFAULT false,
    synced_at       TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_call_history_user ON call_history (user_sip_uri);
CREATE INDEX idx_call_history_start_time ON call_history (user_sip_uri, start_time DESC);
-- Deduplication index for sync merge
CREATE UNIQUE INDEX idx_call_history_dedup ON call_history (user_sip_uri, start_time, remote_uri, direction);

-- ============================================================================
-- FILES
-- ============================================================================

CREATE TABLE files (
    id              UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    owner           TEXT NOT NULL,
    filename        TEXT NOT NULL,
    content_type    TEXT NOT NULL DEFAULT 'application/octet-stream',
    size            BIGINT NOT NULL DEFAULT 0,
    sha256          TEXT NOT NULL,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_files_owner ON files (owner);
CREATE INDEX idx_files_created_at ON files (created_at DESC);

-- ============================================================================
-- ROUTING RULES
-- ============================================================================

CREATE TABLE routing_rules (
    id                      UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    name                    TEXT NOT NULL,
    source_pattern          TEXT NOT NULL DEFAULT '*',
    destination_pattern     TEXT NOT NULL DEFAULT 'sip:*',
    target                  TEXT NOT NULL,
    priority                INTEGER NOT NULL DEFAULT 100,
    enabled                 BOOLEAN NOT NULL DEFAULT true,
    created_at              TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at              TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_routing_rules_priority ON routing_rules (priority ASC) WHERE enabled = true;

-- ============================================================================
-- AUDIT LOG
-- ============================================================================

CREATE TABLE audit_events (
    id              UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    principal       TEXT NOT NULL,
    action          TEXT NOT NULL,
    target          TEXT,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_audit_events_created_at ON audit_events (created_at DESC);
CREATE INDEX idx_audit_events_principal ON audit_events (principal);
CREATE INDEX idx_audit_events_action ON audit_events (action);

-- ============================================================================
-- MAINTENANCE: Auto-cleanup of expired data
-- ============================================================================

-- Function to purge expired registrations and subscriptions
CREATE OR REPLACE FUNCTION cleanup_expired() RETURNS void AS $$
BEGIN
    DELETE FROM sip_registrations WHERE expires_at < now();
    DELETE FROM sip_subscriptions WHERE expires_at < now();
    DELETE FROM admin_sessions WHERE expires_at < now();
END;
$$ LANGUAGE plpgsql;

-- ============================================================================
-- TRIGGERS: Auto-update updated_at
-- ============================================================================

CREATE OR REPLACE FUNCTION update_updated_at()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at = now();
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER trg_users_updated_at BEFORE UPDATE ON users FOR EACH ROW EXECUTE FUNCTION update_updated_at();
CREATE TRIGGER trg_sip_accounts_updated_at BEFORE UPDATE ON sip_accounts FOR EACH ROW EXECUTE FUNCTION update_updated_at();
CREATE TRIGGER trg_sip_registrations_updated_at BEFORE UPDATE ON sip_registrations FOR EACH ROW EXECUTE FUNCTION update_updated_at();
CREATE TRIGGER trg_sip_dialogs_updated_at BEFORE UPDATE ON sip_dialogs FOR EACH ROW EXECUTE FUNCTION update_updated_at();
CREATE TRIGGER trg_sip_subscriptions_updated_at BEFORE UPDATE ON sip_subscriptions FOR EACH ROW EXECUTE FUNCTION update_updated_at();
CREATE TRIGGER trg_calls_updated_at BEFORE UPDATE ON calls FOR EACH ROW EXECUTE FUNCTION update_updated_at();
CREATE TRIGGER trg_routing_rules_updated_at BEFORE UPDATE ON routing_rules FOR EACH ROW EXECUTE FUNCTION update_updated_at();
