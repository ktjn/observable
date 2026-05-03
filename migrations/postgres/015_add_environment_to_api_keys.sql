ALTER TABLE api_keys ADD COLUMN IF NOT EXISTS environment TEXT NOT NULL DEFAULT '';

-- Existing dev key is used by the testbench OTel collector
UPDATE api_keys SET environment = 'testbench'
WHERE key_hash = 'e18f3d8fb3eb31a042e4a55877e0276960294d0980b8076efaac30dabdbbf67b';

-- Dedicated key for Observable's own internal services
-- Plaintext: "observable-api-key-0000"
-- SHA-256:   48163672d1d8365582dfca2d833851ebcba12bf0a6faa4fe509402e28532b53c
INSERT INTO api_keys (tenant_id, key_hash, name, environment, role) VALUES (
    '00000000-0000-0000-0000-000000000001',
    '48163672d1d8365582dfca2d833851ebcba12bf0a6faa4fe509402e28532b53c',
    'observable-internal',
    'observable',
    'member'
) ON CONFLICT DO NOTHING;
