-- ============================================================================
-- Migration 092: approved change freezes and exceptions (observable-control, W5a,
-- docs/assurance-relocations.md, OBS-11-09)
-- ============================================================================

CREATE TABLE IF NOT EXISTS control.change_freezes (
    freeze_id   UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id   UUID        NOT NULL,
    name        TEXT        NOT NULL CHECK (length(btrim(name)) BETWEEN 1 AND 200),
    environment TEXT        NOT NULL CHECK (length(environment) BETWEEN 1 AND 100),
    -- Empty means every service in the environment.
    services    TEXT[]      NOT NULL DEFAULT '{}',
    starts_at   TIMESTAMPTZ NOT NULL,
    ends_at     TIMESTAMPTZ NOT NULL,
    approved_by UUID        NOT NULL,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    CHECK (ends_at > starts_at)
);

CREATE INDEX IF NOT EXISTS change_freezes_tenant_idx
    ON control.change_freezes (tenant_id, environment, starts_at);

-- An approved exception lets a change proceed during a freeze; one per deployment and freeze.
CREATE TABLE IF NOT EXISTS control.freeze_exceptions (
    tenant_id     UUID        NOT NULL,
    freeze_id     UUID        NOT NULL REFERENCES control.change_freezes(freeze_id) ON DELETE CASCADE,
    deployment_id UUID        NOT NULL,
    approved_by   UUID        NOT NULL,
    reason        TEXT        NOT NULL CHECK (length(btrim(reason)) BETWEEN 1 AND 1000),
    approved_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (freeze_id, deployment_id)
);

GRANT SELECT, INSERT, UPDATE, DELETE ON control.change_freezes, control.freeze_exceptions
    TO observable_control_role;
