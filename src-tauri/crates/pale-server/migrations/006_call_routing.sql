-- Migration 006: Ring Groups, IVR, Enhanced Call Routing

-- ============================================================================
-- RING GROUPS
-- ============================================================================

CREATE TABLE IF NOT EXISTS ring_groups (
    id              UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    name            TEXT NOT NULL,
    extension       TEXT NOT NULL UNIQUE,       -- e.g. "sip:sales@pale.local"
    strategy        TEXT NOT NULL DEFAULT 'simultaneous',  -- simultaneous, sequential, random
    ring_timeout    INTEGER NOT NULL DEFAULT 30,  -- seconds before moving to next or failing
    members         JSONB NOT NULL DEFAULT '[]',  -- array of SIP URIs
    fallback_uri    TEXT,                         -- where to route if nobody answers
    enabled         BOOLEAN NOT NULL DEFAULT true,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_ring_groups_extension ON ring_groups (extension);

DO $$ BEGIN
  ALTER TABLE ring_groups ADD CONSTRAINT chk_ring_strategy
    CHECK (strategy IN ('simultaneous', 'sequential', 'random'));
EXCEPTION WHEN duplicate_object THEN NULL;
END $$;

-- ============================================================================
-- IVR (Interactive Voice Response / Auto-Attendant)
-- ============================================================================

CREATE TABLE IF NOT EXISTS ivrs (
    id              UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    name            TEXT NOT NULL,
    extension       TEXT NOT NULL UNIQUE,       -- e.g. "sip:main@pale.local"
    greeting_text   TEXT NOT NULL DEFAULT 'Welcome. Press 1 for sales, 2 for support.',
    greeting_file_id UUID REFERENCES files(id) ON DELETE SET NULL,
    timeout_secs    INTEGER NOT NULL DEFAULT 10,  -- wait time for DTMF
    max_retries     INTEGER NOT NULL DEFAULT 3,
    invalid_destination TEXT,                     -- where to route on invalid input
    timeout_destination TEXT,                     -- where to route on timeout
    enabled         BOOLEAN NOT NULL DEFAULT true,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_ivrs_extension ON ivrs (extension);

CREATE TABLE IF NOT EXISTS ivr_options (
    id              UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    ivr_id          UUID NOT NULL REFERENCES ivrs(id) ON DELETE CASCADE,
    digit           TEXT NOT NULL,               -- "1", "2", "0", "*", "#"
    label           TEXT NOT NULL DEFAULT '',     -- "Sales", "Support"
    destination     TEXT NOT NULL,               -- SIP URI: user, group, or another IVR
    destination_type TEXT NOT NULL DEFAULT 'user', -- user, ring_group, ivr, voicemail
    UNIQUE (ivr_id, digit)
);

CREATE INDEX IF NOT EXISTS idx_ivr_options_ivr ON ivr_options (ivr_id);

DO $$ BEGIN
  ALTER TABLE ivr_options ADD CONSTRAINT chk_ivr_dest_type
    CHECK (destination_type IN ('user', 'ring_group', 'ivr', 'voicemail', 'external'));
EXCEPTION WHEN duplicate_object THEN NULL;
END $$;

-- ============================================================================
-- ENHANCED ROUTING: Add destination_type to routing_rules
-- ============================================================================

ALTER TABLE routing_rules ADD COLUMN IF NOT EXISTS destination_type TEXT NOT NULL DEFAULT 'user';

DO $$ BEGIN
  ALTER TABLE routing_rules ADD CONSTRAINT chk_routing_dest_type
    CHECK (destination_type IN ('user', 'ring_group', 'ivr', 'voicemail', 'external'));
EXCEPTION WHEN duplicate_object THEN NULL;
END $$;

-- ============================================================================
-- TRIGGERS
-- ============================================================================

DROP TRIGGER IF EXISTS trg_ring_groups_updated_at ON ring_groups;
CREATE TRIGGER trg_ring_groups_updated_at BEFORE UPDATE ON ring_groups FOR EACH ROW EXECUTE FUNCTION update_updated_at();

DROP TRIGGER IF EXISTS trg_ivrs_updated_at ON ivrs;
CREATE TRIGGER trg_ivrs_updated_at BEFORE UPDATE ON ivrs FOR EACH ROW EXECUTE FUNCTION update_updated_at();
