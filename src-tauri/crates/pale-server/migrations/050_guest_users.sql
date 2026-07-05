-- Guest access for teams.
-- This schema matches GuestUser in lib.rs.
CREATE TABLE IF NOT EXISTS guest_users (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    email TEXT UNIQUE NOT NULL,
    display_name TEXT NOT NULL,
    invited_by TEXT NOT NULL,
    team_id UUID NOT NULL,
    permissions JSONB NOT NULL DEFAULT '{}'::jsonb,
    token_hash TEXT NOT NULL DEFAULT '',
    expires_at TIMESTAMPTZ NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

ALTER TABLE guest_users ADD COLUMN IF NOT EXISTS token_hash TEXT NOT NULL DEFAULT '';

CREATE INDEX IF NOT EXISTS idx_guest_users_team ON guest_users(team_id);
CREATE INDEX IF NOT EXISTS idx_guest_users_token_hash ON guest_users(token_hash);
CREATE INDEX IF NOT EXISTS idx_guest_users_email ON guest_users(email);
