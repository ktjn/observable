-- ============================================================================
-- Migration 083: Health assessment P1 insights (OBS-03-05..08, OBS-26-03, OBS-26-05, OBS-26-06)
-- ============================================================================

CREATE TABLE IF NOT EXISTS health.segment_assessments (
    tenant_id        UUID        NOT NULL,
    service_id       UUID        NOT NULL,
    dimension        TEXT        NOT NULL CHECK (dimension IN ('region', 'channel', 'segment')),
    segment_value    TEXT        NOT NULL CHECK (length(trim(segment_value)) > 0),
    health_state     TEXT        NOT NULL CHECK (health_state IN ('normal', 'degraded', 'unavailable', 'unknown', 'maintenance')),
    incident_summary TEXT        NOT NULL DEFAULT '',
    updated_at       TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (tenant_id, service_id, dimension, segment_value)
);

CREATE TABLE IF NOT EXISTS health.dependency_contribution_rules (
    tenant_id         UUID        NOT NULL,
    service_id        UUID        NOT NULL,
    dependency_name   TEXT        NOT NULL CHECK (length(trim(dependency_name)) > 0),
    contribution_mode TEXT        NOT NULL CHECK (contribution_mode IN ('critical_path', 'failover_redundant', 'non_blocking')),
    rationale         TEXT        NOT NULL DEFAULT '',
    updated_by        UUID        NOT NULL,
    updated_at        TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (tenant_id, service_id, dependency_name)
);

CREATE TABLE IF NOT EXISTS health.shared_infrastructure_health (
    tenant_id             UUID        NOT NULL,
    infra_id              TEXT        NOT NULL CHECK (length(trim(infra_id)) > 0),
    infra_kind            TEXT        NOT NULL CHECK (infra_kind IN ('network', 'platform', 'compute', 'database', 'queue')),
    health_state          TEXT        NOT NULL CHECK (health_state IN ('normal', 'degraded', 'unavailable', 'unknown', 'maintenance')),
    detail                TEXT        NOT NULL DEFAULT '',
    supported_service_ids UUID[]      NOT NULL DEFAULT '{}',
    updated_at            TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (tenant_id, infra_id)
);

CREATE TABLE IF NOT EXISTS health.evidence_qualifications (
    id               UUID             PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id        UUID             NOT NULL,
    service_id       UUID             NOT NULL,
    finding_id       TEXT             NOT NULL CHECK (length(trim(finding_id)) > 0),
    evidence_source  TEXT             NOT NULL CHECK (length(trim(evidence_source)) > 0),
    coverage_pct     DOUBLE PRECISION NOT NULL CHECK (coverage_pct >= 0.0 AND coverage_pct <= 100.0),
    confidence_level TEXT             NOT NULL CHECK (confidence_level IN ('high', 'medium', 'low')),
    limitations      TEXT             NOT NULL DEFAULT '',
    recorded_at      TIMESTAMPTZ      NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS health_evidence_qualifications_svc_idx
    ON health.evidence_qualifications (tenant_id, service_id, recorded_at DESC);

GRANT SELECT, INSERT, UPDATE, DELETE ON health.segment_assessments TO observable_health_role;
GRANT SELECT, INSERT, UPDATE, DELETE ON health.dependency_contribution_rules TO observable_health_role;
GRANT SELECT, INSERT, UPDATE, DELETE ON health.shared_infrastructure_health TO observable_health_role;
GRANT SELECT, INSERT, UPDATE, DELETE ON health.evidence_qualifications TO observable_health_role;
