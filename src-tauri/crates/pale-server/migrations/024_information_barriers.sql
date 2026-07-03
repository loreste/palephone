CREATE TABLE IF NOT EXISTS information_barriers (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name TEXT NOT NULL,
    segment1_name TEXT NOT NULL,
    segment1_users TEXT[] NOT NULL DEFAULT '{}',
    segment2_name TEXT NOT NULL,
    segment2_users TEXT[] NOT NULL DEFAULT '{}',
    block_chat BOOLEAN NOT NULL DEFAULT TRUE,
    block_call BOOLEAN NOT NULL DEFAULT TRUE,
    enabled BOOLEAN NOT NULL DEFAULT TRUE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
