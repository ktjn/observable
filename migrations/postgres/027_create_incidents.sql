CREATE TABLE IF NOT EXISTS incidents (
    incident_id          UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id            UUID        NOT NULL,
    title                TEXT        NOT NULL,
    severity             TEXT        NOT NULL CHECK (severity IN ('critical', 'warning', 'info')),
    status               TEXT        NOT NULL CHECK (status IN ('triggered', 'acknowledged', 'investigating', 'resolved', 'post_mortem')),
    dedup_key            TEXT        NOT NULL,
    triggered_by_rule_id UUID        REFERENCES alert_rules(rule_id),
    triggered_at         TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    resolved_at          TIMESTAMPTZ,
    runbook_url          TEXT,
    created_at           TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at           TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS incidents_tenant_status_idx ON incidents (tenant_id, status);
CREATE INDEX IF NOT EXISTS incidents_dedup_key_idx ON incidents (tenant_id, dedup_key);
