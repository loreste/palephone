-- Shared auth sessions for multi-API-node HA.
-- Bearer tokens are persisted so any pale-server replica can resolve them.
-- token_hash enables revoke-by-device without scanning all tokens.
-- Application always writes token_hash on insert (sha256 hex of token).

ALTER TABLE admin_sessions
    ADD COLUMN IF NOT EXISTS role TEXT NOT NULL DEFAULT '';

ALTER TABLE admin_sessions
    ADD COLUMN IF NOT EXISTS token_hash TEXT;

CREATE UNIQUE INDEX IF NOT EXISTS idx_admin_sessions_token_hash
    ON admin_sessions (token_hash)
    WHERE token_hash IS NOT NULL;

CREATE INDEX IF NOT EXISTS idx_admin_sessions_principal
    ON admin_sessions (principal);
