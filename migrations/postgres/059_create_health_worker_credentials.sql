-- Health worker identity is separate from ingestion credentials and human sessions.
CREATE TABLE IF NOT EXISTS auth.health_worker_credentials (
    token_hash_sha256 TEXT PRIMARY KEY CHECK (token_hash_sha256 ~ '^[0-9a-f]{64}$'),
    tenant_id UUID NOT NULL,
    expires_at TIMESTAMPTZ NOT NULL,
    revoked_at TIMESTAMPTZ
);

REVOKE ALL ON TABLE auth.health_worker_credentials FROM PUBLIC;
GRANT SELECT ON TABLE auth.health_worker_credentials TO observable_auth_role;
