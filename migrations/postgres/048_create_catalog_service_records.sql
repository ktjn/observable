-- ============================================================================
-- Migration 048: Catalog service records (observable-catalog, phase 1 S3)
--
-- Business services with criticality, lifecycle and data classification,
-- identifier aliases (active or retired), teams, accountable owners and
-- business outcomes. Every row carries tenant_id. Grants come from the
-- default privileges set in migration 046.
-- ============================================================================

CREATE TABLE IF NOT EXISTS catalog.services (
    id               UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id        UUID NOT NULL,
    business_name    TEXT NOT NULL CHECK (length(trim(business_name)) > 0),
    operational_name TEXT NOT NULL CHECK (length(trim(operational_name)) > 0),
    domain           TEXT NOT NULL DEFAULT '',
    description      TEXT NOT NULL DEFAULT '',
    criticality      TEXT NOT NULL CHECK (criticality IN ('critical', 'high', 'standard', 'low')),
    lifecycle        TEXT NOT NULL DEFAULT 'live' CHECK (lifecycle IN ('planned', 'live', 'deprecated', 'retired')),
    classification   TEXT NOT NULL DEFAULT 'internal' CHECK (classification IN ('public', 'internal', 'confidential', 'restricted')),
    support_route    TEXT NOT NULL DEFAULT '',
    created_at       TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at       TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE UNIQUE INDEX IF NOT EXISTS catalog_services_operational_name_uq
    ON catalog.services (tenant_id, lower(operational_name));
CREATE UNIQUE INDEX IF NOT EXISTS catalog_services_business_name_uq
    ON catalog.services (tenant_id, lower(business_name));

CREATE TABLE IF NOT EXISTS catalog.service_aliases (
    service_id UUID NOT NULL REFERENCES catalog.services(id) ON DELETE CASCADE,
    tenant_id  UUID NOT NULL,
    alias      TEXT NOT NULL CHECK (length(trim(alias)) > 0),
    state      TEXT NOT NULL DEFAULT 'active' CHECK (state IN ('active', 'retired')),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    retired_at TIMESTAMPTZ,
    PRIMARY KEY (service_id, alias)
);

CREATE UNIQUE INDEX IF NOT EXISTS catalog_service_aliases_active_uq
    ON catalog.service_aliases (tenant_id, lower(alias)) WHERE state = 'active';

CREATE TABLE IF NOT EXISTS catalog.teams (
    id         UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id  UUID NOT NULL,
    name       TEXT NOT NULL CHECK (length(trim(name)) > 0),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE UNIQUE INDEX IF NOT EXISTS catalog_teams_name_uq ON catalog.teams (tenant_id, lower(name));

CREATE TABLE IF NOT EXISTS catalog.team_members (
    team_id UUID NOT NULL REFERENCES catalog.teams(id) ON DELETE CASCADE,
    user_id UUID NOT NULL,
    PRIMARY KEY (team_id, user_id)
);

CREATE TABLE IF NOT EXISTS catalog.service_owners (
    service_id     UUID NOT NULL REFERENCES catalog.services(id) ON DELETE CASCADE,
    tenant_id      UUID NOT NULL,
    role           TEXT NOT NULL CHECK (role IN ('business_owner', 'operational_owner')),
    principal_kind TEXT NOT NULL CHECK (principal_kind IN ('user', 'team')),
    principal_id   UUID NOT NULL,
    assigned_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (service_id, role)
);

CREATE TABLE IF NOT EXISTS catalog.service_outcomes (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    service_id  UUID NOT NULL REFERENCES catalog.services(id) ON DELETE CASCADE,
    tenant_id   UUID NOT NULL,
    statement   TEXT NOT NULL CHECK (length(trim(statement)) > 0),
    expectation TEXT NOT NULL DEFAULT '',
    owner_kind  TEXT NOT NULL CHECK (owner_kind IN ('user', 'team')),
    owner_id    UUID NOT NULL,
    approved_by UUID,
    approved_at TIMESTAMPTZ,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    CHECK ((approved_by IS NULL) = (approved_at IS NULL))
);

CREATE INDEX IF NOT EXISTS catalog_service_outcomes_service_idx ON catalog.service_outcomes (service_id);
CREATE INDEX IF NOT EXISTS catalog_services_tenant_idx ON catalog.services (tenant_id);
