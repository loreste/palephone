-- Migration 014: Persist room call metadata
-- Depends on: 013_comprehensive_routing.sql

ALTER TABLE rooms ADD COLUMN IF NOT EXISTS conference_id UUID REFERENCES conferences(id) ON DELETE SET NULL;
ALTER TABLE rooms ADD COLUMN IF NOT EXISTS call_uri TEXT;

CREATE INDEX IF NOT EXISTS idx_rooms_conference_id ON rooms (conference_id) WHERE conference_id IS NOT NULL;
