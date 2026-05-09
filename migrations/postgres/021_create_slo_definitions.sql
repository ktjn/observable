CREATE TABLE IF NOT EXISTS slo_definitions (
    slo_id                   UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id                UUID        NOT NULL,
    service_name             TEXT        NOT NULL,
    environment              TEXT        NOT NULL,
    sli_type                 TEXT        NOT NULL CHECK (sli_type IN ('availability')),
    target                   DOUBLE PRECISION NOT NULL CHECK (target > 0 AND target < 1),
    window_days              INTEGER     NOT NULL CHECK (window_days > 0),
    burn_rate_fast_threshold DOUBLE PRECISION NOT NULL CHECK (burn_rate_fast_threshold > 0),
    burn_rate_slow_threshold DOUBLE PRECISION NOT NULL CHECK (burn_rate_slow_threshold > 0),
    description              TEXT        NOT NULL DEFAULT '',
    created_at               TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at               TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (tenant_id, service_name, environment, sli_type)
);

CREATE INDEX IF NOT EXISTS slo_definitions_tenant_service_idx
    ON slo_definitions (tenant_id, service_name, environment);

INSERT INTO slo_definitions (
    slo_id, tenant_id, service_name, environment, sli_type, target, window_days,
    burn_rate_fast_threshold, burn_rate_slow_threshold, description
) VALUES (
    '20000000-0000-0000-0000-000000000001',
    '00000000-0000-0000-0000-000000000001',
    'checkout',
    'prod',
    'availability',
    0.999,
    30,
    14.4,
    1.0,
    'Checkout availability SLO'
) ON CONFLICT DO NOTHING;

INSERT INTO alert_rules (rule_id, tenant_id, name, alert_type, severity, condition) VALUES (
    '20000000-0000-0000-0000-000000000002',
    '00000000-0000-0000-0000-000000000001',
    'Checkout availability burn rate',
    'slo_burn_rate',
    'critical',
    '{"slo_id":"20000000-0000-0000-0000-000000000001","fast_window_minutes":60,"slow_window_minutes":360}'
) ON CONFLICT DO NOTHING;
