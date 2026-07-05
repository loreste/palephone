-- Migration 064: Message threading support
CREATE TABLE IF NOT EXISTS message_threads (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    room_id UUID NOT NULL REFERENCES rooms(id),
    root_message_id UUID NOT NULL,
    reply_count INT NOT NULL DEFAULT 0,
    last_reply_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    participants TEXT[] NOT NULL DEFAULT '{}',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_message_threads_room ON message_threads(room_id);
CREATE INDEX IF NOT EXISTS idx_message_threads_root ON message_threads(root_message_id);

-- Add thread_id to room_messages
DO $$ BEGIN
    ALTER TABLE room_messages ADD COLUMN thread_id UUID REFERENCES message_threads(id);
EXCEPTION
    WHEN duplicate_column THEN NULL;
END $$;

CREATE INDEX IF NOT EXISTS idx_room_messages_thread ON room_messages(thread_id) WHERE thread_id IS NOT NULL;
