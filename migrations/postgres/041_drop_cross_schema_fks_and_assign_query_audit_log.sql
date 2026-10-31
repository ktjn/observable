-- Phase 3 completion (docs/component-decomposition.md, spec/22-system-specification.md
-- Milestone 3):
-- 1. Move `query_audit_log` into the `control` schema (`observable-control`
--    owns control-plane usage/audit reporting in `services/admin-service/src/usage.rs`),
--    leaving a replay-safe `public.query_audit_log` placeholder behind just like
--    `040_component_ownership_schemas.sql`.
-- 2. Drop cross-schema foreign keys between `auth.*` (`api_keys`,
--    `user_tenant_roles`, `user_sessions`) and `control.tenants(id)`, and
--    between `control.*` (`dashboard_grants`, `saved_views`, `saved_view_grants`)
--    and `auth.users(id)` so each component schema is migratable and lock-free
--    across component boundaries.
-- 3. Provision schema-scoped component roles (`observable_auth_role`,
--    `observable_control_role`, `observable_alerting_role`) with strictly isolated
--    USAGE and table DML grants on their respective schemas (`auth`, `control`,
--    `alerting`).

DO $$
DECLARE
    fk record;
BEGIN
    -- 1. Move query_audit_log into control on pristine run; leave replay placeholder in public.
    IF EXISTS (
         SELECT 1 FROM pg_tables
         WHERE schemaname = 'public' AND tablename = 'query_audit_log'
       )
       AND NOT EXISTS (
         SELECT 1 FROM pg_tables
         WHERE schemaname = 'control' AND tablename = 'query_audit_log'
       )
    THEN
        ALTER TABLE public.query_audit_log SET SCHEMA control;
    END IF;

    IF EXISTS (
         SELECT 1 FROM pg_tables
         WHERE schemaname = 'control' AND tablename = 'query_audit_log'
       )
    THEN
        CREATE TABLE IF NOT EXISTS public.query_audit_log (LIKE control.query_audit_log INCLUDING ALL);
    END IF;

    -- 2. Drop any foreign-key constraint in auth/control/alerting whose referenced
    --    table resides in a different schema (or public placeholder).
    FOR fk IN
        SELECT
            ns_src.nspname AS src_schema,
            cl_src.relname AS src_table,
            con.conname    AS constraint_name
        FROM pg_constraint con
        JOIN pg_class cl_src      ON cl_src.oid = con.conrelid
        JOIN pg_namespace ns_src  ON ns_src.oid = cl_src.relnamespace
        JOIN pg_class cl_dst      ON cl_dst.oid = con.confrelid
        JOIN pg_namespace ns_dst  ON ns_dst.oid = cl_dst.relnamespace
        WHERE con.contype = 'f'
          AND ns_src.nspname IN ('auth', 'control', 'alerting')
          AND ns_dst.nspname <> ns_src.nspname
    LOOP
        EXECUTE format(
            'ALTER TABLE %I.%I DROP CONSTRAINT IF EXISTS %I',
            fk.src_schema, fk.src_table, fk.constraint_name
        );
    END LOOP;

    -- 3. Create component roles idempotently and scope permissions strictly to each owner schema.
    -- Catch unique_violation or duplicate_object in case of race across concurrent database migrations in shared cluster.
    BEGIN
        EXECUTE 'CREATE ROLE observable_auth_role NOLOGIN';
    EXCEPTION
        WHEN duplicate_object OR unique_violation THEN
            NULL;
    END;
    BEGIN
        EXECUTE 'CREATE ROLE observable_control_role NOLOGIN';
    EXCEPTION
        WHEN duplicate_object OR unique_violation THEN
            NULL;
    END;
    BEGIN
        EXECUTE 'CREATE ROLE observable_alerting_role NOLOGIN';
    EXCEPTION
        WHEN duplicate_object OR unique_violation THEN
            NULL;
    END;
END $$;

REVOKE ALL ON SCHEMA auth, control, alerting FROM PUBLIC, observable_auth_role, observable_control_role, observable_alerting_role;
REVOKE ALL ON ALL TABLES IN SCHEMA auth, control, alerting FROM PUBLIC, observable_auth_role, observable_control_role, observable_alerting_role;

GRANT USAGE ON SCHEMA auth TO observable_auth_role;
GRANT SELECT, INSERT, UPDATE, DELETE ON ALL TABLES IN SCHEMA auth TO observable_auth_role;
GRANT USAGE, SELECT ON ALL SEQUENCES IN SCHEMA auth TO observable_auth_role;

GRANT USAGE ON SCHEMA control TO observable_control_role;
GRANT SELECT, INSERT, UPDATE, DELETE ON ALL TABLES IN SCHEMA control TO observable_control_role;
GRANT USAGE, SELECT ON ALL SEQUENCES IN SCHEMA control TO observable_control_role;

GRANT USAGE ON SCHEMA alerting TO observable_alerting_role;
GRANT SELECT, INSERT, UPDATE, DELETE ON ALL TABLES IN SCHEMA alerting TO observable_alerting_role;
GRANT USAGE, SELECT ON ALL SEQUENCES IN SCHEMA alerting TO observable_alerting_role;
