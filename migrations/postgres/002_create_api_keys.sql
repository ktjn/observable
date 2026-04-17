CREATE TABLE api_keys (
    id         UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id  UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    key_hash   TEXT NOT NULL UNIQUE,
    name       TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    revoked_at TIMESTAMPTZ
);

CREATE INDEX api_keys_tenant_id_idx ON api_keys(tenant_id);

-- Seed dev API key: value = "dev-api-key-0000" (SHA-256 hash)
-- SHA-256("dev-api-key-0000") stored; services compare hash, not plaintext
INSERT INTO api_keys (tenant_id, key_hash, name) VALUES (
    '00000000-0000-0000-0000-000000000001',
    '5a3f8e2b4d1c9f07a6b2e8d3c4f1a9e2b7d5c3f8e1a4b6d9c2e5f7a3b8d1c4e6',
    'dev-key'
) ON CONFLICT DO NOTHING;
