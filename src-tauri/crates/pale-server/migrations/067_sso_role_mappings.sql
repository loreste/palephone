-- OIDC group/role claim mapping for SSO providers (Phase 0.4)

ALTER TABLE sso_providers
    ADD COLUMN IF NOT EXISTS groups_claim TEXT NOT NULL DEFAULT 'groups';

ALTER TABLE sso_providers
    ADD COLUMN IF NOT EXISTS default_role TEXT NOT NULL DEFAULT 'user';

-- JSON object: {"idp-group-name": "admin", "staff": "user"}
ALTER TABLE sso_providers
    ADD COLUMN IF NOT EXISTS role_mappings_json TEXT NOT NULL DEFAULT '{}';
