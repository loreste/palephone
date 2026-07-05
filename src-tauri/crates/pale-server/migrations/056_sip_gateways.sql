-- SIP gateway / analog device management
CREATE TABLE IF NOT EXISTS sip_gateways (
    id UUID PRIMARY KEY,
    name TEXT NOT NULL,
    host TEXT NOT NULL,
    port INT NOT NULL DEFAULT 5060,
    transport TEXT NOT NULL DEFAULT 'udp',
    username TEXT,
    password_enc TEXT,
    prefix TEXT NOT NULL DEFAULT '',
    enabled BOOL NOT NULL DEFAULT true,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_sip_gateways_prefix ON sip_gateways(prefix);
