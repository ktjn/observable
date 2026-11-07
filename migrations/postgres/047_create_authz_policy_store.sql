-- ============================================================================
-- Migration 047: decide-access policy store (observable-auth, ADR-041)
--
-- Tables in the `auth` schema, owned by observable-auth:
--   - authz_role_grants:    role may perform action on resource_type up to a
--                           data classification (tenant_id NULL = all tenants)
--   - authz_purpose_rules:  action on resource_type at or above a
--                           classification requires a stated purpose
--   - authz_elevations:     time-limited, justified elevation grants per user
--   - access_decision_log:  one row per recorded decision (actor, purpose,
--                           elevation, decision, reason)
--
-- Classification order: public < internal < confidential < restricted.
-- Default grants cover `catalog.service` (phase 1 catalog reads and writes).
-- ============================================================================

CREATE TABLE IF NOT EXISTS auth.authz_role_grants (
    id                 BIGSERIAL PRIMARY KEY,
    tenant_id          UUID,
    role               TEXT NOT NULL CHECK (role IN ('tenant_admin', 'member', 'viewer')),
    resource_type      TEXT NOT NULL CHECK (resource_type <> ''),
    action             TEXT NOT NULL CHECK (action <> ''),
    max_classification TEXT NOT NULL CHECK (max_classification IN ('public', 'internal', 'confidential', 'restricted')),
    created_at         TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE NULLS NOT DISTINCT (tenant_id, role, resource_type, action)
);

CREATE TABLE IF NOT EXISTS auth.authz_purpose_rules (
    id                 BIGSERIAL PRIMARY KEY,
    tenant_id          UUID,
    resource_type      TEXT NOT NULL CHECK (resource_type <> ''),
    action             TEXT NOT NULL CHECK (action <> ''),
    min_classification TEXT NOT NULL CHECK (min_classification IN ('public', 'internal', 'confidential', 'restricted')),
    created_at         TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE NULLS NOT DISTINCT (tenant_id, resource_type, action)
);

CREATE TABLE IF NOT EXISTS auth.authz_elevations (
    id                 UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id          UUID NOT NULL,
    user_id            UUID NOT NULL REFERENCES auth.users(id) ON DELETE CASCADE,
    resource_type      TEXT NOT NULL CHECK (resource_type <> ''),
    max_classification TEXT NOT NULL CHECK (max_classification IN ('public', 'internal', 'confidential', 'restricted')),
    justification      TEXT NOT NULL CHECK (length(trim(justification)) > 0),
    granted_by         UUID,
    granted_at         TIMESTAMPTZ NOT NULL DEFAULT now(),
    expires_at         TIMESTAMPTZ NOT NULL,
    revoked_at         TIMESTAMPTZ,
    CHECK (expires_at > granted_at)
);

CREATE INDEX IF NOT EXISTS authz_elevations_user_idx
    ON auth.authz_elevations (tenant_id, user_id, expires_at);

CREATE TABLE IF NOT EXISTS auth.access_decision_log (
    id             BIGSERIAL PRIMARY KEY,
    decided_at     TIMESTAMPTZ NOT NULL DEFAULT now(),
    tenant_id      UUID,
    principal_kind TEXT CHECK (principal_kind IN ('user', 'service')),
    principal_id   UUID,
    resource_type  TEXT,
    resource_id    TEXT,
    action         TEXT,
    purpose        TEXT,
    elevation_id   UUID,
    decision       TEXT NOT NULL CHECK (decision IN ('permit', 'deny', 'purpose_missing')),
    reason         TEXT
);

CREATE INDEX IF NOT EXISTS access_decision_log_tenant_idx
    ON auth.access_decision_log (tenant_id, decided_at);

INSERT INTO auth.authz_role_grants (tenant_id, role, resource_type, action, max_classification) VALUES
    (NULL, 'viewer',       'catalog.service', 'read',  'internal'),
    (NULL, 'member',       'catalog.service', 'read',  'confidential'),
    (NULL, 'tenant_admin', 'catalog.service', 'read',  'restricted'),
    (NULL, 'member',       'catalog.service', 'write', 'internal'),
    (NULL, 'tenant_admin', 'catalog.service', 'write', 'restricted')
ON CONFLICT DO NOTHING;

GRANT SELECT, INSERT, UPDATE, DELETE ON
    auth.authz_role_grants, auth.authz_purpose_rules, auth.authz_elevations, auth.access_decision_log
    TO observable_auth_role;
GRANT USAGE, SELECT ON
    auth.authz_role_grants_id_seq, auth.authz_purpose_rules_id_seq, auth.access_decision_log_id_seq
    TO observable_auth_role;
