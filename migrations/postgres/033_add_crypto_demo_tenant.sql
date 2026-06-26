-- Tenant and API key for the crypto live-data demo app.
-- The crypto-demo backend sends OTel signals to ingest-gateway using this key.

INSERT INTO tenants (id, name) VALUES
    ('00000000-0000-0000-0000-000000000003', 'crypto-demo')
ON CONFLICT DO NOTHING;

-- API key for crypto-demo OTel ingest
-- Plaintext: "crypto-demo-api-key-0000"
-- SHA-256:   37ac18cbe1211e5d3801d2199d03b0af6227eb32756d6660983f248bc5594d6b
INSERT INTO api_keys (tenant_id, key_hash, name, environment, role) VALUES (
    '00000000-0000-0000-0000-000000000003',
    '37ac18cbe1211e5d3801d2199d03b0af6227eb32756d6660983f248bc5594d6b',
    'crypto-demo-ingest',
    'crypto-demo',
    'member'
) ON CONFLICT DO NOTHING;
