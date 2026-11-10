-- ============================================================================
-- Migration 068: planned maintenance windows (phase 2 E2)
--
-- A window is requested for one service and environment class, and has effect only once a
-- different person approves it and while it is current and not cancelled. Request, approval
-- and cancellation each record actor and time; the trigger keeps a window's scope, period,
-- reason and recorded actors from being rewritten.
-- ============================================================================

CREATE TABLE IF NOT EXISTS health.maintenance_windows (
    id                UUID        PRIMARY KEY,
    tenant_id         UUID        NOT NULL,
    service_id        UUID        NOT NULL,
    environment_class TEXT        NOT NULL CHECK (environment_class IN ('production', 'non_production')),
    starts_at         TIMESTAMPTZ NOT NULL,
    ends_at           TIMESTAMPTZ NOT NULL,
    reason            TEXT        NOT NULL CHECK (length(btrim(reason)) BETWEEN 1 AND 500),
    requested_by      UUID        NOT NULL,
    requested_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    approved_by       UUID,
    approved_at       TIMESTAMPTZ,
    cancelled_by      UUID,
    cancelled_at      TIMESTAMPTZ,
    CHECK (ends_at > starts_at),
    CHECK (ends_at - starts_at <= INTERVAL '7 days'),
    CHECK ((approved_by IS NULL) = (approved_at IS NULL)),
    CHECK ((cancelled_by IS NULL) = (cancelled_at IS NULL)),
    -- Second-person approval.
    CHECK (approved_by IS NULL OR approved_by <> requested_by)
);

CREATE INDEX IF NOT EXISTS health_maintenance_service_idx
    ON health.maintenance_windows (tenant_id, service_id, environment_class, ends_at DESC);

CREATE OR REPLACE FUNCTION health.maintenance_windows_guard() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
    IF NEW.id <> OLD.id OR NEW.tenant_id <> OLD.tenant_id OR NEW.service_id <> OLD.service_id
       OR NEW.environment_class <> OLD.environment_class OR NEW.starts_at <> OLD.starts_at
       OR NEW.ends_at <> OLD.ends_at OR NEW.reason <> OLD.reason
       OR NEW.requested_by <> OLD.requested_by OR NEW.requested_at <> OLD.requested_at
       OR (OLD.approved_at IS NOT NULL AND (NEW.approved_by IS DISTINCT FROM OLD.approved_by
                                            OR NEW.approved_at IS DISTINCT FROM OLD.approved_at))
       OR (OLD.cancelled_at IS NOT NULL AND (NEW.cancelled_by IS DISTINCT FROM OLD.cancelled_by
                                             OR NEW.cancelled_at IS DISTINCT FROM OLD.cancelled_at)) THEN
        RAISE EXCEPTION 'maintenance window records are append-only' USING ERRCODE = 'check_violation';
    END IF;
    RETURN NEW;
END $$;

DROP TRIGGER IF EXISTS maintenance_windows_guard ON health.maintenance_windows;
CREATE TRIGGER maintenance_windows_guard BEFORE UPDATE ON health.maintenance_windows
    FOR EACH ROW EXECUTE FUNCTION health.maintenance_windows_guard();

GRANT SELECT, INSERT, UPDATE, DELETE ON health.maintenance_windows TO observable_health_role;
