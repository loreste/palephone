-- Personal call groups (ring multiple devices/numbers)
CREATE TABLE IF NOT EXISTS personal_call_groups (
    id            UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id       TEXT NOT NULL,
    name          TEXT NOT NULL DEFAULT '',
    numbers       TEXT[] NOT NULL DEFAULT '{}',
    ring_duration INT NOT NULL DEFAULT 20,
    enabled       BOOLEAN NOT NULL DEFAULT true
);
CREATE INDEX IF NOT EXISTS idx_personal_call_groups_user ON personal_call_groups (user_id);
