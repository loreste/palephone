-- Migration 046: Channel tabs (embedded web apps)
CREATE TABLE IF NOT EXISTS channel_tabs (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    room_id UUID NOT NULL,
    name TEXT NOT NULL,
    url TEXT NOT NULL,
    icon TEXT,
    created_by TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    position INT NOT NULL DEFAULT 0
);

CREATE INDEX IF NOT EXISTS idx_channel_tabs_room ON channel_tabs(room_id);
