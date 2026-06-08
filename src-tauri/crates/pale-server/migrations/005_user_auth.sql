-- Migration 005: Add password_hash and role to users table

ALTER TABLE users ADD COLUMN IF NOT EXISTS password_hash TEXT;
ALTER TABLE users ADD COLUMN IF NOT EXISTS role TEXT NOT NULL DEFAULT 'user';

DO $$ BEGIN
  ALTER TABLE users ADD CONSTRAINT chk_user_role CHECK (role IN ('admin', 'user'));
EXCEPTION WHEN duplicate_object THEN NULL;
END $$;
