-- Migration 049: Multi-geo / data residency controls
CREATE TABLE IF NOT EXISTS data_residency_config (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    region TEXT NOT NULL UNIQUE,
    pg_connection_string_enc TEXT NOT NULL,
    file_storage_path TEXT NOT NULL,
    enabled BOOLEAN NOT NULL DEFAULT true,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Add data_region to users (if not already present)
DO $$ BEGIN
    ALTER TABLE users ADD COLUMN data_region TEXT NOT NULL DEFAULT 'default';
EXCEPTION
    WHEN duplicate_column THEN NULL;
END $$;
