-- 036: Platform & Integration features
-- OAuth API clients, bots, calendar sync, contact sync, outbound connectors

-- ─── OAuth API Clients ───

CREATE TABLE IF NOT EXISTS api_clients (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name TEXT NOT NULL,
    client_id TEXT UNIQUE NOT NULL,
    client_secret_hash TEXT NOT NULL,
    scopes TEXT[] NOT NULL DEFAULT '{}',
    redirect_uris TEXT[] NOT NULL DEFAULT '{}',
    created_by TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS api_tokens (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    client_id UUID NOT NULL REFERENCES api_clients(id) ON DELETE CASCADE,
    user_uri TEXT,
    scopes TEXT[] NOT NULL DEFAULT '{}',
    token_hash TEXT NOT NULL,
    expires_at TIMESTAMPTZ NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_api_tokens_hash ON api_tokens(token_hash);
CREATE INDEX IF NOT EXISTS idx_api_tokens_client ON api_tokens(client_id);

-- ─── Bots ───

CREATE TABLE IF NOT EXISTS bots (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name TEXT NOT NULL,
    webhook_url TEXT NOT NULL,
    events TEXT[] NOT NULL DEFAULT '{}',
    owner_uri TEXT NOT NULL,
    api_token TEXT NOT NULL,
    enabled BOOLEAN NOT NULL DEFAULT true,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- ─── Calendar Integrations ───

CREATE TABLE IF NOT EXISTS calendar_integrations (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_uri TEXT NOT NULL,
    provider TEXT NOT NULL CHECK (provider IN ('google', 'exchange', 'caldav')),
    access_token_enc TEXT NOT NULL,
    refresh_token_enc TEXT,
    calendar_id TEXT,
    enabled BOOLEAN NOT NULL DEFAULT true,
    last_sync TIMESTAMPTZ
);

CREATE INDEX IF NOT EXISTS idx_calendar_integrations_user ON calendar_integrations(user_uri);

-- ─── Contact Sync ───

CREATE TABLE IF NOT EXISTS contact_sync_configs (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_uri TEXT NOT NULL,
    provider TEXT NOT NULL,
    access_token_enc TEXT NOT NULL,
    last_sync TIMESTAMPTZ,
    enabled BOOLEAN NOT NULL DEFAULT true
);

CREATE INDEX IF NOT EXISTS idx_contact_sync_user ON contact_sync_configs(user_uri);

CREATE TABLE IF NOT EXISTS synced_contacts (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_uri TEXT NOT NULL,
    name TEXT NOT NULL,
    email TEXT,
    phone TEXT,
    source TEXT NOT NULL,
    external_id TEXT,
    synced_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_synced_contacts_user ON synced_contacts(user_uri);

-- ─── Outbound Connectors ───

CREATE TABLE IF NOT EXISTS connectors (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name TEXT NOT NULL,
    type TEXT NOT NULL,
    webhook_url TEXT NOT NULL,
    events TEXT[] NOT NULL DEFAULT '{}',
    auth_header TEXT,
    enabled BOOLEAN NOT NULL DEFAULT true,
    created_by TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
