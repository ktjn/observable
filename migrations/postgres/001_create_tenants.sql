CREATE TABLE IF NOT EXISTS tenants (
    id         UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name       TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Seed a dev tenant for local testing
INSERT INTO tenants (id, name) VALUES
    ('00000000-0000-0000-0000-000000000001', 'dev-tenant')
ON CONFLICT DO NOTHING;
