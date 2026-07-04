-- Migration 048: ML-based communication compliance
CREATE TABLE IF NOT EXISTS compliance_reviews (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    message_id UUID NOT NULL,
    policy_id UUID,
    category TEXT NOT NULL,
    severity TEXT NOT NULL,
    flagged_content TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'pending',
    reviewer TEXT,
    reviewed_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_compliance_reviews_status
    ON compliance_reviews (status, created_at DESC);

CREATE INDEX IF NOT EXISTS idx_compliance_reviews_message
    ON compliance_reviews (message_id);
