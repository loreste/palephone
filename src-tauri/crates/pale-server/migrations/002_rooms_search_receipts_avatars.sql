-- Migration 002: Group chat rooms, full-text search, read receipts, avatars
-- Depends on: 001_initial_schema.sql

-- ============================================================================
-- GROUP CHAT ROOMS (server-native, alongside Matrix rooms)
-- ============================================================================

CREATE TABLE IF NOT EXISTS rooms (
    id              UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    name            TEXT NOT NULL,
    description     TEXT NOT NULL DEFAULT '',
    is_direct       BOOLEAN NOT NULL DEFAULT false,
    created_by      TEXT NOT NULL,          -- SIP URI of creator
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_rooms_created_by ON rooms (created_by);

CREATE TABLE IF NOT EXISTS room_members (
    id              UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    room_id         UUID NOT NULL REFERENCES rooms(id) ON DELETE CASCADE,
    user_sip_uri    TEXT NOT NULL,
    role            TEXT NOT NULL DEFAULT 'member',  -- admin, member
    joined_at       TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (room_id, user_sip_uri)
);

CREATE INDEX IF NOT EXISTS idx_room_members_room ON room_members (room_id);
CREATE INDEX IF NOT EXISTS idx_room_members_user ON room_members (user_sip_uri);

CREATE TABLE IF NOT EXISTS room_messages (
    id              UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    room_id         UUID NOT NULL REFERENCES rooms(id) ON DELETE CASCADE,
    sender_uri      TEXT NOT NULL,
    body            TEXT NOT NULL DEFAULT '',
    content_type    TEXT NOT NULL DEFAULT 'text/plain',
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_room_messages_room ON room_messages (room_id, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_room_messages_sender ON room_messages (sender_uri);

-- ============================================================================
-- FULL-TEXT SEARCH on messages
-- ============================================================================

-- Add tsvector column for full-text search on sip_messages
ALTER TABLE sip_messages ADD COLUMN IF NOT EXISTS search_vector tsvector;

-- Populate search vector from body
CREATE OR REPLACE FUNCTION sip_messages_search_trigger() RETURNS trigger AS $$
BEGIN
    NEW.search_vector := to_tsvector('english', coalesce(NEW.body, ''));
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS trg_sip_messages_search ON sip_messages;
CREATE TRIGGER trg_sip_messages_search
    BEFORE INSERT OR UPDATE ON sip_messages
    FOR EACH ROW EXECUTE FUNCTION sip_messages_search_trigger();

CREATE INDEX IF NOT EXISTS idx_sip_messages_search ON sip_messages USING GIN (search_vector);

-- Same for room_messages
ALTER TABLE room_messages ADD COLUMN IF NOT EXISTS search_vector tsvector;

CREATE OR REPLACE FUNCTION room_messages_search_trigger() RETURNS trigger AS $$
BEGIN
    NEW.search_vector := to_tsvector('english', coalesce(NEW.body, ''));
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS trg_room_messages_search ON room_messages;
CREATE TRIGGER trg_room_messages_search
    BEFORE INSERT OR UPDATE ON room_messages
    FOR EACH ROW EXECUTE FUNCTION room_messages_search_trigger();

CREATE INDEX IF NOT EXISTS idx_room_messages_search ON room_messages USING GIN (search_vector);

-- ============================================================================
-- READ RECEIPTS
-- ============================================================================

CREATE TABLE IF NOT EXISTS message_reads (
    id              UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    message_id      UUID NOT NULL,         -- References sip_messages.id or room_messages.id
    message_source  TEXT NOT NULL DEFAULT 'sip',  -- 'sip' or 'room'
    reader_uri      TEXT NOT NULL,
    read_at         TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (message_id, reader_uri)
);

CREATE INDEX IF NOT EXISTS idx_message_reads_message ON message_reads (message_id);
CREATE INDEX IF NOT EXISTS idx_message_reads_reader ON message_reads (reader_uri);

-- ============================================================================
-- USER AVATARS
-- ============================================================================

ALTER TABLE users ADD COLUMN IF NOT EXISTS avatar_file_id UUID REFERENCES files(id) ON DELETE SET NULL;

-- ============================================================================
-- AUTO-UPDATE TRIGGERS for new tables
-- ============================================================================

CREATE TRIGGER trg_rooms_updated_at BEFORE UPDATE ON rooms FOR EACH ROW EXECUTE FUNCTION update_updated_at();
