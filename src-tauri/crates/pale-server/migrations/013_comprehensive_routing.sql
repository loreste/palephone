-- ============================================================================
-- COMPREHENSIVE SIP ROUTING
-- ============================================================================

ALTER TABLE routing_rules ADD COLUMN IF NOT EXISTS method_pattern TEXT NOT NULL DEFAULT '*';
ALTER TABLE routing_rules ADD COLUMN IF NOT EXISTS header_conditions JSONB NOT NULL DEFAULT '[]'::jsonb;
ALTER TABLE routing_rules ADD COLUMN IF NOT EXISTS header_actions JSONB NOT NULL DEFAULT '[]'::jsonb;
ALTER TABLE routing_rules ADD COLUMN IF NOT EXISTS stop_processing BOOLEAN NOT NULL DEFAULT true;
ALTER TABLE extensions ADD COLUMN IF NOT EXISTS is_did BOOLEAN NOT NULL DEFAULT false;

CREATE INDEX IF NOT EXISTS idx_routing_rules_method_priority
  ON routing_rules (method_pattern, priority ASC)
  WHERE enabled = true;

CREATE INDEX IF NOT EXISTS idx_extensions_dids ON extensions (extension) WHERE is_did = true;
