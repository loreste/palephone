-- Customer Key / BYOK encryption configuration
CREATE TABLE IF NOT EXISTS encryption_config (
    id            UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    key_id        TEXT NOT NULL DEFAULT '',
    key_source    TEXT NOT NULL DEFAULT 'server',
    wrapped_key_enc TEXT NOT NULL DEFAULT '',
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    rotated_at    TIMESTAMPTZ
);
