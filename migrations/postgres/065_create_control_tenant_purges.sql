-- ============================================================================
-- Migration 065: Tenant purge requests (observable-control)
--
-- A tenant purge deletes control and auth data synchronously. Catalog, health
-- and alerting depend on control, so control cannot call them (COMP-06); each
-- pulls pending requests from control, purges its own schema and acknowledges.
-- The request and acknowledgements are retained as the purge record.
-- ============================================================================

CREATE TABLE IF NOT EXISTS control.tenant_purges (
    tenant_id    UUID PRIMARY KEY,
    requested_by TEXT NOT NULL CHECK (length(trim(requested_by)) > 0),
    requested_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    completed_at TIMESTAMPTZ
);

CREATE TABLE IF NOT EXISTS control.tenant_purge_acks (
    tenant_id UUID NOT NULL REFERENCES control.tenant_purges (tenant_id),
    component TEXT NOT NULL CHECK (component IN ('alerting', 'catalog', 'health')),
    purged_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (tenant_id, component)
);

GRANT SELECT, INSERT, UPDATE ON control.tenant_purges TO observable_control_role;
GRANT SELECT, INSERT ON control.tenant_purge_acks TO observable_control_role;
