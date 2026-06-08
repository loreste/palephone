-- Migration 010: Link extensions to users
-- Adds nullable FK from extensions to users for unified provisioning.
-- ON DELETE SET NULL preserves extensions when users are deleted (shows "Unassigned").

ALTER TABLE extensions ADD COLUMN IF NOT EXISTS user_id UUID REFERENCES users(id) ON DELETE SET NULL;

CREATE INDEX IF NOT EXISTS idx_extensions_user_id ON extensions (user_id) WHERE user_id IS NOT NULL;

-- Backfill: match existing user-type extensions to users by SIP URI
UPDATE extensions e
SET user_id = u.id
FROM users u
WHERE e.destination_type = 'user'
  AND e.user_id IS NULL
  AND (e.destination = u.sip_uri OR e.destination = 'sip:' || u.sip_uri);
