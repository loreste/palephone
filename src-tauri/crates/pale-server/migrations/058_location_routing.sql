-- Location-based PSTN routing
CREATE TABLE IF NOT EXISTS location_routing_rules (
    id UUID PRIMARY KEY,
    name TEXT NOT NULL,
    location_pattern TEXT NOT NULL,
    gateway_id UUID NOT NULL,
    priority INT NOT NULL DEFAULT 0,
    enabled BOOL NOT NULL DEFAULT true,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_location_routing_priority ON location_routing_rules(priority);
