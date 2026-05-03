CREATE TABLE IF NOT EXISTS dashboards (
    dashboard_id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id    UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    name         TEXT NOT NULL,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS dashboards_tenant_created_idx
    ON dashboards (tenant_id, created_at DESC);

CREATE TABLE IF NOT EXISTS dashboard_panels (
    panel_id         UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    dashboard_id     UUID NOT NULL REFERENCES dashboards(dashboard_id) ON DELETE CASCADE,
    title            TEXT NOT NULL,
    query_kind       TEXT NOT NULL CHECK (query_kind IN ('logs', 'traces', 'metrics')),
    service          TEXT,
    lookback_minutes INTEGER NOT NULL CHECK (lookback_minutes > 0),
    filters          JSONB NOT NULL DEFAULT '{}'::jsonb,
    position         INTEGER NOT NULL DEFAULT 0
);

CREATE INDEX IF NOT EXISTS dashboard_panels_dashboard_position_idx
    ON dashboard_panels (dashboard_id, position);
