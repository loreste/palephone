-- Migration 015: Teams-style business collaboration records
-- Depends on: 014_room_call_metadata.sql

ALTER TABLE rooms ADD COLUMN IF NOT EXISTS team_id UUID;
ALTER TABLE rooms ADD COLUMN IF NOT EXISTS channel_name TEXT;

CREATE INDEX IF NOT EXISTS idx_rooms_team_id ON rooms (team_id) WHERE team_id IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_rooms_channel_name ON rooms (lower(channel_name)) WHERE channel_name IS NOT NULL;

CREATE TABLE IF NOT EXISTS business_objects (
    collection  TEXT NOT NULL,
    object_key  TEXT NOT NULL,
    json        JSONB NOT NULL,
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (collection, object_key)
);

CREATE INDEX IF NOT EXISTS idx_business_objects_collection_updated
    ON business_objects (collection, updated_at DESC);
