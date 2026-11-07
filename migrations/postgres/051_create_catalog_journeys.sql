-- ============================================================================
-- Migration 051: Critical customer journeys (observable-catalog, phase 1 S4)
--
-- A journey names the business services a customer path depends on, its
-- criticality, an accountable owner and whether it is in observability scope
-- (OBS-01-01). Out-of-scope journeys keep a rationale. Grants come from the
-- default privileges set in migration 046.
-- ============================================================================

CREATE TABLE IF NOT EXISTS catalog.journeys (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id       UUID NOT NULL,
    name            TEXT NOT NULL CHECK (length(trim(name)) > 0),
    description     TEXT NOT NULL DEFAULT '',
    criticality     TEXT NOT NULL CHECK (criticality IN ('critical', 'high', 'standard', 'low')),
    in_scope        BOOLEAN NOT NULL DEFAULT true,
    scope_rationale TEXT NOT NULL DEFAULT '',
    classification  TEXT NOT NULL DEFAULT 'internal' CHECK (classification IN ('public', 'internal', 'confidential', 'restricted')),
    owner_kind      TEXT CHECK (owner_kind IN ('user', 'team')),
    owner_id        UUID,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    CHECK ((owner_kind IS NULL) = (owner_id IS NULL))
);

CREATE UNIQUE INDEX IF NOT EXISTS catalog_journeys_name_uq ON catalog.journeys (tenant_id, lower(name));

CREATE TABLE IF NOT EXISTS catalog.journey_services (
    journey_id UUID NOT NULL REFERENCES catalog.journeys(id) ON DELETE CASCADE,
    service_id UUID NOT NULL REFERENCES catalog.services(id) ON DELETE CASCADE,
    tenant_id  UUID NOT NULL,
    step       INTEGER NOT NULL CHECK (step >= 0),
    PRIMARY KEY (journey_id, service_id)
);

CREATE INDEX IF NOT EXISTS catalog_journey_services_service_idx ON catalog.journey_services (service_id);
