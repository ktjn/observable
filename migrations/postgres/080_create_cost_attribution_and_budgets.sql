-- Service cost attribution (OBS-17-01) and budget overrun alerting with
-- backup owner escalation (OBS-17-02).

CREATE TABLE IF NOT EXISTS control.service_cost_attribution (
    id                 UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id          UUID NOT NULL,
    service_name       TEXT NOT NULL,
    owner_team         TEXT,
    cost_center        TEXT,
    expenditure_cents  BIGINT NOT NULL CHECK (expenditure_cents >= 0),
    period             TEXT NOT NULL,
    recorded_at        TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS service_cost_attribution_tenant_period_idx
    ON control.service_cost_attribution (tenant_id, period);

CREATE TABLE IF NOT EXISTS control.cost_budgets (
    id                 UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id          UUID NOT NULL,
    cost_scope         TEXT NOT NULL,
    budget_cents       BIGINT NOT NULL CHECK (budget_cents >= 0),
    primary_owner      TEXT NOT NULL,
    backup_owner       TEXT NOT NULL,
    updated_at         TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (tenant_id, cost_scope)
);

CREATE TABLE IF NOT EXISTS control.cost_budget_alerts (
    id                    UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id             UUID NOT NULL,
    cost_scope            TEXT NOT NULL,
    actual_cents          BIGINT NOT NULL,
    budget_cents          BIGINT NOT NULL,
    recipient             TEXT NOT NULL,
    delivery_status       TEXT NOT NULL CHECK (delivery_status IN ('delivered', 'escalated_delivered', 'failed')),
    escalated_to_backup   BOOLEAN NOT NULL DEFAULT false,
    reason                TEXT NOT NULL,
    created_at            TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS cost_budget_alerts_tenant_idx
    ON control.cost_budget_alerts (tenant_id, created_at DESC);
