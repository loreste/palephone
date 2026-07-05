-- Policy-based compliance recording
CREATE TABLE IF NOT EXISTS recording_policies (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name        TEXT NOT NULL DEFAULT '',
    trigger     TEXT NOT NULL DEFAULT 'all_calls',
    target_ids  TEXT[] NOT NULL DEFAULT '{}',
    enabled     BOOLEAN NOT NULL DEFAULT true,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Configurable music on hold
CREATE TABLE IF NOT EXISTS hold_music (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name        TEXT NOT NULL DEFAULT '',
    file_path   TEXT NOT NULL DEFAULT '',
    queue_id    UUID,
    is_default  BOOLEAN NOT NULL DEFAULT false,
    uploaded_by TEXT NOT NULL DEFAULT '',
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);
