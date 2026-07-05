CREATE TABLE IF NOT EXISTS sensitivity_labels (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name TEXT NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    color TEXT NOT NULL DEFAULT '#6b7280',
    priority INT NOT NULL DEFAULT 0,
    encrypt_content BOOLEAN NOT NULL DEFAULT FALSE,
    restrict_sharing BOOLEAN NOT NULL DEFAULT FALSE,
    watermark BOOLEAN NOT NULL DEFAULT FALSE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

ALTER TABLE files
  ADD COLUMN IF NOT EXISTS sensitivity_label_id UUID REFERENCES sensitivity_labels(id);

ALTER TABLE room_messages
  ADD COLUMN IF NOT EXISTS sensitivity_label_id UUID REFERENCES sensitivity_labels(id);
