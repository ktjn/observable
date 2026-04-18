CREATE TABLE IF NOT EXISTS api_keys (
    id         UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id  UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    key_hash   TEXT NOT NULL UNIQUE,
    name       TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    revoked_at TIMESTAMPTZ
);

CREATE INDEX IF NOT EXISTS api_keys_tenant_id_idx ON api_keys(tenant_id);

-- Seed dev API key: value = "dev-api-key-0000" (SHA-256 hash)
-- SHA-256("dev-api-key-0000") stored; services compare hash, not plaintext
INSERT INTO api_keys (tenant_id, key_hash, name) VALUES (
    '00000000-0000-0000-0000-000000000001',
    'e18f3d8fb3eb31a042e4a55877e0276960294d0980b8076efaac30dabdbbf67b',
    'dev-key'
) ON CONFLICT DO NOTHING;
