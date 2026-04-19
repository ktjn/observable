CREATE TABLE IF NOT EXISTS alert_rules (
    rule_id           UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id         UUID        NOT NULL,
    name              TEXT        NOT NULL,
    alert_type        TEXT        NOT NULL CHECK (alert_type IN (
                          'threshold', 'anomaly', 'change_detection', 'deadman',
                          'composite', 'topology_impact', 'slo_burn_rate', 'deployment_regression'
                      )),
    severity          TEXT        NOT NULL CHECK (severity IN ('critical', 'warning', 'info')),
    condition         JSONB       NOT NULL,
    for_duration_secs BIGINT,
    created_at        TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS alert_rules_tenant_type_idx ON alert_rules (tenant_id, alert_type);

-- Seed one threshold rule for the dev tenant.
-- Fires when any metric named "error_rate" exceeds 0.05 (5%).
INSERT INTO alert_rules (rule_id, tenant_id, name, alert_type, severity, condition) VALUES (
    '10000000-0000-0000-0000-000000000001',
    '00000000-0000-0000-0000-000000000001',
    'High error rate',
    'threshold',
    'critical',
    '{"metric_name":"error_rate","operator":"gt","threshold":0.05}'
) ON CONFLICT DO NOTHING;
