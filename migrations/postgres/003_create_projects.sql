CREATE TABLE projects (
    id         UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id  UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    name       TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (tenant_id, name)
);

CREATE INDEX projects_tenant_id_idx ON projects(tenant_id);

INSERT INTO projects (tenant_id, name) VALUES
    ('00000000-0000-0000-0000-000000000001', 'default')
ON CONFLICT DO NOTHING;
