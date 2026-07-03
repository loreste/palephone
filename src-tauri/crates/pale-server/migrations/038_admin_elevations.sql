-- Privileged access management / just-in-time admin
CREATE TABLE IF NOT EXISTS admin_elevations (
    id            UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id       UUID NOT NULL,
    reason        TEXT NOT NULL DEFAULT '',
    granted_by    TEXT NOT NULL DEFAULT '',
    granted_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    expires_at    TIMESTAMPTZ NOT NULL,
    revoked_at    TIMESTAMPTZ
);
CREATE INDEX IF NOT EXISTS idx_admin_elevations_user ON admin_elevations (user_id);
CREATE INDEX IF NOT EXISTS idx_admin_elevations_expires ON admin_elevations (expires_at) WHERE revoked_at IS NULL;
