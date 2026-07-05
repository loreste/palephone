-- Reverse number lookup / CNAM cache
CREATE TABLE IF NOT EXISTS cnam_cache (
    id UUID PRIMARY KEY,
    phone_number TEXT UNIQUE NOT NULL,
    caller_name TEXT NOT NULL,
    source TEXT NOT NULL DEFAULT 'manual',
    cached_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    expires_at TIMESTAMPTZ
);

CREATE INDEX IF NOT EXISTS idx_cnam_cache_phone ON cnam_cache(phone_number);
