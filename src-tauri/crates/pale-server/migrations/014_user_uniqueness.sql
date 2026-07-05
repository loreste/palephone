-- Migration 013: Enforce case-insensitive user SIP URI uniqueness.

CREATE UNIQUE INDEX IF NOT EXISTS idx_users_sip_uri_unique_ci
    ON users (lower(sip_uri));
