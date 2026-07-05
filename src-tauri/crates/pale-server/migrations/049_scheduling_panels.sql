-- Scheduling panel devices for meeting rooms
CREATE TABLE IF NOT EXISTS scheduling_panels (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name TEXT NOT NULL,
    meeting_room_id UUID NOT NULL REFERENCES meeting_rooms(id) ON DELETE CASCADE,
    device_identifier TEXT UNIQUE NOT NULL,
    display_mode TEXT NOT NULL DEFAULT 'schedule',
    enabled BOOLEAN NOT NULL DEFAULT TRUE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_scheduling_panels_room ON scheduling_panels(meeting_room_id);
CREATE INDEX IF NOT EXISTS idx_scheduling_panels_device ON scheduling_panels(device_identifier);
