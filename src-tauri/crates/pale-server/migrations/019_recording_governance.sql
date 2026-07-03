-- Migration 019: Recording governance metadata for retention and eDiscovery

ALTER TABLE call_recordings ADD COLUMN IF NOT EXISTS conference_id UUID;
ALTER TABLE call_recordings ADD COLUMN IF NOT EXISTS transcript_segment_count INTEGER NOT NULL DEFAULT 0;
ALTER TABLE call_recordings ADD COLUMN IF NOT EXISTS legal_hold BOOLEAN NOT NULL DEFAULT false;
ALTER TABLE call_recordings ADD COLUMN IF NOT EXISTS deleted_at TIMESTAMPTZ;
ALTER TABLE call_recordings ADD COLUMN IF NOT EXISTS deleted_by TEXT;

CREATE INDEX IF NOT EXISTS idx_call_recordings_conference ON call_recordings (conference_id) WHERE conference_id IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_call_recordings_legal_hold ON call_recordings (created_at DESC) WHERE legal_hold = true;
CREATE INDEX IF NOT EXISTS idx_call_recordings_deleted_at ON call_recordings (deleted_at) WHERE deleted_at IS NOT NULL;
