-- Migration 047: Message extensions / compose-area action commands
CREATE TABLE IF NOT EXISTS message_extensions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name TEXT NOT NULL,
    command TEXT UNIQUE NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    handler_url TEXT NOT NULL,
    icon TEXT,
    enabled BOOLEAN NOT NULL DEFAULT true,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_message_extensions_command ON message_extensions(command);
