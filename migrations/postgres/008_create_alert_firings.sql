CREATE TABLE IF NOT EXISTS alert_firings (
    firing_id   UUID             PRIMARY KEY DEFAULT gen_random_uuid(),
    rule_id     UUID             NOT NULL REFERENCES alert_rules(rule_id),
    tenant_id   UUID             NOT NULL,
    state       TEXT             NOT NULL CHECK (state IN ('pending', 'active', 'resolved')),
    value       DOUBLE PRECISION,
    occurred_at TIMESTAMPTZ      NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS alert_firings_rule_idx        ON alert_firings (rule_id);
CREATE INDEX IF NOT EXISTS alert_firings_tenant_idx      ON alert_firings (tenant_id);
CREATE INDEX IF NOT EXISTS alert_firings_occurred_at_idx ON alert_firings (occurred_at);
