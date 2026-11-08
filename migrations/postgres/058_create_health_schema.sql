-- Phase 1 C2.1: isolate the assessment runtime from other component schemas.
CREATE SCHEMA IF NOT EXISTS health;

DO $$
BEGIN
    BEGIN
        CREATE ROLE observable_health_role NOLOGIN;
    EXCEPTION WHEN duplicate_object OR unique_violation THEN
        NULL;
    END;

    BEGIN
        -- Local bootstrap credential. Production provisions a rotated credential.
        CREATE USER observable_health WITH PASSWORD 'observable_health' IN ROLE observable_health_role;
    EXCEPTION WHEN duplicate_object OR unique_violation THEN
        GRANT observable_health_role TO observable_health;
    END;
END $$;

REVOKE ALL ON SCHEMA health FROM PUBLIC;
GRANT USAGE ON SCHEMA health TO observable_health_role;
GRANT SELECT, INSERT, UPDATE, DELETE ON ALL TABLES IN SCHEMA health TO observable_health_role;
GRANT USAGE, SELECT ON ALL SEQUENCES IN SCHEMA health TO observable_health_role;

ALTER DEFAULT PRIVILEGES IN SCHEMA health
    GRANT SELECT, INSERT, UPDATE, DELETE ON TABLES TO observable_health_role;
ALTER DEFAULT PRIVILEGES IN SCHEMA health
    GRANT USAGE, SELECT ON SEQUENCES TO observable_health_role;

ALTER ROLE observable_health SET search_path = health;
