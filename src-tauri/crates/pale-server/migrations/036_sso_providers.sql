-- SSO / OIDC / SAML providers
CREATE TABLE IF NOT EXISTS sso_providers (
    id               UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name             TEXT NOT NULL,
    provider_type    TEXT NOT NULL DEFAULT 'oidc',
    client_id        TEXT NOT NULL DEFAULT '',
    client_secret_enc TEXT NOT NULL DEFAULT '',
    issuer_url       TEXT NOT NULL DEFAULT '',
    redirect_uri     TEXT NOT NULL DEFAULT '',
    enabled          BOOLEAN NOT NULL DEFAULT true,
    created_at       TIMESTAMPTZ NOT NULL DEFAULT now()
);
