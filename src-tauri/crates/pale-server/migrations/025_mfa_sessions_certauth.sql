-- Migration 024: MFA/TOTP, session management, certificate auth
-- Adds enterprise security features for Teams parity

-- ─── TOTP MFA ───

CREATE TABLE IF NOT EXISTS totp_secrets (
    user_id     UUID PRIMARY KEY REFERENCES users(id) ON DELETE CASCADE,
    encrypted_secret TEXT NOT NULL,
    enabled     BOOLEAN NOT NULL DEFAULT false,
    backup_codes TEXT NOT NULL DEFAULT '[]',
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- ─── User Sessions (concurrent session tracking) ───

CREATE TABLE IF NOT EXISTS user_sessions (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id     UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    token_hash  TEXT NOT NULL UNIQUE,
    device_name TEXT NOT NULL DEFAULT 'Unknown',
    device_type TEXT NOT NULL DEFAULT 'desktop',
    ip_address  TEXT NOT NULL DEFAULT 'unknown',
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    last_active TIMESTAMPTZ NOT NULL DEFAULT now(),
    revoked     BOOLEAN NOT NULL DEFAULT false
);

CREATE INDEX IF NOT EXISTS idx_user_sessions_user_id ON user_sessions(user_id);
CREATE INDEX IF NOT EXISTS idx_user_sessions_token_hash ON user_sessions(token_hash);
