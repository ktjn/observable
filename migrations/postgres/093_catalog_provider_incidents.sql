-- ============================================================================
-- Migration 093: regulated functions and provider incidents (observable-catalog,
-- W5b, docs/assurance-relocations.md section 4.6, OBS-15-06)
-- ============================================================================

-- A tenant admin marks a dependent function as regulated, and who did so is recorded.
ALTER TABLE catalog.provider_dependencies
    ADD COLUMN IF NOT EXISTS regulated    BOOLEAN NOT NULL DEFAULT false,
    ADD COLUMN IF NOT EXISTS regulated_by UUID,
    ADD COLUMN IF NOT EXISTS regulated_at TIMESTAMPTZ;

-- A disruption at a registered provider. Only a material incident exposes functions.
CREATE TABLE IF NOT EXISTS catalog.provider_incidents (
    id          UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id   UUID        NOT NULL,
    provider_id UUID        NOT NULL REFERENCES catalog.providers(id) ON DELETE CASCADE,
    title       TEXT        NOT NULL CHECK (length(btrim(title)) BETWEEN 1 AND 200),
    started_at  TIMESTAMPTZ NOT NULL,
    material    BOOLEAN     NOT NULL,
    recorded_by TEXT        NOT NULL,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS catalog_provider_incidents_provider_idx
    ON catalog.provider_incidents (tenant_id, provider_id, started_at DESC);

GRANT SELECT, INSERT, UPDATE, DELETE ON catalog.provider_incidents TO observable_catalog_role;
