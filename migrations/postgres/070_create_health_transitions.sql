-- ============================================================================
-- Migration 070: health state transitions (phase 2 E3)
--
-- One row each time a stored assessment changes state (not every evaluation), written in
-- the same transaction as the snapshot. Interruptions are derived from these rows at read
-- time. Rows are append-only for the health role except retention and tenant purge, which
-- delete.
-- ============================================================================

CREATE TABLE IF NOT EXISTS health.transitions (
    tenant_id         UUID        NOT NULL,
    service_id        UUID        NOT NULL,
    environment_class TEXT        NOT NULL CHECK (environment_class IN ('production', 'non_production')),
    at                TIMESTAMPTZ NOT NULL,
    -- NULL for the first stored assessment of a service and class.
    from_state        TEXT        CHECK (from_state IN ('normal', 'degraded', 'unavailable', 'planned_maintenance', 'unknown')),
    to_state          TEXT        NOT NULL CHECK (to_state IN ('normal', 'degraded', 'unavailable', 'planned_maintenance', 'unknown')),
    evidence_status   TEXT        NOT NULL,
    rule_id           TEXT        NOT NULL,
    rule_revision     INTEGER     NOT NULL,
    PRIMARY KEY (tenant_id, service_id, environment_class, at)
);

CREATE INDEX IF NOT EXISTS health_transitions_at_idx ON health.transitions (at);

GRANT SELECT, INSERT, DELETE ON health.transitions TO observable_health_role;
REVOKE UPDATE ON health.transitions FROM observable_health_role;
