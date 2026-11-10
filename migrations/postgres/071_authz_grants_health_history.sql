-- ============================================================================
-- Migration 071: decide-access default grants for health history (phase 2 E3)
--
-- Policy data in the auth schema (owned by observable-auth).
--   health.history  request-level read of a service's transitions and interruptions,
--                   checked before any lookup (like health.assessment). Evidence
--                   contributors are still checked through health.evidence.
-- ============================================================================

INSERT INTO auth.authz_role_grants (tenant_id, role, resource_type, action, max_classification) VALUES
    (NULL, 'viewer',       'health.history', 'read', 'public'),
    (NULL, 'member',       'health.history', 'read', 'public'),
    (NULL, 'tenant_admin', 'health.history', 'read', 'public')
ON CONFLICT DO NOTHING;
