-- ============================================================================
-- Migration 087: Who operates a catalog service (observable-catalog, W2c, OBS-16-03)
--
-- A business-critical service may be operated by a third party (outsourced). It is a
-- catalog service like any other, listed alongside internal services, and names the
-- provider that operates it.
-- ============================================================================

ALTER TABLE catalog.services
    ADD COLUMN operated_by TEXT NOT NULL DEFAULT 'internal'
        CHECK (operated_by IN ('internal', 'third_party')),
    ADD COLUMN operator_provider_id UUID REFERENCES catalog.providers(id) ON DELETE SET NULL,
    ADD CONSTRAINT catalog_services_operator_provider_ck
        CHECK (operated_by = 'third_party' OR operator_provider_id IS NULL);
