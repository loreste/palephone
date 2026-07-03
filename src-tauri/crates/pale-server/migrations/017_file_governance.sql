-- Migration 017: File governance metadata for DLP, retention, and eDiscovery

ALTER TABLE files ADD COLUMN IF NOT EXISTS dlp_status TEXT NOT NULL DEFAULT 'clean';
ALTER TABLE files ADD COLUMN IF NOT EXISTS dlp_violation_count INTEGER NOT NULL DEFAULT 0;
ALTER TABLE files ADD COLUMN IF NOT EXISTS legal_hold BOOLEAN NOT NULL DEFAULT false;
ALTER TABLE files ADD COLUMN IF NOT EXISTS deleted_at TIMESTAMPTZ;
ALTER TABLE files ADD COLUMN IF NOT EXISTS deleted_by TEXT;

CREATE INDEX IF NOT EXISTS idx_files_legal_hold ON files (created_at DESC) WHERE legal_hold = true;
CREATE INDEX IF NOT EXISTS idx_files_deleted_at ON files (deleted_at) WHERE deleted_at IS NOT NULL;
