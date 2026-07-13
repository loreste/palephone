-- Internal file share links (Phase 1.18)

CREATE TABLE IF NOT EXISTS file_share_links (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    file_id         UUID NOT NULL,
    token           TEXT NOT NULL UNIQUE,
    created_by      TEXT NOT NULL,
    expires_at      TIMESTAMPTZ,
    revoked         BOOLEAN NOT NULL DEFAULT false,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_file_share_links_file ON file_share_links (file_id);
CREATE INDEX IF NOT EXISTS idx_file_share_links_token ON file_share_links (token) WHERE NOT revoked;
