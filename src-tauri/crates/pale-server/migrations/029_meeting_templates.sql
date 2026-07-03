CREATE TABLE IF NOT EXISTS meeting_templates (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name TEXT NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    default_lobby BOOLEAN NOT NULL DEFAULT FALSE,
    default_mute_on_join BOOLEAN NOT NULL DEFAULT FALSE,
    default_allow_reactions BOOLEAN NOT NULL DEFAULT TRUE,
    default_recording BOOLEAN NOT NULL DEFAULT FALSE,
    max_participants INT,
    allowed_roles TEXT[] NOT NULL DEFAULT '{}',
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    created_by TEXT NOT NULL DEFAULT ''
);
