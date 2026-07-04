-- Migration 051: Digital signage
CREATE TABLE IF NOT EXISTS signage_displays (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name TEXT NOT NULL,
    location TEXT NOT NULL DEFAULT '',
    content_url TEXT NOT NULL DEFAULT '',
    schedule JSONB NOT NULL DEFAULT '{}',
    enabled BOOLEAN NOT NULL DEFAULT true,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
