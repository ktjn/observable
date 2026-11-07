-- ============================================================================
-- Migration 046: Catalog Schema, Role and Login User (observable-catalog)
--
-- Establishes the `catalog` schema owned by `observable-catalog`
-- (spec/system.yaml, domain `service-model`), following the isolation pattern
-- of migrations 040-042:
--   - `observable_catalog_role` (NOLOGIN) -> catalog schema only
--   - `observable_catalog`      (LOGIN, member of the role)
--
-- Default privileges cover tables and sequences created later in the schema by
-- the migration owner, so catalog tables added in later slices are reachable by
-- the role without per-migration grants. No other component role is granted
-- access to `catalog`, and the catalog role is granted nothing outside it.
-- ============================================================================

CREATE SCHEMA IF NOT EXISTS catalog;

DO $$
BEGIN
    BEGIN
        EXECUTE 'CREATE ROLE observable_catalog_role NOLOGIN';
    EXCEPTION
        WHEN duplicate_object OR unique_violation THEN
            NULL;
    END;

    BEGIN
        EXECUTE 'CREATE USER observable_catalog WITH PASSWORD ''observable_catalog'' IN ROLE observable_catalog_role';
    EXCEPTION
        WHEN duplicate_object OR unique_violation THEN
            EXECUTE 'GRANT observable_catalog_role TO observable_catalog';
    END;
END $$;

REVOKE ALL ON SCHEMA catalog FROM PUBLIC;

GRANT USAGE ON SCHEMA catalog TO observable_catalog_role;
GRANT SELECT, INSERT, UPDATE, DELETE ON ALL TABLES IN SCHEMA catalog TO observable_catalog_role;
GRANT USAGE, SELECT ON ALL SEQUENCES IN SCHEMA catalog TO observable_catalog_role;

ALTER DEFAULT PRIVILEGES IN SCHEMA catalog
    GRANT SELECT, INSERT, UPDATE, DELETE ON TABLES TO observable_catalog_role;
ALTER DEFAULT PRIVILEGES IN SCHEMA catalog
    GRANT USAGE, SELECT ON SEQUENCES TO observable_catalog_role;

ALTER ROLE observable_catalog SET search_path = catalog, public;
