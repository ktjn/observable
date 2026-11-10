-- ============================================================================
-- Migration 072: data-quality issues (phase 2 E4)
--
-- Raised and resolved by the evaluation worker:
--   source_stopped       a bound identity had evidence within the lookback and then none
--                        for one freshness bound; resolves when evidence returns.
--   inconsistent_inputs  the catalog inputs for the service are ambiguous.
--   evaluation_failed    evidence for the service could not be read.
-- At most one open issue per service, class, kind and identity (empty for non-identity kinds).
-- ============================================================================

CREATE TABLE IF NOT EXISTS health.data_quality_issues (
    id                UUID        PRIMARY KEY,
    tenant_id         UUID        NOT NULL,
    service_id        UUID        NOT NULL,
    environment_class TEXT        NOT NULL CHECK (environment_class IN ('production', 'non_production')),
    kind              TEXT        NOT NULL CHECK (kind IN ('source_stopped', 'inconsistent_inputs', 'evaluation_failed')),
    service_name      TEXT        NOT NULL DEFAULT '',
    environment       TEXT        NOT NULL DEFAULT '',
    opened_at         TIMESTAMPTZ NOT NULL,
    last_seen_at      TIMESTAMPTZ NOT NULL,
    resolved_at       TIMESTAMPTZ,
    detail            JSONB       NOT NULL DEFAULT '{}'::jsonb,
    CHECK (last_seen_at >= opened_at),
    CHECK (resolved_at IS NULL OR resolved_at >= opened_at)
);

CREATE UNIQUE INDEX IF NOT EXISTS health_issues_open_idx
    ON health.data_quality_issues (tenant_id, service_id, environment_class, kind, service_name, environment)
    WHERE resolved_at IS NULL;
CREATE INDEX IF NOT EXISTS health_issues_service_idx
    ON health.data_quality_issues (tenant_id, service_id, environment_class, opened_at DESC);

GRANT SELECT, INSERT, UPDATE, DELETE ON health.data_quality_issues TO observable_health_role;
