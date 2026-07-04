-- Migration 050: Bandwidth management / call admission control
CREATE TABLE IF NOT EXISTS bandwidth_policies (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name TEXT NOT NULL,
    max_concurrent_calls INT NOT NULL DEFAULT 0,
    max_bandwidth_kbps INT NOT NULL DEFAULT 0,
    location_pattern TEXT NOT NULL DEFAULT '*',
    enabled BOOLEAN NOT NULL DEFAULT true,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_bandwidth_policies_enabled
    ON bandwidth_policies(enabled) WHERE enabled = true;
