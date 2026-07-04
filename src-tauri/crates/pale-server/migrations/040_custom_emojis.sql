-- Custom emoji / sticker packs per team
CREATE TABLE IF NOT EXISTS custom_emojis (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    team_id     UUID NOT NULL,
    shortcode   TEXT NOT NULL UNIQUE,
    image_url   TEXT NOT NULL,
    uploaded_by TEXT NOT NULL,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX IF NOT EXISTS idx_custom_emojis_team ON custom_emojis (team_id);
