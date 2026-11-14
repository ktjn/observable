-- Compliance incident reconstruction timeline (OBS-15-02) and operational
-- context hand-off outbox (OBS-16-01).

CREATE TABLE IF NOT EXISTS auth.compliance_incident_timeline (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id       UUID NOT NULL,
    incident_id     UUID NOT NULL,
    title           TEXT NOT NULL,
    severity        TEXT NOT NULL,
    event_type      TEXT NOT NULL,
    decision_maker  TEXT NOT NULL,
    rationale       TEXT NOT NULL,
    retention_days  INTEGER NOT NULL CHECK (retention_days > 0),
    details         JSONB NOT NULL DEFAULT '{}'::jsonb,
    occurred_at     TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS compliance_incident_timeline_idx
    ON auth.compliance_incident_timeline (tenant_id, incident_id, occurred_at ASC);

CREATE TABLE IF NOT EXISTS control.context_handoff_outbox (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id       UUID NOT NULL,
    incident_id     TEXT NOT NULL,
    target_url      TEXT NOT NULL,
    status          TEXT NOT NULL CHECK (status IN ('delivered', 'failed_pending_retry')),
    attempts        INTEGER NOT NULL,
    last_error      TEXT,
    payload         JSONB NOT NULL,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS context_handoff_outbox_tenant_idx
    ON control.context_handoff_outbox (tenant_id, created_at DESC);
