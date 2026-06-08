-- Migration 007: User voicemail settings and follow-me call forwarding

-- Per-user call handling settings
CREATE TABLE IF NOT EXISTS user_call_settings (
    user_sip_uri        TEXT PRIMARY KEY,

    -- Voicemail
    voicemail_enabled   BOOLEAN NOT NULL DEFAULT true,
    voicemail_greeting_file_id UUID REFERENCES files(id) ON DELETE SET NULL,
    voicemail_greeting_text TEXT NOT NULL DEFAULT 'Please leave a message after the tone.',
    voicemail_timeout   INTEGER NOT NULL DEFAULT 20,  -- seconds before going to voicemail

    -- Follow-me / Find-me
    followme_enabled    BOOLEAN NOT NULL DEFAULT false,
    followme_numbers    JSONB NOT NULL DEFAULT '[]',  -- ordered list of {number, ring_timeout, label}
    followme_final      TEXT NOT NULL DEFAULT 'voicemail',  -- voicemail, hangup, or SIP URI

    -- Call forwarding
    forward_always      TEXT,             -- always forward to this URI (if set, overrides everything)
    forward_busy        TEXT,             -- forward when busy
    forward_no_answer   TEXT,             -- forward when no answer (before voicemail)

    -- DND override for calls
    dnd_enabled         BOOLEAN NOT NULL DEFAULT false,
    dnd_forward_to      TEXT,             -- where to send calls during DND (null = voicemail)

    updated_at          TIMESTAMPTZ NOT NULL DEFAULT now()
);

DROP TRIGGER IF EXISTS trg_user_call_settings_updated_at ON user_call_settings;
CREATE TRIGGER trg_user_call_settings_updated_at BEFORE UPDATE ON user_call_settings
    FOR EACH ROW EXECUTE FUNCTION update_updated_at();

-- Add voicemail notification preferences
ALTER TABLE voicemails ADD COLUMN IF NOT EXISTS transcription TEXT;
ALTER TABLE voicemails ADD COLUMN IF NOT EXISTS notify_email TEXT;
