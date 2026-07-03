-- 024: Chat enterprise parity (scheduled send, delivery status, tags, notification preferences)

-- Scheduled send columns on room_messages
ALTER TABLE room_messages
  ADD COLUMN IF NOT EXISTS scheduled_at TIMESTAMPTZ,
  ADD COLUMN IF NOT EXISTS delivered BOOLEAN NOT NULL DEFAULT TRUE;

-- Delivery status column on room_messages
ALTER TABLE room_messages
  ADD COLUMN IF NOT EXISTS delivery_status TEXT NOT NULL DEFAULT 'sent';

-- Tags for targeted communication
CREATE TABLE IF NOT EXISTS tags (
  id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  team_id UUID NOT NULL,
  name TEXT NOT NULL,
  members TEXT[] NOT NULL DEFAULT '{}',
  created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
  UNIQUE (team_id, name)
);

-- Per-channel notification preferences
CREATE TABLE IF NOT EXISTS notification_preferences (
  room_id UUID NOT NULL,
  user_uri TEXT NOT NULL,
  notification_level TEXT NOT NULL DEFAULT 'all',
  updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
  PRIMARY KEY (room_id, user_uri)
);
