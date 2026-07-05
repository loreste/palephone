-- Spotlight state on conferences
ALTER TABLE conferences
    ADD COLUMN IF NOT EXISTS spotlight_participant_id UUID;

-- Green room flag on conferences
ALTER TABLE conferences
    ADD COLUMN IF NOT EXISTS green_room_enabled BOOLEAN NOT NULL DEFAULT FALSE;

-- Persistent meeting chat: link a chat room to a conference/meeting
ALTER TABLE conferences
    ADD COLUMN IF NOT EXISTS chat_room_id UUID;

-- Out-of-office auto-reply on users
ALTER TABLE users
    ADD COLUMN IF NOT EXISTS out_of_office_message TEXT,
    ADD COLUMN IF NOT EXISTS out_of_office_until TIMESTAMPTZ;
