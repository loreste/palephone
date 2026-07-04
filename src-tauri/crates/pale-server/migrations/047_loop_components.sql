-- Migration 047: Loop components (live collaborative inline content)
CREATE TABLE IF NOT EXISTS loop_components (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    room_id UUID NOT NULL,
    component_type TEXT NOT NULL CHECK (component_type IN ('checklist', 'table', 'paragraph')),
    data JSONB NOT NULL DEFAULT '{}',
    created_by TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_loop_components_room_id
    ON loop_components (room_id, created_at);
