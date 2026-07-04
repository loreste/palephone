-- Wiki / knowledge base per team
CREATE TABLE IF NOT EXISTS wiki_pages (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    team_id     UUID NOT NULL,
    title       TEXT NOT NULL,
    body        TEXT NOT NULL DEFAULT '',
    created_by  TEXT NOT NULL,
    updated_by  TEXT NOT NULL,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    parent_id   UUID
);
CREATE INDEX IF NOT EXISTS idx_wiki_pages_team ON wiki_pages (team_id);
CREATE INDEX IF NOT EXISTS idx_wiki_pages_parent ON wiki_pages (parent_id);
