-- Migration 018: Scheduled meeting lifecycle and recurrence metadata
-- Stored ScheduledMeeting records are JSON business objects; this migration
-- reserves the version slot for deployments and documents the contract.

CREATE INDEX IF NOT EXISTS idx_business_objects_meetings_updated
    ON business_objects (updated_at DESC)
    WHERE collection = 'scheduled_meetings';
