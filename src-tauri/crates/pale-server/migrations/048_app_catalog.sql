-- Migration 048: App store / extension catalog
CREATE TABLE IF NOT EXISTS app_catalog (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name TEXT NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    category TEXT NOT NULL DEFAULT 'other',
    icon_url TEXT,
    manifest_url TEXT,
    installed BOOLEAN NOT NULL DEFAULT false,
    installed_by TEXT,
    installed_at TIMESTAMPTZ,
    version TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_app_catalog_category ON app_catalog(category);
CREATE INDEX IF NOT EXISTS idx_app_catalog_installed ON app_catalog(installed) WHERE installed = true;
