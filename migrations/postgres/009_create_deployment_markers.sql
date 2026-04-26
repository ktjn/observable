CREATE TABLE IF NOT EXISTS deployment_markers (
    deployment_id   UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id       UUID        NOT NULL,
    project_id      UUID        REFERENCES projects(id) ON DELETE SET NULL,
    service_name    TEXT        NOT NULL,
    environment     TEXT        NOT NULL,
    service_version TEXT        NOT NULL,
    status          TEXT        NOT NULL CHECK (status IN ('in_progress', 'success', 'failed', 'rolled_back')),
    started_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    finished_at     TIMESTAMPTZ,
    deployed_by     TEXT,
    commit_sha      TEXT,
    rollback_of     UUID        REFERENCES deployment_markers(deployment_id) ON DELETE SET NULL,
    metadata        JSONB,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS deployment_markers_tenant_idx     ON deployment_markers (tenant_id);
CREATE INDEX IF NOT EXISTS deployment_markers_service_idx    ON deployment_markers (tenant_id, service_name);
CREATE INDEX IF NOT EXISTS deployment_markers_started_at_idx ON deployment_markers (started_at);

-- Dev seed: one successful deployment for the dev tenant / shop-api
INSERT INTO deployment_markers (
    deployment_id,
    tenant_id,
    project_id,
    service_name,
    environment,
    service_version,
    status,
    started_at,
    finished_at,
    deployed_by,
    commit_sha
) VALUES (
    '20000000-0000-0000-0000-000000000001',
    '00000000-0000-0000-0000-000000000001',
    (SELECT id FROM projects
     WHERE tenant_id = '00000000-0000-0000-0000-000000000001'
       AND name = 'default'
     LIMIT 1),
    'shop-api',
    'staging',
    'v1.2.0',
    'success',
    NOW() - INTERVAL '2 hours',
    NOW() - INTERVAL '1 hour 45 minutes',
    'ci-bot',
    'abc123def456'
) ON CONFLICT DO NOTHING;
