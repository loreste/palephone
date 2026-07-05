-- Migration 046: Federated cross-organization chat
CREATE TABLE IF NOT EXISTS federation_peers (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    domain TEXT NOT NULL UNIQUE,
    server_url TEXT NOT NULL,
    shared_key_enc TEXT NOT NULL,
    enabled BOOLEAN NOT NULL DEFAULT true,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS federated_messages (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    from_domain TEXT NOT NULL,
    from_user TEXT NOT NULL,
    to_domain TEXT NOT NULL,
    to_user TEXT NOT NULL,
    body TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_federated_messages_to_user
    ON federated_messages (to_user, created_at DESC);

CREATE INDEX IF NOT EXISTS idx_federated_messages_from_user
    ON federated_messages (from_user, created_at DESC);
