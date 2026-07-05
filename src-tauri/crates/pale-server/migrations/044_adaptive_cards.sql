-- Adaptive cards / interactive structured messages
ALTER TABLE room_messages ADD COLUMN IF NOT EXISTS card_payload JSONB;
