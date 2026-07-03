-- Approvals workflow
CREATE TABLE IF NOT EXISTS approval_requests (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    title       TEXT NOT NULL DEFAULT '',
    description TEXT NOT NULL DEFAULT '',
    requestor   TEXT NOT NULL DEFAULT '',
    approvers   TEXT[] NOT NULL DEFAULT '{}',
    status      TEXT NOT NULL DEFAULT 'pending',
    responses   JSONB NOT NULL DEFAULT '[]',
    room_id     UUID,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    resolved_at TIMESTAMPTZ
);
CREATE INDEX IF NOT EXISTS idx_approval_requests_room ON approval_requests (room_id);
CREATE INDEX IF NOT EXISTS idx_approval_requests_status ON approval_requests (status);
