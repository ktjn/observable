-- Phase 3 prep (docs/component-decomposition.md): document PostgreSQL table
-- ownership by physical schema, without yet separating credentials or
-- rewriting application SQL. Every existing table moves into the PostgreSQL
-- schema matching its target component owner (ADR-035's Data Ownership
-- table); application code keeps using unqualified table names because every
-- connection is opened with `options=-c search_path=...` in the connection
-- URL itself (`observable_config::with_search_path`/`require_database_url`,
-- and the equivalent in libs/test-support and every Postgres-testcontainer
-- test harness). No behavior change: this is a namespace reorganization, not
-- an access-control boundary yet -- the shared application role can still
-- read/write across all three schemas, and several existing foreign keys
-- already cross an ownership boundary (see the note at the end of this
-- file). Real credential separation and removing cross-owner SQL is blocked
-- on Phase 4 (shrink query-api) and Phase 6 (consolidate alerting) first
-- moving the code that issues that SQL out of query-api.
--
-- REPLAY SAFETY (read this before touching this file):
-- docker-compose's postgres-setup and charts/observable's migration-job.yaml
-- both unconditionally replay *every* file in migrations/postgres/ on every
-- startup/deploy -- there is no migration-tracking table. Every migration
-- before this one is purely additive (CREATE TABLE/INDEX IF NOT EXISTS,
-- ALTER TABLE ... ADD COLUMN IF NOT EXISTS, INSERT ... ON CONFLICT DO
-- NOTHING), so blind replay was always a safe no-op. This migration is the
-- first to *move* a table, which breaks that invariant in a subtle way:
-- Postgres's `CREATE TABLE IF NOT EXISTS <unqualified name>` resolves its
-- existence check *only* against the single schema it would create into
-- (the first schema in search_path with CREATE privilege) -- it does NOT
-- search the rest of search_path for a same-named relation living
-- elsewhere. Confirmed empirically: with search_path = `auth, public` and
-- `auth.foo` already existing, `CREATE TABLE IF NOT EXISTS foo` targets
-- `public` and happily creates a second, unrelated `public.foo` --
-- no "already exists" skip, no error. So on a bare replay, migration 018's
-- `CREATE TABLE IF NOT EXISTS users` would recreate an empty `public.users`
-- once `users` has moved to `auth`, and *this* migration's own
-- `ALTER TABLE public.users SET SCHEMA auth` would then fail on its own
-- next replay ("relation users already exists in schema auth") -- both
-- reproduced against a real Postgres container while developing this fix.
--
-- The fix below has two parts:
--   1. Each move only runs if the target schema doesn't already have the
--      table (i.e. only on the first, pristine run).
--   2. A structurally identical placeholder table (`LIKE ... INCLUDING
--      ALL`, so it has the real columns/constraints) is always left behind
--      in `public` under the original name. This makes every historical
--      migration's unqualified reference to that table replay-safe: `CREATE
--      TABLE IF NOT EXISTS` sees the placeholder and skips; `ALTER TABLE ...
--      ADD COLUMN IF NOT EXISTS` succeeds harmlessly against it (a view
--      wouldn't support ADD COLUMN, which is why this uses a real table, not
--      a view); the handful of `INSERT ... ON CONFLICT DO NOTHING`/`DELETE`
--      seed migrations operate on it harmlessly, isolated from the real
--      table -- which already has the correct data from the original
--      pristine install, since this migration is the *last* file, so it
--      only ever affects replays, never a table's first creation.
--
-- This only works because `public` stays first in the *database-level*
-- default search_path set below -- that's what makes it the CREATE target
-- for every historical migration's unqualified `CREATE TABLE IF NOT
-- EXISTS`. It is deliberately the opposite order from
-- `observable_config::with_search_path`, which puts the owner schemas
-- first so application code resolves directly to the real tables. A manual
-- `psql $DATABASE_URL` session (or anything else connecting without the
-- application's connection-string override) will see these placeholders,
-- not the real data, for unqualified queries against any moved table --
-- schema-qualify explicitly, or connect with the same `options=-c
-- search_path=...` override the application services use.

CREATE SCHEMA IF NOT EXISTS auth;
CREATE SCHEMA IF NOT EXISTS control;
CREATE SCHEMA IF NOT EXISTS alerting;

DO $$
BEGIN
    EXECUTE format(
        'ALTER DATABASE %I SET search_path = public, auth, control, alerting',
        current_database()
    );
END
$$;

DO $$
DECLARE
    moves record;
BEGIN
    FOR moves IN
        SELECT * FROM (VALUES
            -- observable-auth
            ('users', 'auth'),
            ('user_tenant_roles', 'auth'),
            ('user_sessions', 'auth'),
            ('api_keys', 'auth'),
            ('credential_audit_log', 'auth'),
            -- observable-control
            ('tenants', 'control'),
            ('projects', 'control'),
            ('change_events', 'control'),
            ('deployment_markers', 'control'),
            ('platform_config', 'control'),
            ('schema_entries', 'control'),
            ('semantic_annotations', 'control'),
            ('dashboards', 'control'),
            ('dashboard_panels', 'control'),
            ('dashboard_grants', 'control'),
            ('saved_views', 'control'),
            ('saved_view_grants', 'control'),
            -- observable-alerting
            ('alert_rules', 'alerting'),
            ('alert_firings', 'alerting'),
            ('slo_definitions', 'alerting'),
            ('notification_channels', 'alerting'),
            ('notification_audit_log', 'alerting'),
            ('incidents', 'alerting'),
            ('incident_events', 'alerting')
        ) AS t(table_name, target_schema)
    LOOP
        -- Move the real table on a pristine run only. On replay, `public`
        -- holds just the placeholder created below (not the real table),
        -- and the target schema already has the real table -- skip.
        IF EXISTS (
             SELECT 1 FROM pg_tables
             WHERE schemaname = 'public' AND tablename = moves.table_name
           )
           AND NOT EXISTS (
             SELECT 1 FROM pg_tables
             WHERE schemaname = moves.target_schema AND tablename = moves.table_name
           )
        THEN
            EXECUTE format('ALTER TABLE public.%I SET SCHEMA %I', moves.table_name, moves.target_schema);
        END IF;

        EXECUTE format(
            'CREATE TABLE IF NOT EXISTS public.%I (LIKE %I.%I INCLUDING ALL)',
            moves.table_name, moves.target_schema, moves.table_name
        );
    END LOOP;
END $$;

-- query_audit_log deliberately stays in public: it's query-api's own read-audit
-- trail, and query has no target PostgreSQL ownership in the component
-- decomposition (the target architecture's exit evidence is "core query runs
-- without PostgreSQL" -- see docs/component-decomposition.md Phase 4). Moving
-- it into one of the three owner schemas above would be guessing an ownership
-- decision that hasn't actually been made; left as an explicit open question.

-- Known cross-owner foreign keys that predate this reorganization and remain
-- functionally valid across schemas (PostgreSQL permits cross-schema FKs),
-- but represent the "no cross-owner SQL" boundary ADR-035 targets, not yet
-- resolved:
--   auth.api_keys.tenant_id          -> control.tenants(id)
--   auth.user_tenant_roles.tenant_id -> control.tenants(id)
--   auth.user_sessions.tenant_id     -> control.tenants(id)
--   control.dashboard_grants.user_id -> auth.users(id)
--   control.saved_views.owner_user_id -> auth.users(id)
--   control.saved_view_grants.user_id -> auth.users(id)
-- Removing these (replacing the DB-enforced FK with an application-level
-- check) is part of the eventual "remove cross-owner SQL" work, not this
-- migration.
