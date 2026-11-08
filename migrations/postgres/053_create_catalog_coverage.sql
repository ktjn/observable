-- ============================================================================
-- Migration 053: Coverage and asset registry (observable-catalog, phase 1 S5)
--
--   service_bindings     telemetry identities (service.name, environment) of a
--                        catalog service; one identity maps to one service
--   coverage_policies    required coverage items per criticality, with approval
--   observability_assets registered alert rules, objectives, dashboards,
--                        proactive checks, health rules and runbooks, each with
--                        an owning component and an accountable owner
--   coverage_claims      owner statements that an item is met; never evidence
--   coverage_runs        latest assessment status per service
--   coverage_assessments latest required/demonstrated result per item; a
--                        demonstrated item always carries an evidence reference
--   coverage_gaps        unmet critical items and journeys with remediation,
--                        due/review dates and risk acceptance
-- Grants come from the default privileges set in migration 046.
-- ============================================================================

CREATE TABLE IF NOT EXISTS catalog.service_bindings (
    service_id   UUID NOT NULL REFERENCES catalog.services(id) ON DELETE CASCADE,
    tenant_id    UUID NOT NULL,
    service_name TEXT NOT NULL CHECK (length(service_name) BETWEEN 1 AND 255),
    environment  TEXT NOT NULL DEFAULT '' CHECK (length(environment) <= 64),
    created_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (service_id, service_name, environment)
);

CREATE UNIQUE INDEX IF NOT EXISTS catalog_service_bindings_identity_uq
    ON catalog.service_bindings (tenant_id, service_name, environment);

CREATE TABLE IF NOT EXISTS catalog.coverage_policies (
    tenant_id      UUID NOT NULL,
    criticality    TEXT NOT NULL CHECK (criticality IN ('critical', 'high', 'standard', 'low')),
    items          TEXT[] NOT NULL CHECK (
        cardinality(items) > 0
        AND items <@ ARRAY['traces', 'logs', 'metrics', 'alert_rule', 'objective', 'dashboard',
                           'proactive_check', 'health_rule', 'runbook']::TEXT[]
    ),
    window_minutes INTEGER NOT NULL DEFAULT 60 CHECK (window_minutes BETWEEN 5 AND 10080),
    approved_by    UUID,
    approved_at    TIMESTAMPTZ,
    updated_at     TIMESTAMPTZ NOT NULL DEFAULT now(),
    CHECK ((approved_by IS NULL) = (approved_at IS NULL)),
    PRIMARY KEY (tenant_id, criticality)
);

CREATE TABLE IF NOT EXISTS catalog.observability_assets (
    tenant_id        UUID NOT NULL,
    kind             TEXT NOT NULL CHECK (kind IN ('alert_rule', 'objective', 'dashboard',
                                                   'proactive_check', 'health_rule', 'runbook')),
    external_id      TEXT NOT NULL CHECK (length(external_id) BETWEEN 1 AND 200),
    owning_component TEXT NOT NULL CHECK (length(owning_component) BETWEEN 1 AND 100),
    service_id       UUID NOT NULL REFERENCES catalog.services(id) ON DELETE CASCADE,
    name             TEXT NOT NULL DEFAULT '',
    owner_kind       TEXT CHECK (owner_kind IN ('user', 'team')),
    owner_id         UUID,
    registered_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at       TIMESTAMPTZ NOT NULL DEFAULT now(),
    retired_at       TIMESTAMPTZ,
    CHECK ((owner_kind IS NULL) = (owner_id IS NULL)),
    PRIMARY KEY (tenant_id, kind, external_id)
);

CREATE INDEX IF NOT EXISTS catalog_observability_assets_service_idx
    ON catalog.observability_assets (service_id);

CREATE TABLE IF NOT EXISTS catalog.coverage_claims (
    service_id UUID NOT NULL REFERENCES catalog.services(id) ON DELETE CASCADE,
    tenant_id  UUID NOT NULL,
    item       TEXT NOT NULL,
    claimed_by TEXT NOT NULL,
    note       TEXT NOT NULL DEFAULT '',
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (service_id, item)
);

CREATE TABLE IF NOT EXISTS catalog.coverage_runs (
    service_id    UUID PRIMARY KEY REFERENCES catalog.services(id) ON DELETE CASCADE,
    tenant_id     UUID NOT NULL,
    status        TEXT NOT NULL CHECK (status IN ('covered', 'partial', 'uncovered', 'unknown')),
    policy_source TEXT NOT NULL CHECK (policy_source IN ('approved', 'unapproved', 'default')),
    window_minutes INTEGER NOT NULL,
    error         TEXT,
    assessed_at   TIMESTAMPTZ NOT NULL
);

CREATE TABLE IF NOT EXISTS catalog.coverage_assessments (
    service_id   UUID NOT NULL REFERENCES catalog.services(id) ON DELETE CASCADE,
    tenant_id    UUID NOT NULL,
    item         TEXT NOT NULL,
    demonstrated BOOLEAN NOT NULL,
    evidence_ref TEXT,
    detail       TEXT NOT NULL DEFAULT '',
    assessed_at  TIMESTAMPTZ NOT NULL,
    CHECK (NOT demonstrated OR evidence_ref IS NOT NULL),
    PRIMARY KEY (service_id, item)
);

CREATE TABLE IF NOT EXISTS catalog.coverage_gaps (
    id                     UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id              UUID NOT NULL,
    service_id             UUID REFERENCES catalog.services(id) ON DELETE CASCADE,
    journey_id             UUID REFERENCES catalog.journeys(id) ON DELETE CASCADE,
    item                   TEXT NOT NULL,
    state                  TEXT NOT NULL DEFAULT 'open'
                           CHECK (state IN ('open', 'remediating', 'accepted', 'closed')),
    remediation_owner_kind TEXT CHECK (remediation_owner_kind IN ('user', 'team')),
    remediation_owner_id   UUID,
    due_date               DATE,
    review_date            DATE,
    acceptance_approver    UUID,
    acceptance_rationale   TEXT,
    closed_reason          TEXT CHECK (closed_reason IN ('remediated', 'risk_accepted')),
    created_at             TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at             TIMESTAMPTZ NOT NULL DEFAULT now(),
    CHECK ((service_id IS NULL) <> (journey_id IS NULL)),
    CHECK ((remediation_owner_kind IS NULL) = (remediation_owner_id IS NULL)),
    CHECK (state <> 'remediating' OR (remediation_owner_id IS NOT NULL AND due_date IS NOT NULL)),
    CHECK (state <> 'accepted' OR (acceptance_approver IS NOT NULL AND review_date IS NOT NULL
                                   AND length(trim(coalesce(acceptance_rationale, ''))) > 0)),
    CHECK ((state = 'closed') = (closed_reason IS NOT NULL))
);

CREATE UNIQUE INDEX IF NOT EXISTS catalog_coverage_gaps_open_uq
    ON catalog.coverage_gaps (tenant_id, coalesce(service_id, journey_id), item)
    WHERE state <> 'closed';
