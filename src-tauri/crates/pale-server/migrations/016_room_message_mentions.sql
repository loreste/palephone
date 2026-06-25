-- Migration 016: Structured room message mentions
-- Depends on: 015_business_collaboration.sql

ALTER TABLE room_messages ADD COLUMN IF NOT EXISTS mentions JSONB NOT NULL DEFAULT '[]'::jsonb;
ALTER TABLE room_messages ADD COLUMN IF NOT EXISTS mentioned_user_uris JSONB NOT NULL DEFAULT '[]'::jsonb;

CREATE INDEX IF NOT EXISTS idx_room_messages_mentioned_user_uris
    ON room_messages USING GIN (mentioned_user_uris);
