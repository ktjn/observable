-- ============================================================================
-- Migration 069: decide-access default grants for maintenance windows (phase 2 E2)
--
-- Policy data in the auth schema (owned by observable-auth).
--   health.maintenance  read: request-level, checked before any lookup (like
--                       health.assessment). write: request, approve or cancel a window
--                       for a service, classified as the service. The service's
--                       operational owner (a user) may also write; health checks that
--                       through the caller's own catalog read.
-- ============================================================================

INSERT INTO auth.authz_role_grants (tenant_id, role, resource_type, action, max_classification) VALUES
    (NULL, 'viewer',       'health.maintenance', 'read',  'public'),
    (NULL, 'member',       'health.maintenance', 'read',  'public'),
    (NULL, 'tenant_admin', 'health.maintenance', 'read',  'public'),
    (NULL, 'tenant_admin', 'health.maintenance', 'write', 'restricted')
ON CONFLICT DO NOTHING;
