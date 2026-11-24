-- ============================================================================
-- Migration 088: W3 of docs/plans/wire-unrouted-capabilities.md (observable-alerting)
--
--   * OBS-06-10 contractual terms of an objective: contracted target, period fee and
--     credit tiers, read back as an SLA review per period.
--   * OBS-07-12 alert type capacity_forecast; a firing keeps the forecast it was raised on
--     (label, predicted value, horizon, basis) so the warning is never shown as measured.
--   * OBS-33-05 recorded execution of an incident's approved procedure: runs, approval of
--     automated runs, and each step with actor, inputs, outputs and result.
-- ============================================================================

DO $$
DECLARE
    target record;
BEGIN
    FOR target IN
        SELECT table_schema, table_name FROM information_schema.tables
        WHERE table_schema IN ('alerting', 'public') AND table_name = 'alert_rules'
    LOOP
        EXECUTE format(
            'ALTER TABLE %I.%I DROP CONSTRAINT IF EXISTS alert_rules_alert_type_check',
            target.table_schema, target.table_name
        );
        EXECUTE format(
            'ALTER TABLE %I.%I ADD CONSTRAINT alert_rules_alert_type_check '
            'CHECK (alert_type IN (''threshold'', ''anomaly'', ''change_detection'', ''deadman'', '
            '''composite'', ''topology_impact'', ''slo_burn_rate'', ''deployment_regression'', '
            '''capacity_forecast''))',
            target.table_schema, target.table_name
        );
    END LOOP;
    FOR target IN
        SELECT table_schema, table_name FROM information_schema.tables
        WHERE table_schema IN ('alerting', 'public') AND table_name = 'alert_firings'
    LOOP
        EXECUTE format(
            'ALTER TABLE %I.%I ADD COLUMN IF NOT EXISTS forecast JSONB',
            target.table_schema, target.table_name
        );
    END LOOP;
END $$;

CREATE TABLE IF NOT EXISTS alerting.objective_contracts (
    objective_id      UUID             PRIMARY KEY,
    tenant_id         UUID             NOT NULL,
    contract_ref      TEXT             NOT NULL CHECK (length(trim(contract_ref)) BETWEEN 1 AND 200),
    contracted_target DOUBLE PRECISION NOT NULL CHECK (contracted_target > 0 AND contracted_target < 1),
    -- Fee for one review period in minor currency units (cents).
    period_fee_minor  BIGINT           NOT NULL CHECK (period_fee_minor >= 0),
    currency          TEXT             NOT NULL CHECK (currency ~ '^[A-Z]{3}$'),
    -- [{"below": attainment, "credit_bp": basis points of the fee}], ascending by "below".
    credit_tiers      JSONB            NOT NULL,
    recorded_by       UUID             NOT NULL,
    updated_at        TIMESTAMPTZ      NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS objective_contracts_tenant_idx
    ON alerting.objective_contracts (tenant_id);

CREATE TABLE IF NOT EXISTS alerting.procedure_runs (
    run_id         UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id      UUID        NOT NULL,
    incident_id    UUID        NOT NULL,
    procedure_ref  TEXT        NOT NULL,
    automated      BOOLEAN     NOT NULL,
    status         TEXT        NOT NULL CHECK (status IN ('pending_approval', 'in_progress', 'completed', 'failed')),
    requested_by   UUID        NOT NULL,
    approved_by    UUID,
    approved_at    TIMESTAMPTZ,
    created_at     TIMESTAMPTZ NOT NULL DEFAULT now(),
    finished_at    TIMESTAMPTZ,
    CHECK ((approved_by IS NULL) = (approved_at IS NULL)),
    CHECK (approved_by IS NULL OR approved_by <> requested_by)
);

CREATE INDEX IF NOT EXISTS procedure_runs_incident_idx
    ON alerting.procedure_runs (tenant_id, incident_id, created_at);

CREATE TABLE IF NOT EXISTS alerting.procedure_steps (
    step_id     UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    run_id      UUID        NOT NULL REFERENCES alerting.procedure_runs(run_id) ON DELETE CASCADE,
    tenant_id   UUID        NOT NULL,
    seq         INTEGER     NOT NULL CHECK (seq > 0),
    name        TEXT        NOT NULL CHECK (length(trim(name)) BETWEEN 1 AND 200),
    actor_kind  TEXT        NOT NULL CHECK (actor_kind IN ('user', 'automation')),
    actor_id    UUID,
    inputs      JSONB       NOT NULL,
    outputs     JSONB       NOT NULL,
    result      TEXT        NOT NULL CHECK (result IN ('succeeded', 'failed', 'skipped')),
    recorded_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CHECK ((actor_kind = 'user') = (actor_id IS NOT NULL)),
    UNIQUE (run_id, seq)
);

GRANT SELECT, INSERT, UPDATE, DELETE
    ON alerting.objective_contracts, alerting.procedure_runs, alerting.procedure_steps
    TO observable_alerting_role;
