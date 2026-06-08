-- Migration 008: Core PBX Features
-- Call Queues, Music on Hold, Business Hours, Holidays, Extensions, Call Park, Speed Dial

-- ============================================================================
-- EXTENSIONS (short dial codes mapped to users/groups/IVRs)
-- ============================================================================

CREATE TABLE IF NOT EXISTS extensions (
    extension       TEXT PRIMARY KEY,          -- e.g. "1001", "300"
    destination     TEXT NOT NULL,             -- SIP URI
    destination_type TEXT NOT NULL DEFAULT 'user',  -- user, ring_group, ivr, queue, park, voicemail
    label           TEXT NOT NULL DEFAULT '',
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);

DO $$ BEGIN
  ALTER TABLE extensions ADD CONSTRAINT chk_ext_dest_type
    CHECK (destination_type IN ('user', 'ring_group', 'ivr', 'queue', 'park', 'voicemail', 'external', 'conference'));
EXCEPTION WHEN duplicate_object THEN NULL;
END $$;

-- ============================================================================
-- CALL QUEUES (ACD — Automatic Call Distribution)
-- ============================================================================

CREATE TABLE IF NOT EXISTS call_queues (
    id                  UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    name                TEXT NOT NULL,
    extension           TEXT NOT NULL UNIQUE,
    strategy            TEXT NOT NULL DEFAULT 'round_robin',  -- round_robin, longest_idle, ring_all, random, skills_based
    max_wait_time       INTEGER NOT NULL DEFAULT 300,         -- seconds before overflow
    max_queue_size      INTEGER NOT NULL DEFAULT 50,
    wrap_up_time        INTEGER NOT NULL DEFAULT 10,          -- seconds between calls for agent
    announce_position   BOOLEAN NOT NULL DEFAULT true,
    announce_interval   INTEGER NOT NULL DEFAULT 30,          -- seconds between position announcements
    hold_music_file_id  UUID REFERENCES files(id) ON DELETE SET NULL,
    join_announcement_file_id UUID REFERENCES files(id) ON DELETE SET NULL,
    overflow_destination TEXT,                                 -- where to route on queue full/timeout
    enabled             BOOLEAN NOT NULL DEFAULT true,
    created_at          TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at          TIMESTAMPTZ NOT NULL DEFAULT now()
);

DO $$ BEGIN
  ALTER TABLE call_queues ADD CONSTRAINT chk_queue_strategy
    CHECK (strategy IN ('round_robin', 'longest_idle', 'ring_all', 'random', 'skills_based'));
EXCEPTION WHEN duplicate_object THEN NULL;
END $$;

CREATE TABLE IF NOT EXISTS queue_agents (
    id              UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    queue_id        UUID NOT NULL REFERENCES call_queues(id) ON DELETE CASCADE,
    agent_uri       TEXT NOT NULL,
    priority        INTEGER NOT NULL DEFAULT 1,
    skills          JSONB NOT NULL DEFAULT '[]',
    state           TEXT NOT NULL DEFAULT 'available',  -- available, busy, wrap_up, paused, offline
    last_call_at    TIMESTAMPTZ,
    calls_handled   INTEGER NOT NULL DEFAULT 0,
    UNIQUE (queue_id, agent_uri)
);

CREATE INDEX IF NOT EXISTS idx_queue_agents_queue ON queue_agents (queue_id);
CREATE INDEX IF NOT EXISTS idx_queue_agents_state ON queue_agents (state) WHERE state = 'available';

DO $$ BEGIN
  ALTER TABLE queue_agents ADD CONSTRAINT chk_agent_state
    CHECK (state IN ('available', 'busy', 'wrap_up', 'paused', 'offline'));
EXCEPTION WHEN duplicate_object THEN NULL;
END $$;

CREATE TABLE IF NOT EXISTS queue_callers (
    id              UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    queue_id        UUID NOT NULL REFERENCES call_queues(id) ON DELETE CASCADE,
    caller_uri      TEXT NOT NULL,
    caller_name     TEXT NOT NULL DEFAULT '',
    position        INTEGER NOT NULL DEFAULT 0,
    entered_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    answered_at     TIMESTAMPTZ,
    answered_by     TEXT,
    status          TEXT NOT NULL DEFAULT 'waiting'  -- waiting, ringing, answered, abandoned, overflow
);

CREATE INDEX IF NOT EXISTS idx_queue_callers_queue ON queue_callers (queue_id, position);
CREATE INDEX IF NOT EXISTS idx_queue_callers_status ON queue_callers (status) WHERE status = 'waiting';

-- ============================================================================
-- MUSIC ON HOLD
-- ============================================================================

CREATE TABLE IF NOT EXISTS music_on_hold (
    id              UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    name            TEXT NOT NULL,
    is_default      BOOLEAN NOT NULL DEFAULT false,
    file_id         UUID NOT NULL REFERENCES files(id) ON DELETE CASCADE,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- ============================================================================
-- BUSINESS HOURS
-- ============================================================================

CREATE TABLE IF NOT EXISTS business_hours (
    id              UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    name            TEXT NOT NULL,
    timezone        TEXT NOT NULL DEFAULT 'America/New_York',
    schedule        JSONB NOT NULL DEFAULT '{}',  -- {"mon":{"open":"09:00","close":"17:00"},...}
    after_hours_destination TEXT,                  -- SIP URI, IVR, or voicemail
    after_hours_greeting_file_id UUID REFERENCES files(id) ON DELETE SET NULL,
    enabled         BOOLEAN NOT NULL DEFAULT true,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- ============================================================================
-- HOLIDAY CALENDAR
-- ============================================================================

CREATE TABLE IF NOT EXISTS holidays (
    id              UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    name            TEXT NOT NULL,
    date            DATE NOT NULL,
    recurring       BOOLEAN NOT NULL DEFAULT false,  -- repeats yearly
    greeting_file_id UUID REFERENCES files(id) ON DELETE SET NULL,
    destination     TEXT,                             -- where to route on holiday
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_holidays_date ON holidays (date);

-- ============================================================================
-- CALL PARK
-- ============================================================================

CREATE TABLE IF NOT EXISTS parked_calls (
    slot            TEXT PRIMARY KEY,               -- e.g. "701", "702"
    call_id         TEXT NOT NULL,
    parked_by       TEXT NOT NULL,
    caller_uri      TEXT NOT NULL,
    caller_name     TEXT NOT NULL DEFAULT '',
    parked_at       TIMESTAMPTZ NOT NULL DEFAULT now(),
    timeout_secs    INTEGER NOT NULL DEFAULT 120     -- ring back to parker after timeout
);

-- ============================================================================
-- SPEED DIAL
-- ============================================================================

CREATE TABLE IF NOT EXISTS speed_dials (
    id              UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    owner_uri       TEXT,                            -- NULL = system-wide
    code            TEXT NOT NULL,                    -- e.g. "1", "20"
    destination     TEXT NOT NULL,
    label           TEXT NOT NULL DEFAULT '',
    UNIQUE (owner_uri, code)
);

CREATE INDEX IF NOT EXISTS idx_speed_dials_owner ON speed_dials (owner_uri);

-- ============================================================================
-- CALL DETAIL RECORDS (CDR)
-- ============================================================================

CREATE TABLE IF NOT EXISTS call_detail_records (
    id              UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    call_id         TEXT,
    caller_uri      TEXT NOT NULL,
    callee_uri      TEXT NOT NULL,
    direction       TEXT NOT NULL DEFAULT 'inbound',
    start_time      TIMESTAMPTZ NOT NULL DEFAULT now(),
    answer_time     TIMESTAMPTZ,
    end_time        TIMESTAMPTZ,
    duration_secs   INTEGER NOT NULL DEFAULT 0,
    disposition     TEXT NOT NULL DEFAULT 'no_answer',  -- answered, no_answer, busy, failed, voicemail
    queue_name      TEXT,
    queue_wait_secs INTEGER,
    recorded        BOOLEAN NOT NULL DEFAULT false,
    recording_id    UUID,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_cdr_caller ON call_detail_records (caller_uri);
CREATE INDEX IF NOT EXISTS idx_cdr_callee ON call_detail_records (callee_uri);
CREATE INDEX IF NOT EXISTS idx_cdr_start_time ON call_detail_records (start_time DESC);
CREATE INDEX IF NOT EXISTS idx_cdr_disposition ON call_detail_records (disposition);

DO $$ BEGIN
  ALTER TABLE call_detail_records ADD CONSTRAINT chk_cdr_disposition
    CHECK (disposition IN ('answered', 'no_answer', 'busy', 'failed', 'voicemail', 'transferred', 'abandoned'));
EXCEPTION WHEN duplicate_object THEN NULL;
END $$;

-- ============================================================================
-- SUPERVISOR MONITORING
-- ============================================================================

CREATE TABLE IF NOT EXISTS monitor_sessions (
    id              UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    supervisor_uri  TEXT NOT NULL,
    target_call_id  TEXT NOT NULL,
    mode            TEXT NOT NULL DEFAULT 'listen',  -- listen, whisper, barge
    started_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);

DO $$ BEGIN
  ALTER TABLE monitor_sessions ADD CONSTRAINT chk_monitor_mode
    CHECK (mode IN ('listen', 'whisper', 'barge'));
EXCEPTION WHEN duplicate_object THEN NULL;
END $$;

-- ============================================================================
-- PAGING GROUPS
-- ============================================================================

CREATE TABLE IF NOT EXISTS paging_groups (
    id              UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    name            TEXT NOT NULL,
    extension       TEXT NOT NULL UNIQUE,
    members         JSONB NOT NULL DEFAULT '[]',
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- ============================================================================
-- TRIGGERS
-- ============================================================================

DROP TRIGGER IF EXISTS trg_call_queues_updated_at ON call_queues;
CREATE TRIGGER trg_call_queues_updated_at BEFORE UPDATE ON call_queues FOR EACH ROW EXECUTE FUNCTION update_updated_at();

DROP TRIGGER IF EXISTS trg_business_hours_updated_at ON business_hours;
CREATE TRIGGER trg_business_hours_updated_at BEFORE UPDATE ON business_hours FOR EACH ROW EXECUTE FUNCTION update_updated_at();
