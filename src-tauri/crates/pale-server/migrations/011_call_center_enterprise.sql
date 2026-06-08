-- Migration 011: Enterprise Call Center Module
-- Virtual queue callbacks, VIP routing, penalty-based agent tiers, SLA targets

-- Virtual queue callbacks: callers preserve position and get called back
CREATE TABLE IF NOT EXISTS queue_callbacks (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    queue_id UUID NOT NULL REFERENCES call_queues(id) ON DELETE CASCADE,
    caller_uri TEXT NOT NULL,
    caller_name TEXT DEFAULT '',
    callback_number TEXT NOT NULL,
    position INTEGER DEFAULT 0,
    status TEXT DEFAULT 'pending',
    requested_at TIMESTAMPTZ DEFAULT now(),
    scheduled_at TIMESTAMPTZ,
    attempted_at TIMESTAMPTZ,
    completed_at TIMESTAMPTZ,
    attempts INTEGER DEFAULT 0,
    max_attempts INTEGER DEFAULT 3
);
CREATE INDEX IF NOT EXISTS idx_queue_callbacks_status ON queue_callbacks (status) WHERE status = 'pending';
CREATE INDEX IF NOT EXISTS idx_queue_callbacks_queue ON queue_callbacks (queue_id, status);

-- VIP callers: priority routing with agent/queue overrides
CREATE TABLE IF NOT EXISTS vip_callers (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    caller_pattern TEXT NOT NULL UNIQUE,
    priority INTEGER DEFAULT 10,
    label TEXT DEFAULT '',
    queue_override TEXT,
    agent_override TEXT,
    created_at TIMESTAMPTZ DEFAULT now()
);

-- Agent penalty tiers for weighted distribution
ALTER TABLE queue_agents ADD COLUMN IF NOT EXISTS penalty INTEGER DEFAULT 0;

-- Queue callback and SLA settings
ALTER TABLE call_queues ADD COLUMN IF NOT EXISTS callback_enabled BOOLEAN DEFAULT false;
ALTER TABLE call_queues ADD COLUMN IF NOT EXISTS callback_threshold_secs INTEGER DEFAULT 120;
ALTER TABLE call_queues ADD COLUMN IF NOT EXISTS sla_target_secs INTEGER DEFAULT 20;
