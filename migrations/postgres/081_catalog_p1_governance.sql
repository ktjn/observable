-- ============================================================================
-- Migration 081: Catalog P1 governance (OBS-02-05, OBS-02-06, OBS-02-07, OBS-02-08)
--
-- Adds:
--   * catalog.services.operating_hours
--   * catalog.service_maintenance_windows (planned unavailability)
--   * catalog.observed_unregistered (observed services/components not in catalog)
--   * catalog.authoritative_sync_conflicts (owner mismatches vs authoritative records)
-- ============================================================================

ALTER TABLE catalog.services
    ADD COLUMN IF NOT EXISTS operating_hours TEXT NOT NULL DEFAULT '';

CREATE TABLE IF NOT EXISTS catalog.service_maintenance_windows (
    id         UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id  UUID NOT NULL,
    service_id UUID NOT NULL REFERENCES catalog.services(id) ON DELETE CASCADE,
    title      TEXT NOT NULL CHECK (length(trim(title)) > 0),
    starts_at  TIMESTAMPTZ NOT NULL,
    ends_at    TIMESTAMPTZ NOT NULL,
    created_by UUID NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CHECK (ends_at > starts_at)
);

CREATE INDEX IF NOT EXISTS catalog_maintenance_service_idx
    ON catalog.service_maintenance_windows (tenant_id, service_id, starts_at);

CREATE TABLE IF NOT EXISTS catalog.observed_unregistered (
    id                  UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id           UUID NOT NULL,
    operational_name    TEXT NOT NULL CHECK (length(trim(operational_name)) > 0),
    component_kind      TEXT NOT NULL DEFAULT 'service' CHECK (component_kind IN ('service', 'component', 'provider')),
    proposed_owner_kind TEXT CHECK (proposed_owner_kind IN ('user', 'team')),
    proposed_owner_id   UUID,
    last_observed_at    TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE UNIQUE INDEX IF NOT EXISTS catalog_observed_unregistered_name_uq
    ON catalog.observed_unregistered (tenant_id, lower(operational_name));

CREATE TABLE IF NOT EXISTS catalog.authoritative_sync_conflicts (
    id                         UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id                  UUID NOT NULL,
    service_id                 UUID NOT NULL REFERENCES catalog.services(id) ON DELETE CASCADE,
    role                       TEXT NOT NULL CHECK (role IN ('business_owner', 'operational_owner')),
    catalog_principal_id       UUID,
    authoritative_principal_id UUID NOT NULL,
    detected_at                TIMESTAMPTZ NOT NULL DEFAULT now(),
    resolved_at                TIMESTAMPTZ
);

CREATE UNIQUE INDEX IF NOT EXISTS catalog_auth_conflicts_open_uq
    ON catalog.authoritative_sync_conflicts (tenant_id, service_id, role)
    WHERE resolved_at IS NULL;
