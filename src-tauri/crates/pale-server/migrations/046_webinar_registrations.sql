-- Webinar registration, attendee management, waitlist
CREATE TABLE IF NOT EXISTS webinar_registrations (
    id UUID PRIMARY KEY,
    conference_id UUID NOT NULL,
    name TEXT NOT NULL,
    email TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'registered',
    registered_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    custom_fields JSONB DEFAULT '{}'::jsonb
);

CREATE INDEX IF NOT EXISTS idx_webinar_registrations_conference ON webinar_registrations(conference_id);
CREATE INDEX IF NOT EXISTS idx_webinar_registrations_email ON webinar_registrations(email);
