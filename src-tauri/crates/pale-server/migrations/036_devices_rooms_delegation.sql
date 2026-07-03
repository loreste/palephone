-- Shared line appearance / boss-secretary delegation
CREATE TABLE IF NOT EXISTS line_delegations (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    owner_uri       TEXT NOT NULL,
    delegate_uri    TEXT NOT NULL,
    can_answer      BOOLEAN NOT NULL DEFAULT true,
    can_make        BOOLEAN NOT NULL DEFAULT false,
    can_view_history BOOLEAN NOT NULL DEFAULT false,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX IF NOT EXISTS idx_line_delegations_owner ON line_delegations (owner_uri);
CREATE INDEX IF NOT EXISTS idx_line_delegations_delegate ON line_delegations (delegate_uri);
CREATE UNIQUE INDEX IF NOT EXISTS idx_line_delegations_pair ON line_delegations (owner_uri, delegate_uri);

-- Common area phone profiles
CREATE TABLE IF NOT EXISTS common_area_phones (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name        TEXT NOT NULL,
    extension   TEXT NOT NULL,
    location    TEXT NOT NULL DEFAULT '',
    features    JSONB NOT NULL DEFAULT '{}',
    enabled     BOOLEAN NOT NULL DEFAULT true,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Meeting rooms and room bookings
CREATE TABLE IF NOT EXISTS meeting_rooms (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name        TEXT NOT NULL,
    location    TEXT NOT NULL DEFAULT '',
    capacity    INT NOT NULL DEFAULT 0,
    equipment   TEXT[] NOT NULL DEFAULT '{}',
    bookable    BOOLEAN NOT NULL DEFAULT true,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS room_bookings (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    room_id     UUID NOT NULL REFERENCES meeting_rooms(id) ON DELETE CASCADE,
    meeting_id  UUID,
    booked_by   TEXT NOT NULL,
    start_time  TIMESTAMPTZ NOT NULL,
    end_time    TIMESTAMPTZ NOT NULL,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX IF NOT EXISTS idx_room_bookings_room ON room_bookings (room_id);
CREATE INDEX IF NOT EXISTS idx_room_bookings_time ON room_bookings (start_time, end_time);

-- SIP phone provisioning
CREATE TABLE IF NOT EXISTS provisioned_devices (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    mac_address     TEXT NOT NULL UNIQUE,
    model           TEXT NOT NULL DEFAULT '',
    assigned_user   TEXT,
    config_template TEXT NOT NULL DEFAULT '',
    provisioned_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    last_seen       TIMESTAMPTZ
);
CREATE INDEX IF NOT EXISTS idx_provisioned_devices_mac ON provisioned_devices (mac_address);

-- Hot desking sessions
CREATE TABLE IF NOT EXISTS hotdesk_sessions (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    device_id       UUID NOT NULL REFERENCES provisioned_devices(id) ON DELETE CASCADE,
    user_uri        TEXT NOT NULL,
    logged_in_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    logged_out_at   TIMESTAMPTZ
);
CREATE INDEX IF NOT EXISTS idx_hotdesk_sessions_device ON hotdesk_sessions (device_id);
CREATE INDEX IF NOT EXISTS idx_hotdesk_sessions_active ON hotdesk_sessions (device_id) WHERE logged_out_at IS NULL;
