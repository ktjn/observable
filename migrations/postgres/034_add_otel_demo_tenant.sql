-- Tenant and API key for the OpenTelemetry Demo app.
-- The otel-demo otel-collector sends signals to ingest-gateway using this key.

INSERT INTO tenants (id, name) VALUES
    ('00000000-0000-0000-0000-000000000004', 'otel-demo')
ON CONFLICT DO NOTHING;

-- API key for otel-demo OTel ingest
-- Plaintext: "otel-demo-api-key-0000"
-- SHA-256:   030125c5cc858af2101f76252b43d2584542ed00525857398835e21e91d126c7
INSERT INTO api_keys (tenant_id, key_hash, name, environment, role) VALUES (
    '00000000-0000-0000-0000-000000000004',
    '030125c5cc858af2101f76252b43d2584542ed00525857398835e21e91d126c7',
    'otel-demo-ingest',
    'otel-demo',
    'member'
) ON CONFLICT DO NOTHING;
