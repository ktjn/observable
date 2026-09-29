-- Phase 3 prep (docs/component-decomposition.md): document PostgreSQL table
-- ownership by physical schema, without yet separating credentials or
-- rewriting application SQL. Every existing table moves into the PostgreSQL
-- schema matching its target component owner (ADR-035's Data Ownership
-- table); application code keeps using unqualified table names because every
-- connection is opened with `options=-c search_path=...` in the connection
-- URL itself (`observable_config::with_search_path` / `require_database_url`,
-- and the equivalent in libs/test-support and every Postgres-testcontainer
-- test harness) -- see the note near the bottom of this file for why a
-- server-side default alone doesn't work. No behavior change: this is a
-- namespace reorganization, not an access-control boundary yet -- the shared
-- application role can still read/write across all three schemas, and
-- several existing foreign keys already cross an ownership boundary (see the
-- note at the end of this file). Real credential separation and removing
-- cross-owner SQL is blocked on Phase 4 (shrink query-api) and Phase 6
-- (consolidate alerting) first moving the code that issues that SQL out of
-- query-api.

CREATE SCHEMA IF NOT EXISTS auth;
CREATE SCHEMA IF NOT EXISTS control;
CREATE SCHEMA IF NOT EXISTS alerting;

-- observable-auth: users, memberships/roles, sessions, API keys, credential audit
ALTER TABLE IF EXISTS public.users               SET SCHEMA auth;
ALTER TABLE IF EXISTS public.user_tenant_roles    SET SCHEMA auth;
ALTER TABLE IF EXISTS public.user_sessions        SET SCHEMA auth;
ALTER TABLE IF EXISTS public.api_keys             SET SCHEMA auth;
ALTER TABLE IF EXISTS public.credential_audit_log SET SCHEMA auth;

-- observable-control: tenants/projects/environments, configuration,
-- dashboards, saved views, schema/catalog annotations, deployment/change metadata
ALTER TABLE IF EXISTS public.tenants              SET SCHEMA control;
ALTER TABLE IF EXISTS public.projects             SET SCHEMA control;
ALTER TABLE IF EXISTS public.change_events        SET SCHEMA control;
ALTER TABLE IF EXISTS public.deployment_markers   SET SCHEMA control;
ALTER TABLE IF EXISTS public.platform_config      SET SCHEMA control;
ALTER TABLE IF EXISTS public.schema_entries       SET SCHEMA control;
ALTER TABLE IF EXISTS public.semantic_annotations SET SCHEMA control;
ALTER TABLE IF EXISTS public.dashboards           SET SCHEMA control;
ALTER TABLE IF EXISTS public.dashboard_panels     SET SCHEMA control;
ALTER TABLE IF EXISTS public.dashboard_grants     SET SCHEMA control;
ALTER TABLE IF EXISTS public.saved_views          SET SCHEMA control;
ALTER TABLE IF EXISTS public.saved_view_grants    SET SCHEMA control;

-- observable-alerting: alert rules, SLOs, firings, notifications, incidents
ALTER TABLE IF EXISTS public.alert_rules           SET SCHEMA alerting;
ALTER TABLE IF EXISTS public.alert_firings         SET SCHEMA alerting;
ALTER TABLE IF EXISTS public.slo_definitions       SET SCHEMA alerting;
ALTER TABLE IF EXISTS public.notification_channels SET SCHEMA alerting;
ALTER TABLE IF EXISTS public.notification_audit_log SET SCHEMA alerting;
ALTER TABLE IF EXISTS public.incidents             SET SCHEMA alerting;
ALTER TABLE IF EXISTS public.incident_events       SET SCHEMA alerting;

-- query_audit_log deliberately stays in public: it's query-api's own read-audit
-- trail, and query has no target PostgreSQL ownership in the component
-- decomposition (the target architecture's exit evidence is "core query runs
-- without PostgreSQL" -- see docs/component-decomposition.md Phase 4). Moving
-- it into one of the three owner schemas above would be guessing an ownership
-- decision that hasn't actually been made; left as an explicit open question.

-- Sets the database-level default search_path too, purely as a convenience
-- for anyone connecting directly (psql, an ad hoc debugging session) without
-- going through the application's connection-string helper below. This is
-- NOT what makes application code work: `ALTER DATABASE ... SET search_path`
-- only affects sessions opened *after* this statement runs, so a connection
-- pool that opened its connections earlier in the same session (exactly what
-- every migration-then-query test harness in this codebase does) never picks
-- it up -- tried first, and it broke admin-service's integration tests for
-- precisely that reason. The real fix is that every PostgreSQL connection
-- URL in this codebase now carries `?options=-c%20search_path%3D...`
-- (`observable_config::with_search_path`/`require_database_url` in
-- production; the same pattern in libs/test-support and every
-- Postgres-testcontainer test harness), which libpq applies at connection
-- startup regardless of pooling or timing.
DO $$
BEGIN
    EXECUTE format(
        'ALTER DATABASE %I SET search_path = public, auth, control, alerting',
        current_database()
    );
END
$$;

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
