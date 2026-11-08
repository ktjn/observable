-- ============================================================================
-- Migration 056: Material providers and their dependents (observable-catalog,
-- phase 1 S7)
--
-- A provider record names a third party, its kind, materiality, contract
-- reference and accountable contract owner (OBS-36-01). A dependency links a
-- business service or a critical journey and the function it performs to a
-- provider. A dependency may name a provider that is not registered
-- (provider_id NULL): it is reported as an `unregistered_provider` gap, and
-- registering a provider of that name links it. Deleting a provider returns
-- its dependencies to unregistered. Grants come from the default privileges
-- set in migration 046.
-- ============================================================================

CREATE TABLE IF NOT EXISTS catalog.providers (
    id                  UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id           UUID NOT NULL,
    name                TEXT NOT NULL CHECK (length(trim(name)) BETWEEN 1 AND 200),
    kind                TEXT NOT NULL CHECK (kind IN ('cloud', 'saas', 'network', 'payment', 'data', 'other')),
    material            BOOLEAN NOT NULL DEFAULT true,
    contract_ref        TEXT NOT NULL DEFAULT '',
    contract_owner_kind TEXT CHECK (contract_owner_kind IN ('user', 'team')),
    contract_owner_id   UUID,
    classification      TEXT NOT NULL DEFAULT 'internal' CHECK (classification IN ('public', 'internal', 'confidential', 'restricted')),
    created_at          TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at          TIMESTAMPTZ NOT NULL DEFAULT now(),
    CHECK ((contract_owner_kind IS NULL) = (contract_owner_id IS NULL))
);

CREATE UNIQUE INDEX IF NOT EXISTS catalog_providers_name_uq ON catalog.providers (tenant_id, lower(name));

CREATE TABLE IF NOT EXISTS catalog.provider_dependencies (
    id            UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id     UUID NOT NULL,
    provider_id   UUID REFERENCES catalog.providers(id) ON DELETE SET NULL,
    provider_name TEXT NOT NULL CHECK (length(trim(provider_name)) BETWEEN 1 AND 200),
    service_id    UUID REFERENCES catalog.services(id) ON DELETE CASCADE,
    journey_id    UUID REFERENCES catalog.journeys(id) ON DELETE CASCADE,
    function      TEXT NOT NULL CHECK (length(trim(function)) BETWEEN 1 AND 200),
    created_by    TEXT NOT NULL,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    CHECK ((service_id IS NULL) <> (journey_id IS NULL))
);

CREATE UNIQUE INDEX IF NOT EXISTS catalog_provider_dependencies_uq ON catalog.provider_dependencies
    (tenant_id, lower(provider_name), COALESCE(service_id, journey_id), lower(function));
CREATE INDEX IF NOT EXISTS catalog_provider_dependencies_provider_idx ON catalog.provider_dependencies (provider_id);
