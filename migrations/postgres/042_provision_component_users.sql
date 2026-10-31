-- ============================================================================
-- Migration 042: Provision Isolated Component Login Users and Role Grants
--
-- In migration 041, schema-scoped NOLOGIN roles (`observable_auth_role`,
-- `observable_control_role`, `observable_alerting_role`) were established with
-- strictly isolated permissions:
--   - observable_auth_role     -> auth schema only
--   - observable_control_role  -> control schema only
--   - observable_alerting_role -> alerting schema only
--
-- This migration provisions explicit LOGIN users for each component:
--   - `observable_auth`     (member of `observable_auth_role`)
--   - `observable_control`  (member of `observable_control_role`)
--   - `observable_alerting` (member of `observable_alerting_role`)
--
-- Each login user defaults its search_path to its owned schema, followed by
-- public, ensuring that each component connects using least-privilege
-- credentials isolated to its own data boundary.
-- ============================================================================

DO $$
BEGIN
    -- 1. Create component login users idempotently.
    BEGIN
        EXECUTE 'CREATE USER observable_auth WITH PASSWORD ''observable_auth'' IN ROLE observable_auth_role';
    EXCEPTION
        WHEN duplicate_object OR unique_violation THEN
            EXECUTE 'GRANT observable_auth_role TO observable_auth';
    END;

    BEGIN
        EXECUTE 'CREATE USER observable_control WITH PASSWORD ''observable_control'' IN ROLE observable_control_role';
    EXCEPTION
        WHEN duplicate_object OR unique_violation THEN
            EXECUTE 'GRANT observable_control_role TO observable_control';
    END;

    BEGIN
        EXECUTE 'CREATE USER observable_alerting WITH PASSWORD ''observable_alerting'' IN ROLE observable_alerting_role';
    EXCEPTION
        WHEN duplicate_object OR unique_violation THEN
            EXECUTE 'GRANT observable_alerting_role TO observable_alerting';
    END;
END $$;

-- 2. Configure default search_path per login user.
ALTER ROLE observable_auth SET search_path = auth, public;
ALTER ROLE observable_control SET search_path = control, public;
ALTER ROLE observable_alerting SET search_path = alerting, public;
