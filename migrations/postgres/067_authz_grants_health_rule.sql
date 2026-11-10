-- ============================================================================
-- Migration 067: decide-access default grants for health rules (phase 2 E1)
--
-- Policy data in the auth schema (owned by observable-auth).
--   health.rule  the tenant's effective health rule. Every member role reads it; only
--                tenant_admin publishes a new revision.
-- ============================================================================

INSERT INTO auth.authz_role_grants (tenant_id, role, resource_type, action, max_classification) VALUES
    (NULL, 'viewer',       'health.rule', 'read',  'internal'),
    (NULL, 'member',       'health.rule', 'read',  'internal'),
    (NULL, 'tenant_admin', 'health.rule', 'read',  'internal'),
    (NULL, 'tenant_admin', 'health.rule', 'write', 'internal')
ON CONFLICT DO NOTHING;
