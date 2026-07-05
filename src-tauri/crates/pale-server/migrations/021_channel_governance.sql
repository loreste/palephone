-- Migration 020: Channel privacy, ownership, and moderation metadata

ALTER TABLE rooms ADD COLUMN IF NOT EXISTS channel_type TEXT NOT NULL DEFAULT 'standard';
ALTER TABLE rooms ADD COLUMN IF NOT EXISTS channel_owners TEXT[] NOT NULL DEFAULT '{}';
ALTER TABLE rooms ADD COLUMN IF NOT EXISTS posting_policy TEXT NOT NULL DEFAULT 'members';

CREATE INDEX IF NOT EXISTS idx_rooms_channel_type ON rooms (channel_type) WHERE team_id IS NOT NULL;
