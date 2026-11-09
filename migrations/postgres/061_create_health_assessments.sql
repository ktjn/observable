-- ============================================================================
-- Migration 061: health rules and scope-bound assessment snapshots (phase 1 C2.3)
--
-- Owned by observable-health (schema `health`, migration 058 grants the role).
--   rules        published rule revisions; tenant_id NULL is the platform default.
--                Tenant rule writes arrive with the rule slice and its audit fields.
--   assessments  current snapshot per tenant, catalog service and environment
--                class. scope_key binds it to the identities evaluated, so a
--                changed binding or class invalidates it until reevaluation.
-- ============================================================================

CREATE TABLE IF NOT EXISTS health.rules (
    rule_id           TEXT        NOT NULL CHECK (rule_id <> ''),
    revision          INTEGER     NOT NULL CHECK (revision > 0),
    tenant_id         UUID,
    freshness_seconds INTEGER     NOT NULL CHECK (freshness_seconds > 0),
    lookback_seconds  INTEGER     NOT NULL CHECK (lookback_seconds > 0),
    -- [{"signal": "traces|logs|metrics|any", "classification": "public|internal|confidential|restricted"}]
    conditions        JSONB       NOT NULL CHECK (jsonb_typeof(conditions) = 'array' AND jsonb_array_length(conditions) > 0),
    published_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (rule_id, revision),
    CHECK (lookback_seconds >= freshness_seconds)
);

CREATE INDEX IF NOT EXISTS health_rules_tenant_idx ON health.rules (tenant_id, revision DESC);

CREATE TABLE IF NOT EXISTS health.assessments (
    tenant_id              UUID        NOT NULL,
    service_id             UUID        NOT NULL,
    environment_class      TEXT        NOT NULL CHECK (environment_class IN ('production', 'non_production')),
    rule_id                TEXT        NOT NULL,
    rule_revision          INTEGER     NOT NULL,
    scope_key              TEXT        NOT NULL,
    -- [{"service_name": "...", "environment": "..."}]
    identities             JSONB       NOT NULL,
    state                  TEXT        NOT NULL CHECK (state IN ('normal', 'degraded', 'unavailable', 'planned_maintenance', 'unknown')),
    -- [{"signal": "...", "classification": "...", "last_evidence_at": "..." | null}]
    conditions             JSONB       NOT NULL,
    freshness_seconds      INTEGER     NOT NULL CHECK (freshness_seconds > 0),
    last_evidence_at       TIMESTAMPTZ,
    evaluated_at           TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (tenant_id, service_id, environment_class)
);

-- Platform default: any signal of a bound identity within five minutes is current evidence.
-- Timestamps alone cannot establish degraded, unavailable or maintenance, so this rule yields
-- only normal or unknown.
INSERT INTO health.rules (rule_id, revision, tenant_id, freshness_seconds, lookback_seconds, conditions)
VALUES ('platform-default', 1, NULL, 300, 86400, '[{"signal": "any", "classification": "internal"}]')
ON CONFLICT DO NOTHING;
