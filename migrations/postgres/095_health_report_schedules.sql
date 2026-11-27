-- ============================================================================
-- Migration 095: scheduled status reports (observable-health, W5b,
-- docs/assurance-relocations.md section 4.5, OBS-13-07)
-- ============================================================================

-- A tenant admin requests a schedule for a business scope; a second tenant admin approves it.
-- Only approved schedules produce reports.
CREATE TABLE IF NOT EXISTS health.report_schedules (
    schedule_id       UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id         UUID        NOT NULL,
    name              TEXT        NOT NULL CHECK (length(btrim(name)) BETWEEN 1 AND 200),
    service_ids       UUID[]      NOT NULL CHECK (cardinality(service_ids) BETWEEN 1 AND 200),
    environment_class TEXT        NOT NULL CHECK (environment_class IN ('production', 'non_production')),
    period            TEXT        NOT NULL CHECK (period IN ('weekly', 'monthly')),
    status            TEXT        NOT NULL DEFAULT 'pending_approval'
                                  CHECK (status IN ('pending_approval', 'approved')),
    requested_by      UUID        NOT NULL,
    approved_by       UUID,
    approved_at       TIMESTAMPTZ,
    created_at        TIMESTAMPTZ NOT NULL DEFAULT now(),
    CHECK (approved_by IS NULL OR approved_by <> requested_by),
    CHECK ((status = 'approved') = (approved_at IS NOT NULL))
);

-- One snapshot per schedule and period, written by the report worker.
CREATE TABLE IF NOT EXISTS health.status_reports (
    report_id    UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    schedule_id  UUID        NOT NULL REFERENCES health.report_schedules(schedule_id) ON DELETE CASCADE,
    tenant_id    UUID        NOT NULL,
    period_start TIMESTAMPTZ NOT NULL,
    period_end   TIMESTAMPTZ NOT NULL,
    entries      JSONB       NOT NULL,
    generated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (schedule_id, period_start)
);

GRANT SELECT, INSERT, UPDATE, DELETE ON health.report_schedules TO observable_health_role;
GRANT SELECT, INSERT, DELETE ON health.status_reports TO observable_health_role;
