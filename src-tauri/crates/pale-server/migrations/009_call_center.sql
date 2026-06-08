-- Migration 009: Full Call Center Module
-- Agent management, supervisor monitoring, QA, wallboard, SLA

-- ============================================================================
-- AGENT PROFILES (extends queue_agents with roles and detailed state)
-- ============================================================================

CREATE TABLE IF NOT EXISTS agent_profiles (
    id              UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    user_sip_uri    TEXT NOT NULL UNIQUE,
    role            TEXT NOT NULL DEFAULT 'agent',     -- agent, supervisor, qa, admin
    display_name    TEXT NOT NULL DEFAULT '',
    queues          JSONB NOT NULL DEFAULT '[]',       -- queue IDs this agent belongs to
    skills          JSONB NOT NULL DEFAULT '[]',       -- skill tags for skills-based routing
    max_concurrent  INTEGER NOT NULL DEFAULT 1,        -- max simultaneous calls
    auto_answer     BOOLEAN NOT NULL DEFAULT false,    -- auto-answer queue calls
    state           TEXT NOT NULL DEFAULT 'offline',   -- available, on_call, wrap_up, break, training, offline
    state_reason    TEXT,                               -- custom reason for break/offline
    state_since     TIMESTAMPTZ NOT NULL DEFAULT now(),
    total_calls     INTEGER NOT NULL DEFAULT 0,
    total_talk_secs BIGINT NOT NULL DEFAULT 0,
    total_hold_secs BIGINT NOT NULL DEFAULT 0,
    total_wrap_secs BIGINT NOT NULL DEFAULT 0,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);

DO $$ BEGIN
  ALTER TABLE agent_profiles ADD CONSTRAINT chk_agent_role
    CHECK (role IN ('agent', 'supervisor', 'qa', 'admin'));
EXCEPTION WHEN duplicate_object THEN NULL;
END $$;

DO $$ BEGIN
  ALTER TABLE agent_profiles ADD CONSTRAINT chk_agent_state
    CHECK (state IN ('available', 'on_call', 'wrap_up', 'break', 'training', 'meeting', 'offline'));
EXCEPTION WHEN duplicate_object THEN NULL;
END $$;

-- ============================================================================
-- AGENT STATE HISTORY (track every state change for reporting)
-- ============================================================================

CREATE TABLE IF NOT EXISTS agent_state_log (
    id              UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    agent_uri       TEXT NOT NULL,
    previous_state  TEXT NOT NULL,
    new_state       TEXT NOT NULL,
    reason          TEXT,
    duration_secs   INTEGER NOT NULL DEFAULT 0,       -- time spent in previous state
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_agent_state_log_agent ON agent_state_log (agent_uri, created_at DESC);

-- ============================================================================
-- QUEUE METRICS (real-time and historical snapshots)
-- ============================================================================

CREATE TABLE IF NOT EXISTS queue_metrics (
    id              UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    queue_id        UUID NOT NULL REFERENCES call_queues(id) ON DELETE CASCADE,
    snapshot_at     TIMESTAMPTZ NOT NULL DEFAULT now(),
    calls_waiting   INTEGER NOT NULL DEFAULT 0,
    calls_active    INTEGER NOT NULL DEFAULT 0,
    agents_available INTEGER NOT NULL DEFAULT 0,
    agents_busy     INTEGER NOT NULL DEFAULT 0,
    agents_paused   INTEGER NOT NULL DEFAULT 0,
    longest_wait_secs INTEGER NOT NULL DEFAULT 0,
    avg_wait_secs   INTEGER NOT NULL DEFAULT 0,
    avg_talk_secs   INTEGER NOT NULL DEFAULT 0,
    calls_answered  INTEGER NOT NULL DEFAULT 0,
    calls_abandoned INTEGER NOT NULL DEFAULT 0,
    sla_percentage  REAL NOT NULL DEFAULT 100.0,      -- % answered within SLA target
    sla_target_secs INTEGER NOT NULL DEFAULT 20       -- SLA target (answer within X seconds)
);

CREATE INDEX IF NOT EXISTS idx_queue_metrics_queue ON queue_metrics (queue_id, snapshot_at DESC);

-- ============================================================================
-- MONITOR SESSIONS (supervisor listen/whisper/barge)
-- ============================================================================

-- Already created in migration 008, enhance it:
ALTER TABLE monitor_sessions ADD COLUMN IF NOT EXISTS agent_uri TEXT;
ALTER TABLE monitor_sessions ADD COLUMN IF NOT EXISTS queue_id UUID;
ALTER TABLE monitor_sessions ADD COLUMN IF NOT EXISTS ended_at TIMESTAMPTZ;
ALTER TABLE monitor_sessions ADD COLUMN IF NOT EXISTS notes TEXT;

-- ============================================================================
-- QA SCORECARDS
-- ============================================================================

CREATE TABLE IF NOT EXISTS qa_scorecards (
    id              UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    call_id         TEXT NOT NULL,
    agent_uri       TEXT NOT NULL,
    reviewer_uri    TEXT NOT NULL,
    queue_name      TEXT,
    scores          JSONB NOT NULL DEFAULT '{}',       -- {"greeting": 5, "resolution": 4, "professionalism": 5}
    total_score     REAL NOT NULL DEFAULT 0,
    max_score       REAL NOT NULL DEFAULT 0,
    comments        TEXT NOT NULL DEFAULT '',
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_qa_scorecards_agent ON qa_scorecards (agent_uri);
CREATE INDEX IF NOT EXISTS idx_qa_scorecards_reviewer ON qa_scorecards (reviewer_uri);

-- ============================================================================
-- CANNED RESPONSES (for agents in chat/queue)
-- ============================================================================

CREATE TABLE IF NOT EXISTS canned_responses (
    id              UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    category        TEXT NOT NULL DEFAULT 'general',
    shortcode       TEXT NOT NULL,                     -- e.g. "/greeting", "/hold"
    title           TEXT NOT NULL,
    body            TEXT NOT NULL,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_canned_responses_category ON canned_responses (category);

-- ============================================================================
-- TRIGGERS
-- ============================================================================

DROP TRIGGER IF EXISTS trg_agent_profiles_updated_at ON agent_profiles;
CREATE TRIGGER trg_agent_profiles_updated_at BEFORE UPDATE ON agent_profiles FOR EACH ROW EXECUTE FUNCTION update_updated_at();
