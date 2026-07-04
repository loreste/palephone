-- Guest access for teams
CREATE TABLE IF NOT EXISTS guest_users (
    id UUID PRIMARY KEY,
    email TEXT NOT NULL,
    display_name TEXT NOT NULL,
    invited_by TEXT NOT NULL,
    team_id UUID NOT NULL,
    permissions JSONB DEFAULT '{}'::jsonb,
    token TEXT NOT NULL,
    expires_at TIMESTAMPTZ NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_guest_users_team ON guest_users(team_id);
CREATE INDEX IF NOT EXISTS idx_guest_users_token ON guest_users(token);
CREATE INDEX IF NOT EXISTS idx_guest_users_email ON guest_users(email);
