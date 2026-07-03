-- File versioning
CREATE TABLE IF NOT EXISTS file_versions (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    file_id     UUID NOT NULL,
    version_number INT NOT NULL DEFAULT 1,
    uploader    TEXT NOT NULL,
    size        BIGINT NOT NULL DEFAULT 0,
    sha256      TEXT NOT NULL DEFAULT '',
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    storage_path TEXT NOT NULL DEFAULT ''
);
CREATE INDEX IF NOT EXISTS idx_file_versions_file_id ON file_versions (file_id, version_number);

-- Folder structure per channel
CREATE TABLE IF NOT EXISTS folders (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    room_id     UUID NOT NULL,
    parent_id   UUID,
    name        TEXT NOT NULL DEFAULT '',
    created_by  TEXT NOT NULL DEFAULT '',
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX IF NOT EXISTS idx_folders_room_id ON folders (room_id);

-- Add folder_id to files
ALTER TABLE files ADD COLUMN IF NOT EXISTS folder_id UUID;

-- File locking / checkout
ALTER TABLE files ADD COLUMN IF NOT EXISTS locked_by TEXT;
ALTER TABLE files ADD COLUMN IF NOT EXISTS locked_at TIMESTAMPTZ;
