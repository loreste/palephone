-- Migration 021: Saved messages and priority/urgent message metadata

ALTER TABLE room_messages ADD COLUMN IF NOT EXISTS priority TEXT NOT NULL DEFAULT 'normal';
ALTER TABLE room_messages ADD COLUMN IF NOT EXISTS saved_by TEXT[] NOT NULL DEFAULT '{}';

CREATE INDEX IF NOT EXISTS idx_room_messages_priority ON room_messages (room_id, priority, created_at DESC)
    WHERE priority <> 'normal';
CREATE INDEX IF NOT EXISTS idx_room_messages_saved_by ON room_messages USING GIN (saved_by);
