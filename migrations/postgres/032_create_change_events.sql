CREATE TABLE IF NOT EXISTS change_events (
    change_event_id UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id       UUID        NOT NULL,
    project_id      UUID        REFERENCES projects(id) ON DELETE SET NULL,
    event_type      TEXT        NOT NULL CHECK (event_type IN ('config_change', 'feature_flag', 'migration', 'incident', 'other')),
    service_name    TEXT,
    environment     TEXT        NOT NULL,
    title           TEXT        NOT NULL,
    description     TEXT,
    occurred_at     TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    source          TEXT,
    created_by      TEXT,
    metadata        JSONB,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_change_events_tenant ON change_events (tenant_id);
CREATE INDEX IF NOT EXISTS idx_change_events_tenant_service ON change_events (tenant_id, service_name);
CREATE INDEX IF NOT EXISTS idx_change_events_occurred_at ON change_events (occurred_at);
