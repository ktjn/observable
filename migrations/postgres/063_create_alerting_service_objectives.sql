-- ============================================================================
-- Migration 063: approved measurable objectives per catalog service and
-- environment class (observable-alerting, phase 1 C3, OBS-01-04)
--
-- An objective binds a measurable target to a catalog service id and one
-- approved environment class. It is separate from the catalog's approved
-- outcome statement (catalog.service_outcomes): the outcome says what matters
-- to the business, the objective says how it is measured. Only an approved
-- objective yields a compliance result; changing the definition clears the
-- approval.
-- ============================================================================

CREATE TABLE IF NOT EXISTS alerting.service_objectives (
    objective_id            UUID             PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id               UUID             NOT NULL,
    service_id              UUID             NOT NULL,
    environment_class       TEXT             NOT NULL CHECK (environment_class IN ('production', 'non_production')),
    sli_type                TEXT             NOT NULL CHECK (sli_type IN ('availability')),
    target                  DOUBLE PRECISION NOT NULL CHECK (target > 0 AND target < 1),
    window_days             INTEGER          NOT NULL CHECK (window_days BETWEEN 1 AND 90),
    -- Share of the error budget consumed at which a compliant objective is at risk.
    at_risk_budget_fraction DOUBLE PRECISION NOT NULL DEFAULT 0.75 CHECK (at_risk_budget_fraction > 0 AND at_risk_budget_fraction < 1),
    -- Evidence older than this is not current; the result is unknown.
    freshness_seconds       INTEGER          NOT NULL DEFAULT 900 CHECK (freshness_seconds > 0),
    description             TEXT             NOT NULL DEFAULT '',
    created_by              UUID,
    created_at              TIMESTAMPTZ      NOT NULL DEFAULT now(),
    updated_at              TIMESTAMPTZ      NOT NULL DEFAULT now(),
    approved_by             UUID,
    approved_at             TIMESTAMPTZ,
    CHECK ((approved_by IS NULL) = (approved_at IS NULL)),
    UNIQUE (tenant_id, service_id, environment_class, sli_type)
);

CREATE INDEX IF NOT EXISTS service_objectives_service_idx
    ON alerting.service_objectives (tenant_id, service_id);

GRANT SELECT, INSERT, UPDATE, DELETE ON alerting.service_objectives TO observable_alerting_role;
