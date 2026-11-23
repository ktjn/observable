-- ============================================================================
-- Migration 085: Response Schema, Role and Login User (observable-response)
--
-- W2 of docs/plans/wire-unrouted-capabilities.md. observable-response provides
-- exchange-operational-context and communicate-incident-status. It runs inside
-- the admin-service binary but owns its own schema and login:
--   - `observable_response_role` (NOLOGIN) -> response schema only
--   - `observable_response`      (LOGIN, member of the role)
-- No other component role is granted access to `response`.
--
-- The hand-off outbox moves out of `control` (it was owned by no control
-- capability). Breaking change, no data carried over (2.0 decision: no
-- migrations or compatibility for pre-release data).
-- ============================================================================

CREATE SCHEMA IF NOT EXISTS response;

DO $$
BEGIN
    BEGIN
        EXECUTE 'CREATE ROLE observable_response_role NOLOGIN';
    EXCEPTION
        WHEN duplicate_object OR unique_violation THEN
            NULL;
    END;

    BEGIN
        EXECUTE 'CREATE USER observable_response WITH PASSWORD ''observable_response'' IN ROLE observable_response_role';
    EXCEPTION
        WHEN duplicate_object OR unique_violation THEN
            EXECUTE 'GRANT observable_response_role TO observable_response';
    END;
END $$;

REVOKE ALL ON SCHEMA response FROM PUBLIC;
GRANT USAGE ON SCHEMA response TO observable_response_role;

ALTER DEFAULT PRIVILEGES IN SCHEMA response
    GRANT SELECT, INSERT, UPDATE, DELETE ON TABLES TO observable_response_role;
ALTER DEFAULT PRIVILEGES IN SCHEMA response
    GRANT USAGE, SELECT ON SEQUENCES TO observable_response_role;

ALTER ROLE observable_response SET search_path = response, public;

DROP TABLE IF EXISTS control.context_handoff_outbox;

-- Approved exchange targets. Registering a target is the tenant admin's approval;
-- nothing is ever sent to a URL that is not a registered, active target.
CREATE TABLE response.exchange_targets (
    target_id             UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id             UUID NOT NULL,
    kind                  TEXT NOT NULL CHECK (kind IN ('incident_handoff', 'notification', 'status_page', 'external_consumer')),
    name                  TEXT NOT NULL CHECK (length(name) BETWEEN 1 AND 200),
    url                   TEXT NOT NULL,
    -- The principal decide-access evaluates for an external consumer's evidence.
    consumer_principal_id UUID,
    registered_by         UUID NOT NULL,
    registered_at         TIMESTAMPTZ NOT NULL DEFAULT now(),
    active                BOOLEAN NOT NULL DEFAULT true,
    UNIQUE (tenant_id, name),
    CHECK (kind <> 'external_consumer' OR consumer_principal_id IS NOT NULL)
);

-- Every hand-off attempt outcome, so a failed hand-off is never silently lost (OBS-16-01).
CREATE TABLE response.handoff_outbox (
    handoff_id   UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id    UUID NOT NULL,
    kind         TEXT NOT NULL CHECK (kind IN ('incident', 'investigation')),
    incident_id  UUID NOT NULL,
    target_id    UUID NOT NULL REFERENCES response.exchange_targets (target_id) ON DELETE CASCADE,
    status       TEXT NOT NULL CHECK (status IN ('delivered', 'failed_pending_retry')),
    attempts     INTEGER NOT NULL CHECK (attempts > 0),
    last_error   TEXT,
    payload      JSONB NOT NULL,
    requested_by UUID,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX handoff_outbox_tenant_idx ON response.handoff_outbox (tenant_id, created_at DESC);

-- Notification deliveries to approved enterprise channels (OBS-16-04).
CREATE TABLE response.notification_deliveries (
    delivery_id  UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id    UUID NOT NULL,
    target_id    UUID NOT NULL REFERENCES response.exchange_targets (target_id) ON DELETE CASCADE,
    incident_id  UUID,
    status       TEXT NOT NULL CHECK (status IN ('delivered', 'failed')),
    http_status  INTEGER,
    last_error   TEXT,
    requested_by UUID,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX notification_deliveries_tenant_idx ON response.notification_deliveries (tenant_id, created_at DESC);

-- External incident communications (OBS-30-01..04): drafted, approved by a different
-- person, then released to one approved audience.
CREATE TABLE response.incident_communications (
    communication_id   UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id          UUID NOT NULL,
    incident_id        UUID NOT NULL,
    audience_target_id UUID NOT NULL REFERENCES response.exchange_targets (target_id) ON DELETE CASCADE,
    suspected_scope    TEXT[] NOT NULL DEFAULT '{}',
    confirmed_services TEXT[] NOT NULL DEFAULT '{}',
    summary            TEXT NOT NULL CHECK (length(summary) BETWEEN 1 AND 4000),
    evidence_refs      JSONB NOT NULL DEFAULT '[]'::jsonb,
    state              TEXT NOT NULL DEFAULT 'draft' CHECK (state IN ('draft', 'approved', 'released')),
    drafted_by         UUID NOT NULL,
    drafted_at         TIMESTAMPTZ NOT NULL DEFAULT now(),
    approved_by        UUID,
    approved_at        TIMESTAMPTZ,
    released_by        UUID,
    released_at        TIMESTAMPTZ,
    delivery_status    TEXT CHECK (delivery_status IN ('delivered', 'failed')),
    CHECK (approved_by IS NULL OR approved_by <> drafted_by),
    CHECK (state = 'draft' OR approved_by IS NOT NULL),
    CHECK (state <> 'released' OR released_by IS NOT NULL)
);
CREATE INDEX incident_communications_tenant_idx
    ON response.incident_communications (tenant_id, incident_id, drafted_at);

GRANT SELECT, INSERT, UPDATE, DELETE ON ALL TABLES IN SCHEMA response TO observable_response_role;
