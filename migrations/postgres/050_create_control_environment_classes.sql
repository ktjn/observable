-- ============================================================================
-- Migration 050: Environment classes (observable-control, phase 1 S4)
--
-- Operating contexts (OBS-01-03): each tenant environment is classified as
-- production or non_production by a tenant admin. An environment without a
-- row is unclassified and belongs to neither context. Business-critical
-- comes from catalog service criticality, not from this table.
-- ============================================================================

CREATE TABLE IF NOT EXISTS control.environment_classes (
    tenant_id   UUID NOT NULL,
    environment TEXT NOT NULL CHECK (length(environment) BETWEEN 1 AND 64),
    class       TEXT NOT NULL CHECK (class IN ('production', 'non_production')),
    updated_by  TEXT NOT NULL,
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (tenant_id, environment)
);

GRANT SELECT, INSERT, UPDATE, DELETE ON control.environment_classes TO observable_control_role;
