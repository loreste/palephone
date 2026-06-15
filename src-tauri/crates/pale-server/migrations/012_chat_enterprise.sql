-- Migration 012: Enterprise chat features
-- Threading, editing, pinning, reactions, favorites, user profiles

-- Message threading and editing
ALTER TABLE room_messages ADD COLUMN IF NOT EXISTS reply_to UUID REFERENCES room_messages(id);
ALTER TABLE room_messages ADD COLUMN IF NOT EXISTS edited_at TIMESTAMPTZ;
ALTER TABLE room_messages ADD COLUMN IF NOT EXISTS pinned BOOLEAN NOT NULL DEFAULT false;

CREATE INDEX IF NOT EXISTS idx_room_messages_reply_to ON room_messages (reply_to) WHERE reply_to IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_room_messages_pinned ON room_messages (room_id) WHERE pinned = true;

-- Reactions persistence
CREATE TABLE IF NOT EXISTS message_reactions (
    id          UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    message_id  UUID NOT NULL REFERENCES room_messages(id) ON DELETE CASCADE,
    user_uri    TEXT NOT NULL,
    emoji       TEXT NOT NULL,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (message_id, user_uri, emoji)
);
CREATE INDEX IF NOT EXISTS idx_message_reactions_message ON message_reactions (message_id);

-- Contact favorites
CREATE TABLE IF NOT EXISTS user_favorites (
    id           UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    user_uri     TEXT NOT NULL,
    favorite_uri TEXT NOT NULL,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (user_uri, favorite_uri)
);

-- User profile enrichment
ALTER TABLE users ADD COLUMN IF NOT EXISTS email TEXT;
ALTER TABLE users ADD COLUMN IF NOT EXISTS title TEXT;
ALTER TABLE users ADD COLUMN IF NOT EXISTS department TEXT;
ALTER TABLE users ADD COLUMN IF NOT EXISTS phone_number TEXT;
ALTER TABLE users ADD COLUMN IF NOT EXISTS status_message TEXT;
