-- Migration 003: Voicemail and call recording tables

CREATE TABLE IF NOT EXISTS voicemails (
    id              UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    callee_uri      TEXT NOT NULL,          -- Who the voicemail is for
    caller_uri      TEXT NOT NULL,          -- Who left the voicemail
    caller_name     TEXT NOT NULL DEFAULT '',
    duration_secs   INTEGER NOT NULL DEFAULT 0,
    file_id         UUID REFERENCES files(id) ON DELETE SET NULL,
    listened        BOOLEAN NOT NULL DEFAULT false,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_voicemails_callee ON voicemails (callee_uri);
CREATE INDEX IF NOT EXISTS idx_voicemails_created_at ON voicemails (created_at DESC);
CREATE INDEX IF NOT EXISTS idx_voicemails_unlistened ON voicemails (callee_uri, listened) WHERE listened = false;

CREATE TABLE IF NOT EXISTS call_recordings (
    id              UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    call_id         TEXT,                   -- SIP Call-ID
    caller_uri      TEXT NOT NULL,
    callee_uri      TEXT NOT NULL,
    duration_secs   INTEGER NOT NULL DEFAULT 0,
    file_id         UUID REFERENCES files(id) ON DELETE SET NULL,
    recorded_by     TEXT NOT NULL,          -- Who initiated the recording
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_call_recordings_caller ON call_recordings (caller_uri);
CREATE INDEX IF NOT EXISTS idx_call_recordings_callee ON call_recordings (callee_uri);
CREATE INDEX IF NOT EXISTS idx_call_recordings_created_at ON call_recordings (created_at DESC);
